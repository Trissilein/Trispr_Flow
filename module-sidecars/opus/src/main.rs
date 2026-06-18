//! `trispr-opus` — opus export sidecar for Trispr Flow.
//!
//! This is the "code-out" half of the opus capability: all FFmpeg/libopus
//! invocation lives here, in a tiny standalone process, instead of in the core
//! binary. Core writes the WAV (it keeps `hound` for playback anyway) and hands
//! us a file path; we run FFmpeg and report back over stdout as a single JSON
//! line. Crash isolation is free — we are a separate process.
//!
//! Subcommands:
//!   trispr-opus encode --input X.wav --output Y.opus [--bitrate 64] [--vbr on]
//!                      [--compression 10] [--sample-rate 16000] [--channels 1]
//!                      [--application voip]
//!   trispr-opus concat --list concat.txt --output session.opus [--cwd DIR]
//!   trispr-opus probe
//!
//! Exit code 0 = success, non-zero = failure. On success a JSON object is
//! printed to stdout; on failure a human-readable message goes to stderr.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    };
    std::process::exit(code);
}

fn run(args: &[String]) -> Result<String, String> {
    let subcommand = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| usage())?;
    let opts = parse_opts(&args[1..]);
    match subcommand {
        "encode" => cmd_encode(&opts),
        "concat" => cmd_concat(&opts),
        "probe" => cmd_probe(),
        "-h" | "--help" | "help" => Ok(usage()),
        other => Err(format!("Unknown subcommand '{other}'.\n{}", usage())),
    }
}

fn usage() -> String {
    "trispr-opus <encode|concat|probe> [--key value ...]".to_string()
}

/// Parse `--key value` pairs into a map. Flags without a following value are
/// stored with an empty string.
fn parse_opts(args: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if let Some(key) = arg.strip_prefix("--") {
            let value = args.get(i + 1).cloned();
            match value {
                Some(v) if !v.starts_with("--") => {
                    map.insert(key.to_string(), v);
                    i += 2;
                }
                _ => {
                    map.insert(key.to_string(), String::new());
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }
    map
}

// --- ffmpeg resolution -----------------------------------------------------

fn ffmpeg_name() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

/// Resolve the FFmpeg binary that ships inside this module package. The sidecar
/// exe lives at `modules/opus/bin/trispr-opus(.exe)` and FFmpeg ships next to
/// it; PATH is the last-resort fallback for dev runs.
fn find_ffmpeg() -> Result<PathBuf, String> {
    let name = ffmpeg_name();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name)); // bin/ffmpeg.exe
            candidates.push(dir.join("ffmpeg").join(name)); // bin/ffmpeg/ffmpeg.exe
        }
    }
    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }
    // PATH fallback (dev convenience).
    if let Ok(path) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path.split(sep) {
            let candidate = Path::new(dir).join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Err(format!(
        "FFmpeg not found next to the opus sidecar (looked for {name}) or on PATH."
    ))
}

fn no_window(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let _ = cmd;
}

// --- subcommands -----------------------------------------------------------

fn cmd_encode(opts: &HashMap<String, String>) -> Result<String, String> {
    let input = require(opts, "input")?;
    let output = require(opts, "output")?;
    let input_path = Path::new(&input);
    let output_path = Path::new(&output);

    if !input_path.exists() {
        return Err(format!("Input file does not exist: {input}"));
    }
    let input_size = std::fs::metadata(input_path)
        .map_err(|e| format!("Failed to stat input: {e}"))?
        .len();

    let bitrate = opt_u32(opts, "bitrate", 64);
    let sample_rate = opt_u32(opts, "sample-rate", 16000);
    let channels = opt_u32(opts, "channels", 1);
    let compression = opt_u32(opts, "compression", 10);
    let vbr = opts.get("vbr").map(String::as_str).unwrap_or("on");
    let application = opts
        .get("application")
        .map(String::as_str)
        .unwrap_or("voip");

    let ffmpeg = find_ffmpeg()?;
    let start = Instant::now();

    let mut cmd = Command::new(&ffmpeg);
    no_window(&mut cmd);
    cmd.arg("-i")
        .arg(input_path)
        .arg("-y")
        .arg("-c:a")
        .arg("libopus")
        .arg("-b:a")
        .arg(format!("{bitrate}k"))
        .arg("-vbr")
        .arg(vbr)
        .arg("-compression_level")
        .arg(compression.to_string())
        .arg("-application")
        .arg(application)
        .arg("-ar")
        .arg(sample_rate.to_string())
        .arg("-ac")
        .arg(channels.to_string())
        .arg("-frame_duration")
        .arg("20")
        .arg(output_path)
        .arg("-loglevel")
        .arg("error")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let out = cmd
        .output()
        .map_err(|e| format!("Failed to execute FFmpeg: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("FFmpeg encoding failed: {stderr}"));
    }
    if !output_path.exists() {
        return Err(format!("Output file was not created: {output}"));
    }
    let output_size = std::fs::metadata(output_path)
        .map_err(|e| format!("Failed to stat output: {e}"))?
        .len();
    let ratio = if input_size > 0 {
        output_size as f64 / input_size as f64
    } else {
        0.0
    };
    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(format!(
        "{{\"output_path\":\"{}\",\"input_size_bytes\":{},\"output_size_bytes\":{},\"compression_ratio\":{:.6},\"duration_ms\":{}}}",
        json_escape(&output_path.to_string_lossy()),
        input_size,
        output_size,
        ratio,
        duration_ms
    ))
}

fn cmd_concat(opts: &HashMap<String, String>) -> Result<String, String> {
    let list = require(opts, "list")?;
    let output = require(opts, "output")?;

    let ffmpeg = find_ffmpeg()?;
    let mut cmd = Command::new(&ffmpeg);
    no_window(&mut cmd);
    if let Some(cwd) = opts.get("cwd") {
        if !cwd.is_empty() {
            cmd.current_dir(cwd);
        }
    }
    cmd.arg("-y")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(&list)
        .arg("-c")
        .arg("copy")
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let out = cmd
        .output()
        .map_err(|e| format!("Failed to execute FFmpeg: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("FFmpeg concat failed: {stderr}"));
    }
    Ok(format!(
        "{{\"output_path\":\"{}\"}}",
        json_escape(&output)
    ))
}

fn cmd_probe() -> Result<String, String> {
    let ffmpeg = match find_ffmpeg() {
        Ok(path) => path,
        Err(_) => return Ok("{\"available\":false,\"version\":\"\"}".to_string()),
    };

    // libopus encoder support
    let mut probe = Command::new(&ffmpeg);
    no_window(&mut probe);
    probe
        .arg("-hide_banner")
        .arg("-h")
        .arg("encoder=libopus")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let libopus = match probe.output() {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            o.status.success()
                && (stdout.contains("Encoder libopus") || stderr.contains("Encoder libopus"))
        }
        Err(_) => false,
    };

    // version string (first line)
    let mut version_cmd = Command::new(&ffmpeg);
    no_window(&mut version_cmd);
    version_cmd.arg("-version").stdout(Stdio::piped());
    let version = version_cmd
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default();

    Ok(format!(
        "{{\"available\":{},\"version\":\"{}\"}}",
        libopus,
        json_escape(&version)
    ))
}

// --- helpers ---------------------------------------------------------------

fn require(opts: &HashMap<String, String>, key: &str) -> Result<String, String> {
    opts.get(key)
        .filter(|v| !v.is_empty())
        .cloned()
        .ok_or_else(|| format!("Missing required argument --{key}"))
}

fn opt_u32(opts: &HashMap<String, String>, key: &str, default: u32) -> u32 {
    opts.get(key)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

/// Escape a string for embedding in a JSON string literal. Handles the cases
/// that actually occur in file paths and FFmpeg version strings.
fn json_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
