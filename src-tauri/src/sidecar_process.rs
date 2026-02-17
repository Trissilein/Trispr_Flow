// Sidecar Process Manager
// Handles spawning, stopping, and health-checking the Python FastAPI process

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::sidecar::SidecarClient;

/// Log a sidecar stderr line at the appropriate level.
///
/// Python's logging format: `2026-02-17 11:56:29 - name - LEVEL - message`
/// uvicorn format:           `LEVEL:     message`
/// Python warnings:          `file.py:42: DeprecationWarning: ...`
fn log_sidecar_line(line: &str) {
  let t = line.trim_start();
  let is_error = t.starts_with("ERROR:") || t.contains(" - ERROR - ") || t.contains(" - CRITICAL - ");
  let is_warn = t.starts_with("WARNING:") || t.contains(" - WARNING - ")
    || t.contains("DeprecationWarning")
    || t.contains("UserWarning");
  if is_error {
    error!("[sidecar] {}", line);
  } else if is_warn {
    warn!("[sidecar] {}", line);
  } else {
    debug!("[sidecar] {}", line);
  }
}

/// Find a Python interpreter that has the sidecar dependencies installed.
///
/// On Windows: tries `py -3.13`, `py -3.12`, `py -3.11` via the Windows py launcher
/// before falling back to generic `python`/`python3`. This avoids picking up Python
/// 3.14+ where pydantic-core and other packages may lack pre-built wheels.
fn find_suitable_python() -> PathBuf {
  // On Windows, use the py launcher to find a specific compatible version
  #[cfg(target_os = "windows")]
  if let Ok(py_launcher) = which::which("py") {
    for version in &["3.13", "3.12", "3.11"] {
      let flag = format!("-{}", version);
      if let Ok(output) = Command::new(&py_launcher)
        .args([&flag as &str, "-c", "import sys; print(sys.executable)"])
        .output()
      {
        if output.status.success() {
          let exe = String::from_utf8_lossy(&output.stdout).trim().to_string();
          if !exe.is_empty() {
            let candidate = PathBuf::from(&exe);
            if candidate.exists() {
              info!("Using Python {} at {:?}", version, candidate);
              return candidate;
            }
          }
        }
      }
    }
  }

  // Generic fallback: whatever `python` or `python3` resolves to in PATH
  which::which("python")
    .or_else(|_| which::which("python3"))
    .unwrap_or_else(|_| PathBuf::from("python"))
}

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
  /// Last stderr lines from sidecar (for error reporting)
  stderr_log: Arc<Mutex<Vec<String>>>,
}

impl SidecarProcess {
  pub fn new() -> Self {
    Self {
      child: None,
      client: SidecarClient::new(),
      python_path: None,
      sidecar_dir: None,
      stderr_log: Arc::new(Mutex::new(Vec::new())),
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
      let mut cmd = Command::new(&bundled_exe);
      cmd.current_dir(&sidecar_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
      #[cfg(target_os = "windows")]
      {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
      }
      cmd.spawn()
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
        find_suitable_python()
      });

      info!("Starting sidecar: {:?} {:?}", python_path, main_py);
      let mut cmd = Command::new(&python_path);
      cmd.arg(&main_py)
        .current_dir(&sidecar_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
      #[cfg(target_os = "windows")]
      {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
      }
      cmd.spawn()
        .map_err(|e| format!("Failed to start sidecar process: {}", e))?
    };

    info!("Sidecar process started (PID: {})", child.id());

    // Spawn background thread to read and log sidecar stderr
    let mut child = child;
    if let Some(stderr) = child.stderr.take() {
      let log = Arc::clone(&self.stderr_log);
      std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
          log_sidecar_line(&line);
          let mut log = log.lock().unwrap();
          log.push(line);
          // Keep last 50 lines
          if log.len() > 50 {
            log.remove(0);
          }
        }
      });
    }

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
      Err(e) => {
        // Collect last stderr lines to surface the real error
        let log = self.stderr_log.lock().unwrap();
        let last_lines: Vec<&str> = log.iter().rev().take(10).rev().map(|s| s.as_str()).collect();
        if last_lines.is_empty() {
          Err(format!(
            "Sidecar failed to start: {}. \
            Make sure Python dependencies are installed: \
            cd sidecar/vibevoice-asr && pip install -r requirements.txt",
            e
          ))
        } else {
          Err(format!(
            "Sidecar failed to start: {}.\nPython output:\n{}",
            e,
            last_lines.join("\n")
          ))
        }
      }
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
