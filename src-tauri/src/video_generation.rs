use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{error, info, warn};

use crate::modules::{registry, VideoGenerationSettings};
use crate::paths;
use crate::state::AppState;
use crate::video_ingest::{self, JobWorkdir, SourceItem, SourceKind};

pub const VIDEO_MODULE_ID: &str = "output_video_generation";

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(target_os = "windows")]
fn apply_hidden_creation_flags(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn apply_hidden_creation_flags(_cmd: &mut Command) {}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoJobRequest {
    pub source_items: Vec<SourceItem>,
    /// "caption" | "slideshow" | "diagram" | "game_viz"
    pub style: String,
    /// "1920x1080" | "1080x1920" | "1080x1080"
    pub resolution: String,
    pub fps: u32,
    pub brief: Option<String>,
    pub tts: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoJobResult {
    pub job_id: String,
    pub output_path: String,
    pub duration_ms: u64,
}

/// Serial single-job lock — prevents two concurrent renders fighting over the
/// Node sidecar. Phase 4 replaces this with a proper queue if needed.
static RENDER_LOCK: Mutex<()> = Mutex::new(());

/// Render a video end-to-end. Caller is responsible for running this on a
/// background thread (not a Tauri command-pool thread) because it blocks for
/// minutes on the Node sidecar.
pub fn render_video(app: &AppHandle, mut req: VideoJobRequest) -> Result<VideoJobResult, String> {
    let _guard = RENDER_LOCK
        .lock()
        .map_err(|e| format!("render lock poisoned: {}", e))?;
    let started = std::time::Instant::now();

    validate_request(&req)?;

    let (settings, module_enabled) = snapshot_settings(app)?;
    if !module_enabled {
        return Err(
            "Video Generation module is not enabled. Open the Modules tab and enable it first."
                .to_string(),
        );
    }
    let _ = settings.enabled; // legacy field kept for forward-compat, no longer gates rendering

    let workdir = JobWorkdir::create(app)?;
    emit_progress(app, &workdir.job_id, "starting", 0.0, None);

    let cleanup_on_drop = CleanupGuard::new(&workdir);

    emit_progress(app, &workdir.job_id, "materialising_assets", 0.05, None);
    video_ingest::materialize_assets(&mut req.source_items, &workdir)?;

    emit_progress(app, &workdir.job_id, "composing", 0.1, None);
    compose_hyperframes_project(&req, &workdir)?;

    emit_progress(app, &workdir.job_id, "rendering", 0.2, None);
    let output_path = resolve_output_path(app, &settings, &workdir);
    run_hyperframes_render(app, &settings, &workdir, &output_path, req.fps)?;

    emit_progress(app, &workdir.job_id, "finalizing", 0.95, None);
    if !output_path.exists() {
        return Err(format!(
            "Render finished but output file not found at {:?}",
            output_path
        ));
    }

    let duration_ms = started.elapsed().as_millis() as u64;
    let result = VideoJobResult {
        job_id: workdir.job_id.clone(),
        output_path: output_path.to_string_lossy().to_string(),
        duration_ms,
    };

    emit_progress(app, &workdir.job_id, "done", 1.0, Some(&result.output_path));
    let _ = app.emit("video:complete", &result);

    drop(cleanup_on_drop);
    Ok(result)
}

// ---------------------------------------------------------------------------
// Validation & settings snapshot
// ---------------------------------------------------------------------------

fn validate_request(req: &VideoJobRequest) -> Result<(), String> {
    if req.source_items.is_empty() {
        return Err("At least one source item is required.".to_string());
    }
    let has_content = req
        .source_items
        .iter()
        .any(|i| matches!(i.kind, SourceKind::Content | SourceKind::Hybrid));
    if !has_content {
        return Err(
            "At least one content or hybrid source is required (all assets is not enough)."
                .to_string(),
        );
    }
    if req.fps != 30 && req.fps != 60 {
        return Err(format!("Unsupported fps: {} (expected 30 or 60).", req.fps));
    }
    match req.resolution.as_str() {
        "1920x1080" | "1080x1920" | "1080x1080" => {}
        other => return Err(format!("Unsupported resolution: {}.", other)),
    }
    match req.style.as_str() {
        "caption" | "slideshow" | "diagram" | "game_viz" => {}
        other => return Err(format!("Unsupported style: {}.", other)),
    }
    Ok(())
}

fn snapshot_settings(app: &AppHandle) -> Result<(VideoGenerationSettings, bool), String> {
    let state = app.state::<AppState>();
    let settings = state
        .settings
        .read()
        .map_err(|e| format!("settings lock poisoned: {}", e))?;
    // The module registry is the source of truth for enable state; the legacy
    // `enabled` field on VideoGenerationSettings is kept purely for forward-
    // compat with sub-feature toggles we may add later.
    let module_enabled = settings
        .module_settings
        .enabled_modules
        .iter()
        .any(|m| m == VIDEO_MODULE_ID);
    // touch registry import so the dep is explicit; registry metadata drives
    // the Modules tab UI, not this check.
    let _ = registry::find_manifest(VIDEO_MODULE_ID);
    Ok((settings.video_generation_settings.clone(), module_enabled))
}

// ---------------------------------------------------------------------------
// Static template composition (Phase 1 — no LLM)
// ---------------------------------------------------------------------------

/// Compose a minimal hyperframes project inside the workdir.
///
/// hyperframes expects a directory containing:
///   - `hyperframes.json` (config, points to registry + paths)
///   - `index.html`       (composition with required data-* attributes)
///   - `meta.json`        (project metadata)
///   - `assets/`          (media referenced via relative paths)
///
/// Asset paths inside the composition are **relative** (e.g. `assets/foo.png`),
/// not `file://` URLs. `materialize_assets` has already placed files under
/// `workdir.assets_dir` (= `<workdir>/assets/`), so the template just needs to
/// strip leading paths down to `assets/<filename>`.
fn compose_hyperframes_project(req: &VideoJobRequest, workdir: &JobWorkdir) -> Result<(), String> {
    let template = load_template_source(&req.style)?;
    let (width, height) = parse_resolution(&req.resolution)?;

    // Rewrite asset_path entries to be relative to the project dir.
    let mut items_for_template = req.source_items.clone();
    for item in items_for_template.iter_mut() {
        if let Some(abs) = item.asset_path.as_deref() {
            let abs_path = PathBuf::from(abs);
            if let Ok(rel) = abs_path.strip_prefix(&workdir.root) {
                let normalized = rel.to_string_lossy().replace('\\', "/");
                item.asset_path = Some(normalized);
            }
        }
    }

    let items_json = serde_json::to_string(&items_for_template)
        .map_err(|e| format!("serialize source items: {}", e))?;
    let brief = req.brief.clone().unwrap_or_default();
    let brief_escaped =
        serde_json::to_string(&brief).map_err(|e| format!("serialize brief: {}", e))?;

    // Duration heuristic: slideshow = 4.5s per content item; caption = transcript
    // length / 3 words-per-second (min 5s, max 60s).
    let duration_seconds = compute_duration_seconds(req);

    let rendered = template
        .replace("{{ITEMS_JSON}}", &items_json)
        .replace("{{BRIEF_JSON}}", &brief_escaped)
        .replace("{{WIDTH}}", &width.to_string())
        .replace("{{HEIGHT}}", &height.to_string())
        .replace("{{DURATION}}", &duration_seconds.to_string())
        .replace("{{STYLE}}", &req.style);

    fs::write(workdir.root.join("index.html"), rendered)
        .map_err(|e| format!("write index.html: {}", e))?;

    fs::write(
        workdir.root.join("hyperframes.json"),
        r#"{
  "$schema": "https://hyperframes.heygen.com/schema/hyperframes.json",
  "registry": "https://raw.githubusercontent.com/heygen-com/hyperframes/main/registry",
  "paths": {
    "blocks": "compositions",
    "components": "compositions/components",
    "assets": "assets"
  }
}
"#,
    )
    .map_err(|e| format!("write hyperframes.json: {}", e))?;

    let meta = serde_json::json!({
        "id": workdir.job_id,
        "name": workdir.job_id,
        "createdAt": chrono::Utc::now().to_rfc3339(),
    });
    fs::write(
        workdir.root.join("meta.json"),
        serde_json::to_string_pretty(&meta).unwrap_or_else(|_| "{}".to_string()),
    )
    .map_err(|e| format!("write meta.json: {}", e))?;

    info!(
        "[video-gen] composed hyperframes project at {:?} ({} items, style={}, duration={}s)",
        workdir.root,
        req.source_items.len(),
        req.style,
        duration_seconds
    );
    Ok(())
}

fn compute_duration_seconds(req: &VideoJobRequest) -> u32 {
    let content_count = req
        .source_items
        .iter()
        .filter(|i| matches!(i.kind, SourceKind::Content | SourceKind::Hybrid))
        .count();
    match req.style.as_str() {
        "caption" => {
            let words: usize = req
                .source_items
                .iter()
                .filter_map(|i| i.extracted_text.as_ref())
                .map(|t| t.split_whitespace().count())
                .sum();
            ((words as f32 / 3.0).ceil() as u32).clamp(5, 60)
        }
        _ => (content_count as u32 * 5).clamp(5, 120),
    }
}

fn parse_resolution(res: &str) -> Result<(u32, u32), String> {
    let (w, h) = res
        .split_once('x')
        .ok_or_else(|| format!("invalid resolution '{}': missing 'x' separator", res))?;
    let width: u32 = w
        .parse()
        .map_err(|_| format!("invalid width in resolution '{}'", res))?;
    let height: u32 = h
        .parse()
        .map_err(|_| format!("invalid height in resolution '{}'", res))?;
    Ok((width, height))
}

/// Embedded templates shipped in the binary. Keeps Phase 1 hermetic — no file
/// lookup required, no bundling dependency on `assets/video_templates/`.
fn load_template_source(style: &str) -> Result<&'static str, String> {
    let template = match style {
        "caption" => include_str!("../assets/video_templates/caption.html.tmpl"),
        "slideshow" => include_str!("../assets/video_templates/slideshow.html.tmpl"),
        // Phase 1 fallback: styles not yet implemented fall back to slideshow.
        "diagram" | "game_viz" => {
            warn!(
                "[video-gen] style '{}' is a Phase 4 target, falling back to slideshow.",
                style
            );
            include_str!("../assets/video_templates/slideshow.html.tmpl")
        }
        other => return Err(format!("no template available for style '{}'", other)),
    };
    Ok(template)
}

// ---------------------------------------------------------------------------
// Output path resolution
// ---------------------------------------------------------------------------

fn resolve_output_path(
    app: &AppHandle,
    settings: &VideoGenerationSettings,
    workdir: &JobWorkdir,
) -> PathBuf {
    let base_dir = if settings.output_dir.is_empty() {
        paths::resolve_video_output_dir(app)
    } else {
        let custom = PathBuf::from(&settings.output_dir);
        let _ = fs::create_dir_all(&custom);
        custom
    };
    let filename = format!("{}_{}.mp4", workdir.job_id, Utc::now().format("%H%M%S"));
    base_dir.join(filename)
}

// ---------------------------------------------------------------------------
// Node + hyperframes spawn
// ---------------------------------------------------------------------------

fn run_hyperframes_render(
    app: &AppHandle,
    settings: &VideoGenerationSettings,
    workdir: &JobWorkdir,
    output_path: &Path,
    fps: u32,
) -> Result<(), String> {
    // First attempt honors the user's GPU preference. If GPU-encoding fails
    // (FFmpeg exits with an encoder-related error — common when the driver/
    // hardware for NVENC/AMF/QSV isn't actually present), retry once with GPU
    // disabled so the user still gets a working MP4.
    let result = run_hyperframes_render_inner(
        app,
        settings,
        workdir,
        output_path,
        fps,
        settings.gpu_encoding,
    );
    if let Err(e) = &result {
        if settings.gpu_encoding && looks_like_gpu_encode_failure(e) {
            warn!(
                "[video-gen] GPU encoding failed ({}). Retrying with CPU encoding.",
                e.lines().next().unwrap_or("").trim()
            );
            emit_progress(
                app,
                &workdir.job_id,
                "rendering",
                0.3,
                Some("GPU encoding failed — retrying with CPU. Check your NVIDIA/AMF/QSV drivers."),
            );
            return run_hyperframes_render_inner(app, settings, workdir, output_path, fps, false);
        }
    }
    result
}

fn looks_like_gpu_encode_failure(err: &str) -> bool {
    // hyperframes surfaces FFmpeg failures with "Encoding failed" and an exit
    // code. Without a reliable way to distinguish GPU vs CPU failures from the
    // message alone, we treat any encoding-phase failure as a candidate for
    // the CPU-fallback retry — safe because the retry ALSO disables GPU, so
    // a genuine CPU-encode bug still bubbles up after the second attempt.
    let lower = err.to_ascii_lowercase();
    lower.contains("encoding failed") || lower.contains("ffmpeg exited with code")
}

fn run_hyperframes_render_inner(
    app: &AppHandle,
    settings: &VideoGenerationSettings,
    workdir: &JobWorkdir,
    output_path: &Path,
    fps: u32,
    use_gpu: bool,
) -> Result<(), String> {
    let node_binary = if settings.node_binary_path.is_empty() {
        paths::resolve_node_binary_path()
            .ok_or_else(|| "Node binary not found: bundle missing at bin/node/ and no system Node on PATH. Install Node 22+ or configure settings.video_generation.node_binary_path.".to_string())?
    } else {
        let p = PathBuf::from(&settings.node_binary_path);
        if !p.exists() {
            return Err(format!(
                "Configured node_binary_path does not exist: {}",
                settings.node_binary_path
            ));
        }
        p
    };

    let hyperframes_cwd = if settings.hyperframes_cwd.is_empty() {
        paths::resolve_hyperframes_cwd()
            .ok_or_else(|| "hyperframes install not found: bundle missing at bin/hyperframes/. Run the Node sidecar bundling step.".to_string())?
    } else {
        let p = PathBuf::from(&settings.hyperframes_cwd);
        if !p.join("package.json").exists() {
            return Err(format!(
                "Configured hyperframes_cwd has no package.json: {}",
                settings.hyperframes_cwd
            ));
        }
        p
    };

    let entry_script = hyperframes_cwd.join("node_modules/hyperframes/dist/cli.js");
    if !entry_script.exists() {
        return Err(format!(
            "hyperframes CLI entry not found at {:?} — did `npm install` run in {:?}?",
            entry_script, hyperframes_cwd
        ));
    }

    // hyperframes render signature:  render [OPTIONS] [DIR]
    //   DIR           = project directory (contains hyperframes.json + index.html)
    //   --output      = absolute output path for the produced MP4
    //   --fps         = 24 | 30 | 60
    //   --quality     = draft | standard | high
    //   --workers     = number (1..16) or "auto"
    //   --gpu         = use GPU-accelerated encoding (NVENC/AMF/QSV)
    // NOTE: hyperframes manages its own ffmpeg/chrome — do NOT pass --ffmpeg-path
    // (that flag does not exist). On a fresh install, `hyperframes browser
    // ensure` must have been run once to download Chrome Headless Shell.
    let mut cmd = Command::new(&node_binary);
    cmd.arg(&entry_script)
        .arg("render")
        .arg("--output")
        .arg(output_path)
        .arg("--fps")
        .arg(fps.to_string());

    // Quality: only pass if non-default to keep the command line minimal.
    if settings.render_quality != "standard" && !settings.render_quality.is_empty() {
        cmd.arg("--quality").arg(&settings.render_quality);
    }

    // Workers: 0 = let hyperframes auto-pick; any positive value is capped in normalize.
    if settings.render_workers > 0 {
        cmd.arg("--workers")
            .arg(settings.render_workers.to_string());
    }

    // GPU encoding (FFmpeg NVENC/AMF/QSV). Hard-fails if drivers/hardware are
    // not available — the caller wraps this in a CPU-fallback retry.
    if use_gpu {
        cmd.arg("--gpu");
    }

    cmd.arg(&workdir.root);
    cmd.current_dir(&workdir.root);
    apply_hidden_creation_flags(&mut cmd);

    info!(
        "[video-gen] spawning node={} cli={} project={} output={} fps={} quality={} workers={} gpu={}",
        node_binary.display(),
        entry_script.display(),
        workdir.root.display(),
        output_path.display(),
        fps,
        settings.render_quality,
        if settings.render_workers == 0 { "auto".to_string() } else { settings.render_workers.to_string() },
        use_gpu
    );

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn node+hyperframes: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "hyperframes: no stdout pipe".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "hyperframes: no stderr pipe".to_string())?;

    let app_stdout = app.clone();
    let job_id_stdout = workdir.job_id.clone();
    let stdout_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            info!("[hyperframes:out] {}", line);
            emit_progress(&app_stdout, &job_id_stdout, "rendering", 0.5, Some(&line));
        }
    });

    let app_stderr = app.clone();
    let job_id_stderr = workdir.job_id.clone();
    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut tail: Vec<String> = Vec::new();
        for line in reader.lines().flatten() {
            warn!("[hyperframes:err] {}", line);
            tail.push(line.clone());
            if tail.len() > 40 {
                tail.remove(0);
            }
            emit_progress(
                &app_stderr,
                &job_id_stderr,
                "rendering",
                0.5,
                Some(&format!("stderr: {}", line)),
            );
        }
        tail
    });

    let status = child
        .wait()
        .map_err(|e| format!("wait hyperframes: {}", e))?;
    let _ = stdout_thread.join();
    let stderr_tail = stderr_thread.join().unwrap_or_default();

    if !status.success() {
        let tail_joined = stderr_tail.join("\n");
        return Err(format!(
            "hyperframes exited with {:?}\n---- stderr tail ----\n{}",
            status.code(),
            tail_joined
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Progress events
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ProgressPayload<'a> {
    job_id: &'a str,
    phase: &'a str,
    progress: f32,
    message: Option<&'a str>,
}

fn emit_progress(app: &AppHandle, job_id: &str, phase: &str, progress: f32, message: Option<&str>) {
    let payload = ProgressPayload {
        job_id,
        phase,
        progress,
        message,
    };
    if let Err(e) = app.emit("video:progress", &payload) {
        error!("[video-gen] failed to emit progress: {}", e);
    }
}

// ---------------------------------------------------------------------------
// Workdir cleanup
// ---------------------------------------------------------------------------

struct CleanupGuard<'a> {
    workdir: &'a JobWorkdir,
    armed: bool,
}

impl<'a> CleanupGuard<'a> {
    fn new(workdir: &'a JobWorkdir) -> Self {
        Self {
            workdir,
            armed: true,
        }
    }
}

impl<'a> Drop for CleanupGuard<'a> {
    fn drop(&mut self) {
        if self.armed {
            self.workdir.cleanup();
        }
    }
}
