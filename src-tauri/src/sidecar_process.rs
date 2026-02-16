// Sidecar Process Manager
// Handles spawning, stopping, and health-checking the Python FastAPI process

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

use crate::sidecar::SidecarClient;

/// Default sidecar port
const SIDECAR_PORT: u16 = 8765;

/// Health check interval
const HEALTH_CHECK_INTERVAL_MS: u64 = 5000;

/// Startup timeout
const STARTUP_TIMEOUT_MS: u64 = 30_000;

/// Process state
pub struct SidecarProcess {
  child: Option<Child>,
  client: SidecarClient,
  python_path: Option<PathBuf>,
  sidecar_dir: Option<PathBuf>,
}

impl SidecarProcess {
  pub fn new() -> Self {
    Self {
      child: None,
      client: SidecarClient::new(),
      python_path: None,
      sidecar_dir: None,
    }
  }

  /// Configure paths before starting
  pub fn configure(&mut self, python_path: PathBuf, sidecar_dir: PathBuf) {
    self.python_path = Some(python_path);
    self.sidecar_dir = Some(sidecar_dir);
  }

  /// Start the sidecar process
  pub fn start(&mut self) -> Result<(), String> {
    if self.is_running() {
      info!("Sidecar already running");
      return Ok(());
    }

    let sidecar_dir = self.sidecar_dir.clone().unwrap_or_else(|| {
      // Default: sidecar directory relative to exe
      std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default()
        .join("sidecar")
        .join("vibevoice-asr")
    });

    // Try bundled executable first (PyInstaller build)
    let bundled_exe = if cfg!(windows) {
      sidecar_dir.join("vibevoice-asr.exe")
    } else {
      sidecar_dir.join("vibevoice-asr")
    };

    let child = if bundled_exe.exists() {
      // Use bundled executable
      info!("Starting bundled sidecar: {:?}", bundled_exe);
      Command::new(&bundled_exe)
        .current_dir(&sidecar_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start bundled sidecar: {}", e))?
    } else {
      // Fall back to Python + main.py
      let main_py = sidecar_dir.join("main.py");
      if !main_py.exists() {
        return Err(format!(
          "Sidecar not found. Expected bundled exe at {:?} or main.py at {:?}",
          bundled_exe, main_py
        ));
      }

      let python_path = self.python_path.clone().unwrap_or_else(|| {
        which::which("python")
          .or_else(|_| which::which("python3"))
          .unwrap_or_else(|_| PathBuf::from("python"))
      });

      info!("Starting sidecar: {:?} {:?}", python_path, main_py);
      Command::new(&python_path)
        .arg(&main_py)
        .current_dir(&sidecar_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start sidecar process: {}", e))?
    };

    info!("Sidecar process started (PID: {})", child.id());
    self.child = Some(child);

    // Wait for sidecar to become ready
    self.wait_for_ready()?;

    Ok(())
  }

  /// Stop the sidecar process gracefully
  pub fn stop(&mut self) -> Result<(), String> {
    if let Some(mut child) = self.child.take() {
      info!("Stopping sidecar process (PID: {})...", child.id());

      // Try graceful kill first
      match child.kill() {
        Ok(()) => {
          // Wait for process to exit
          match child.wait() {
            Ok(status) => info!("Sidecar process exited: {}", status),
            Err(e) => warn!("Failed to wait for sidecar exit: {}", e),
          }
        }
        Err(e) => {
          warn!("Failed to kill sidecar: {}", e);
        }
      }
    }

    Ok(())
  }

  /// Check if the sidecar process is alive
  pub fn is_process_alive(&mut self) -> bool {
    if let Some(ref mut child) = self.child {
      match child.try_wait() {
        Ok(None) => true,  // Still running
        Ok(Some(_)) => {
          self.child = None;
          false  // Exited
        }
        Err(_) => false,
      }
    } else {
      false
    }
  }

  /// Check if the sidecar is running and responding
  pub fn is_running(&self) -> bool {
    self.client.is_running()
  }

  /// Wait for the sidecar to become ready
  fn wait_for_ready(&self) -> Result<(), String> {
    info!("Waiting for sidecar to become ready...");

    match self.client.wait_for_ready(STARTUP_TIMEOUT_MS) {
      Ok(health) => {
        info!(
          "Sidecar ready! GPU: {}, Model loaded: {}",
          health.gpu_available, health.model_loaded
        );
        Ok(())
      }
      Err(e) => Err(format!("Sidecar failed to start: {}", e)),
    }
  }

  /// Get the sidecar client for API calls
  pub fn client(&self) -> &SidecarClient {
    &self.client
  }

  /// Restart the sidecar
  pub fn restart(&mut self) -> Result<(), String> {
    info!("Restarting sidecar...");
    self.stop()?;
    std::thread::sleep(Duration::from_millis(1000));
    self.start()
  }
}

impl Drop for SidecarProcess {
  fn drop(&mut self) {
    if let Err(e) = self.stop() {
      error!("Failed to stop sidecar on drop: {}", e);
    }
  }
}

/// Global sidecar process manager (thread-safe)
static SIDECAR: std::sync::OnceLock<Mutex<SidecarProcess>> = std::sync::OnceLock::new();

fn get_sidecar() -> &'static Mutex<SidecarProcess> {
  SIDECAR.get_or_init(|| Mutex::new(SidecarProcess::new()))
}

/// Start the global sidecar process
pub fn start_sidecar(python_path: Option<PathBuf>, sidecar_dir: Option<PathBuf>) -> Result<(), String> {
  let mut process = get_sidecar().lock().map_err(|e| e.to_string())?;

  if let Some(p) = python_path {
    if let Some(d) = sidecar_dir {
      process.configure(p, d);
    }
  }

  process.start()
}

/// Stop the global sidecar process
pub fn stop_sidecar() -> Result<(), String> {
  let mut process = get_sidecar().lock().map_err(|e| e.to_string())?;
  process.stop()
}

/// Check if the global sidecar is running
pub fn is_sidecar_running() -> bool {
  get_sidecar()
    .lock()
    .map(|p| p.is_running())
    .unwrap_or(false)
}

/// Get a reference to the sidecar client
pub fn get_sidecar_client() -> Result<SidecarClient, String> {
  Ok(SidecarClient::new())
}
