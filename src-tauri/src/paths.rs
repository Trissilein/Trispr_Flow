use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub(crate) fn resolve_config_path(app: &AppHandle, filename: &str) -> PathBuf {
  let base = app
    .path()
    .app_config_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let _ = fs::create_dir_all(&base);
  base.join(filename)
}

pub(crate) fn resolve_data_path(app: &AppHandle, filename: &str) -> PathBuf {
  let base = app
    .path()
    .app_data_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let _ = fs::create_dir_all(&base);
  base.join(filename)
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
  let base = app
    .path()
    .app_data_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let dir = base.join("models");
  let _ = fs::create_dir_all(&dir);
  dir
}

pub(crate) fn resolve_whisper_cli_path() -> Option<PathBuf> {
  if let Ok(path) = std::env::var("TRISPR_WHISPER_CLI") {
    let candidate = PathBuf::from(path);
    if candidate.exists() {
      return Some(candidate);
    }
  }

  let mut candidates = Vec::new();
  if let Ok(cwd) = std::env::current_dir() {
    candidates.push(cwd.join("whisper-cli"));
    candidates.push(cwd.join("whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build/bin/whisper-cli"));
    candidates.push(cwd.join("../whisper.cpp/build/bin/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build/bin/Release/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cpu/bin/whisper-cli"));
    candidates.push(cwd.join("../whisper.cpp/build-cpu/bin/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cpu/bin/Release/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cuda/bin/whisper-cli"));
    candidates.push(cwd.join("../whisper.cpp/build-cuda/bin/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cuda/bin/Release/whisper-cli.exe"));
    candidates.push(cwd.join("../../whisper.cpp/build/bin/whisper-cli"));
  }

  for path in candidates {
    if path.exists() {
      return Some(path);
    }
  }

  None
}
