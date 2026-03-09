use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tracing::warn;

/// Returns the single canonical base directory for all Trispr Flow data.
///
/// Windows default: `%LOCALAPPDATA%\Trispr Flow\`
/// Override: set `TRISPR_DATA_DIR` env var for dev/testing.
pub(crate) fn resolve_base_dir(app: &AppHandle) -> PathBuf {
  if let Ok(p) = std::env::var("TRISPR_DATA_DIR") {
    let path = PathBuf::from(p);
    let _ = fs::create_dir_all(&path);
    return path;
  }
  // app_local_data_dir() returns %LOCALAPPDATA%\{identifier}; parent() gives %LOCALAPPDATA%
  if let Some(path) = app
    .path()
    .app_local_data_dir()
    .ok()
    .and_then(|p| p.parent().map(|parent| parent.join("Trispr Flow")))
  {
    return path;
  }

  // Fallback: read LOCALAPPDATA env var directly (Windows)
  if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
    return PathBuf::from(local_app_data).join("Trispr Flow");
  }

  warn!("Could not resolve %LOCALAPPDATA%, falling back to current directory");
  std::env::current_dir().unwrap_or_else(|e| {
    warn!("current_dir also failed, falling back to \".\": {}", e);
    PathBuf::from(".")
  })
}

pub(crate) fn resolve_config_path(app: &AppHandle, filename: &str) -> PathBuf {
  let base = resolve_base_dir(app);
  let _ = fs::create_dir_all(&base);
  base.join(filename)
}

pub(crate) fn resolve_data_path(app: &AppHandle, filename: &str) -> PathBuf {
  let base = resolve_base_dir(app);
  let _ = fs::create_dir_all(&base);
  base.join(filename)
}

pub(crate) fn resolve_recordings_dir(app: &AppHandle) -> PathBuf {
  let dir = resolve_base_dir(app).join("recordings");
  let _ = fs::create_dir_all(&dir);
  dir
}

pub(crate) fn resolve_models_dir(app: &AppHandle) -> PathBuf {
  if let Ok(dir) = std::env::var("TRISPR_WHISPER_MODEL_DIR") {
    let trimmed = dir.trim();
    if !trimmed.is_empty() {
      let path = PathBuf::from(trimmed);
      if fs::create_dir_all(&path).is_ok() {
        return path;
      }
    }
  }
  let dir = resolve_base_dir(app).join("models");
  let _ = fs::create_dir_all(&dir);
  dir
}

pub(crate) fn resolve_whisper_cli_path() -> Option<PathBuf> {
  // 1. Explicit env var override
  if let Ok(path) = std::env::var("TRISPR_WHISPER_CLI") {
    let candidate = PathBuf::from(path);
    if candidate.exists() {
      return Some(candidate);
    }
  }

  let mut candidates = Vec::new();

  // 2. Next to our own executable (installed app)
  //    Installer places binaries in bin/cuda/ or bin/vulkan/ based on GPU choice.
  if let Ok(exe) = std::env::current_exe() {
    if let Some(exe_dir) = exe.parent() {
      // Backend subdirectories (installed app)
      for backend in &["cuda", "vulkan"] {
        candidates.push(exe_dir.join(format!("bin/{}/whisper-cli.exe", backend)));
        candidates.push(exe_dir.join(format!("bin/{}/whisper-cli", backend)));
      }
      // Flat layout fallback
      candidates.push(exe_dir.join("bin/whisper-cli.exe"));
      candidates.push(exe_dir.join("bin/whisper-cli"));
      candidates.push(exe_dir.join("whisper-cli.exe"));
      candidates.push(exe_dir.join("whisper-cli"));
    }
  }

  // 3. Relative to CWD (dev mode)
  if let Ok(cwd) = std::env::current_dir() {
    // Preferred dev locations in this repository.
    for backend in &["cuda", "vulkan"] {
      candidates.push(cwd.join(format!("src-tauri/bin/{}/whisper-cli.exe", backend)));
      candidates.push(cwd.join(format!("src-tauri/bin/{}/whisper-cli", backend)));
      candidates.push(cwd.join(format!("bin/{}/whisper-cli.exe", backend)));
      candidates.push(cwd.join(format!("bin/{}/whisper-cli", backend)));
    }
  }

  for path in candidates {
    if path.exists() {
      return Some(path);
    }
  }

  None
}

pub(crate) fn resolve_quantize_path(app: &AppHandle) -> Option<PathBuf> {
  if let Ok(path) = std::env::var("TRISPR_WHISPER_QUANTIZE") {
    let candidate = PathBuf::from(path);
    if candidate.exists() {
      return Some(candidate);
    }
  }

  let mut candidates = Vec::new();

  // 1. Bundled resources (installed app)
  if let Ok(resource_dir) = app.path().resource_dir() {
    candidates.push(resource_dir.join("bin/quantize.exe"));
    candidates.push(resource_dir.join("quantize.exe"));
  }

  // 2. Next to our own executable
  if let Ok(exe) = std::env::current_exe() {
    if let Some(exe_dir) = exe.parent() {
      candidates.push(exe_dir.join("bin/quantize.exe"));
      candidates.push(exe_dir.join("quantize.exe"));
    }
  }

  // 3. Relative to CWD (dev mode)
  if let Ok(cwd) = std::env::current_dir() {
    candidates.push(cwd.join("src-tauri/bin/quantize.exe"));
    candidates.push(cwd.join("bin/quantize.exe"));
  }

  for path in candidates {
    if path.exists() {
      return Some(path);
    }
  }

  None
}
