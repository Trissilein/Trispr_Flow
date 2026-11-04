// OPUS audio export — thin client over the `trispr-opus` module sidecar.
//
// All FFmpeg/libopus invocation now lives in the `trispr-opus` sidecar shipped
// inside the on-demand `opus` module package (see `module-sidecars/opus/`). The
// core no longer knows how to find or drive FFmpeg; it resolves the installed
// sidecar binary, hands it file paths, and parses its JSON result. When the
// module is not installed, callers treat opus export as a no-op.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::AppHandle;

/// Module id of the opus export sidecar package (`modules/opus/`).
pub const OPUS_MODULE_ID: &str = "opus";

/// Result of an OPUS encoding operation (returned by the sidecar as JSON).
#[derive(Serialize, Deserialize, Clone)]
pub struct OpusEncodeResult {
    pub output_path: String,
    pub input_size_bytes: u64,
    pub output_size_bytes: u64,
    pub compression_ratio: f32,
    pub duration_ms: u64,
}

/// OPUS encoder configuration handed to the sidecar.
#[derive(Clone)]
pub struct OpusEncoderConfig {
    pub bitrate_kbps: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub compression_level: u32,
    pub vbr_enabled: bool,
    pub application: OpusApplication,
}

/// OPUS application mode.
#[derive(Clone)]
pub enum OpusApplication {
    Voip,
}

impl Default for OpusEncoderConfig {
    fn default() -> Self {
        Self {
            bitrate_kbps: 64,
            sample_rate: 16000,
            channels: 1,
            compression_level: 10,
            vbr_enabled: true,
            application: OpusApplication::Voip,
        }
    }
}

impl OpusApplication {
    fn as_str(&self) -> &str {
        match self {
            OpusApplication::Voip => "voip",
        }
    }
}

/// Relative path of the sidecar binary inside the installed module package.
fn entrypoint_rel() -> &'static str {
    if cfg!(windows) {
        "bin/trispr-opus.exe"
    } else {
        "bin/trispr-opus"
    }
}

/// Resolve the installed opus sidecar binary via an `AppHandle`, or `None` if
/// the `opus` module is not installed.
pub fn resolve_sidecar(app: &AppHandle) -> Option<PathBuf> {
    let bin = crate::modules::runtime::resolve_module_binary(app, OPUS_MODULE_ID, entrypoint_rel());
    bin.exists().then_some(bin)
}

/// Resolve the installed opus sidecar binary from a known modules directory.
/// For callers without an `AppHandle` (e.g. the session manager singleton).
pub fn resolve_sidecar_in(modules_dir: &Path) -> Option<PathBuf> {
    let bin = modules_dir.join(OPUS_MODULE_ID).join(entrypoint_rel());
    bin.exists().then_some(bin)
}

fn no_window(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let _ = cmd;
}

/// Encode a WAV file to OPUS by invoking the sidecar `encode` subcommand.
pub fn encode_with_sidecar(
    sidecar: &Path,
    input: &Path,
    output: &Path,
    config: &OpusEncoderConfig,
) -> Result<OpusEncodeResult, String> {
    let mut cmd = Command::new(sidecar);
    no_window(&mut cmd);
    cmd.arg("encode")
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(output)
        .arg("--bitrate")
        .arg(config.bitrate_kbps.to_string())
        .arg("--sample-rate")
        .arg(config.sample_rate.to_string())
        .arg("--channels")
        .arg(config.channels.to_string())
        .arg("--compression")
        .arg(config.compression_level.to_string())
        .arg("--vbr")
        .arg(if config.vbr_enabled { "on" } else { "off" })
        .arg("--application")
        .arg(config.application.as_str())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let out = cmd
        .output()
        .map_err(|e| format!("Failed to run opus sidecar: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("opus sidecar encode failed: {stderr}"));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str::<OpusEncodeResult>(stdout.trim())
        .map_err(|e| format!("Failed to parse opus sidecar output: {e}; raw: {stdout}"))
}

/// Merge a list of OPUS files into one via the sidecar `concat` subcommand.
/// `list` is the concat manifest path; `cwd` (if set) is the working directory
/// FFmpeg resolves relative entries against.
pub fn concat_with_sidecar(
    sidecar: &Path,
    list: &Path,
    output: &Path,
    cwd: Option<&Path>,
) -> Result<(), String> {
    let mut cmd = Command::new(sidecar);
    no_window(&mut cmd);
    cmd.arg("concat")
        .arg("--list")
        .arg(list)
        .arg("--output")
        .arg(output);
    if let Some(dir) = cwd {
        cmd.arg("--cwd").arg(dir);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());

    let out = cmd
        .output()
        .map_err(|e| format!("Failed to run opus sidecar: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("opus sidecar concat failed: {stderr}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_sidecar_encode_json() {
        // Contract with `module-sidecars/opus` `encode` output.
        let raw = r#"{"output_path":"C:\\rec\\a.opus","input_size_bytes":64078,"output_size_bytes":18223,"compression_ratio":0.284388,"duration_ms":37}"#;
        let result: OpusEncodeResult = serde_json::from_str(raw).expect("encode json parses");
        assert_eq!(result.output_path, "C:\\rec\\a.opus");
        assert_eq!(result.input_size_bytes, 64078);
        assert_eq!(result.output_size_bytes, 18223);
        assert!((result.compression_ratio - 0.284388).abs() < 1e-5);
        assert_eq!(result.duration_ms, 37);
    }

    #[test]
    fn resolve_sidecar_in_returns_none_when_absent() {
        let dir = std::env::temp_dir().join("trispr_opus_resolve_absent");
        let _ = fs::remove_dir_all(&dir);
        assert!(resolve_sidecar_in(&dir).is_none());
    }

    #[test]
    fn resolve_sidecar_in_finds_installed_binary() {
        let dir = std::env::temp_dir().join("trispr_opus_resolve_present");
        let bin = dir.join(OPUS_MODULE_ID).join(entrypoint_rel());
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(&bin, b"stub").unwrap();
        assert_eq!(resolve_sidecar_in(&dir), Some(bin));
        let _ = fs::remove_dir_all(&dir);
    }
}
