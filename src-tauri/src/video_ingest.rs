use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, warn};

use crate::history_partition::PartitionedHistory;
use crate::paths;
use crate::state::{AppState, HistoryEntry};

static SOURCE_ITEM_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// Pure text content — consumed by the composer, not embedded as a file.
    Content,
    /// Binary asset — embedded/referenced in the composition, not analysed.
    Asset,
    /// Both: may be embedded AND have extracted text (e.g. video with transcript).
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceItem {
    pub id: String,
    pub kind: SourceKind,
    pub display_name: String,
    pub original_path: Option<String>,
    pub extracted_text: Option<String>,
    pub asset_path: Option<String>,
    pub metadata: serde_json::Value,
    pub order: usize,
}

/// Isolated workspace for a single video job — lives under video_jobs/<job_id>/.
pub struct JobWorkdir {
    pub job_id: String,
    pub root: PathBuf,
    pub assets_dir: PathBuf,
}

impl JobWorkdir {
    pub fn create(app: &tauri::AppHandle) -> Result<Self, String> {
        let now = Utc::now();
        let job_id = format!(
            "job_{}_{}",
            now.format("%Y%m%d_%H%M%S"),
            SOURCE_ITEM_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let root = paths::resolve_video_jobs_dir(app).join(&job_id);
        let assets_dir = root.join("assets");
        fs::create_dir_all(&assets_dir).map_err(|e| format!("create job workdir: {}", e))?;
        Ok(Self {
            job_id,
            root,
            assets_dir,
        })
    }

    /// Best-effort cleanup. Safe to call repeatedly — missing dir is not an error.
    pub fn cleanup(&self) {
        if self.root.exists() {
            if let Err(e) = fs::remove_dir_all(&self.root) {
                warn!("Failed to clean job workdir {:?}: {}", self.root, e);
            }
        }
    }
}

fn next_source_id() -> String {
    let counter = SOURCE_ITEM_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now_nanos = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_millis() * 1_000_000);
    format!("src_{}_{}", now_nanos, counter)
}

fn extension_lower(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

fn classify(extension: &str) -> Option<SourceKind> {
    match extension {
        "md" | "txt" | "json" | "yaml" | "yml" | "html" | "htm" | "srt" | "vtt" => {
            Some(SourceKind::Content)
        }
        "svg" => Some(SourceKind::Asset),
        "png" | "jpg" | "jpeg" | "webp" | "gif" => Some(SourceKind::Hybrid),
        "mp3" | "wav" | "m4a" | "ogg" => Some(SourceKind::Hybrid),
        "mp4" | "mov" | "webm" | "mkv" => Some(SourceKind::Hybrid),
        _ => None,
    }
}

fn read_text_utf8_lossy(path: &Path, max_bytes: usize) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {:?}: {}", path, e))?;
    let truncated = if bytes.len() > max_bytes {
        &bytes[..max_bytes]
    } else {
        &bytes[..]
    };
    Ok(String::from_utf8_lossy(truncated).to_string())
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn copy_to_assets(path: &Path, workdir: &JobWorkdir) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("no filename in {:?}", path))?;
    let dest = workdir.assets_dir.join(file_name);
    let dest_final = if dest.exists() {
        let counter = SOURCE_ITEM_COUNTER.fetch_add(1, Ordering::Relaxed);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("asset");
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{}", s))
            .unwrap_or_default();
        workdir
            .assets_dir
            .join(format!("{}_{}{}", stem, counter, ext))
    } else {
        dest
    };
    fs::copy(path, &dest_final)
        .map_err(|e| format!("copy {:?} -> {:?}: {}", path, dest_final, e))?;
    Ok(dest_final)
}

fn check_size(path: &Path, max_bytes: u64) -> Result<u64, String> {
    let meta = fs::metadata(path).map_err(|e| format!("stat {:?}: {}", path, e))?;
    let size = meta.len();
    if size > max_bytes {
        return Err(format!(
            "file {:?} is {} MB, exceeds limit {} MB",
            path.file_name().unwrap_or_default(),
            size / (1024 * 1024),
            max_bytes / (1024 * 1024)
        ));
    }
    Ok(size)
}

/// Ingest a single file-system path into a SourceItem. Unknown extensions return Err.
pub fn ingest_path(
    path: &Path,
    workdir: &JobWorkdir,
    order: usize,
    max_upload_mb: u32,
) -> Result<SourceItem, String> {
    if !path.exists() {
        return Err(format!("path does not exist: {:?}", path));
    }
    if !path.is_file() {
        return Err(format!("not a regular file: {:?}", path));
    }
    let max_bytes = u64::from(max_upload_mb) * 1024 * 1024;
    let size = check_size(path, max_bytes)?;

    let ext = extension_lower(path);
    let kind = classify(&ext).ok_or_else(|| {
        format!(
            "unsupported format: .{} ({})",
            if ext.is_empty() { "<none>" } else { &ext },
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })?;

    let display_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed")
        .to_string();
    let original_path = Some(path.to_string_lossy().to_string());

    let mut extracted_text: Option<String> = None;
    let mut asset_path: Option<String> = None;
    let mut metadata = serde_json::json!({
        "size_bytes": size,
        "extension": ext,
    });

    // Phase 1 ingest is read-only: no copy to a workdir. The render layer
    // materialises assets into its own workdir when the job actually starts.
    // This means original files must still exist at render time — acceptable
    // for interactive sessions; Phase 4 can introduce a staging cache.
    let _ = workdir; // kept in signature for future staging-cache work

    match kind {
        SourceKind::Content => {
            let raw = read_text_utf8_lossy(path, 2 * 1024 * 1024)?;
            let text = match ext.as_str() {
                "html" | "htm" => strip_html_tags(&raw),
                _ => raw,
            };
            extracted_text = Some(text);
        }
        SourceKind::Asset => {
            asset_path = Some(path.to_string_lossy().to_string());
        }
        SourceKind::Hybrid => {
            // Phase 1: embed as asset only; Vision/Whisper extraction in Phase 3.
            asset_path = Some(path.to_string_lossy().to_string());
            metadata["pending_text_extraction"] = serde_json::Value::Bool(true);
        }
    }

    debug!(
        "ingested {:?} as {:?} (size {} bytes)",
        path.file_name(),
        kind,
        size
    );

    Ok(SourceItem {
        id: next_source_id(),
        kind,
        display_name,
        original_path,
        extracted_text,
        asset_path,
        metadata,
        order,
    })
}

/// Copy all Asset/Hybrid items into the render workdir so the job is isolated
/// from the user's filesystem mutations between ingest and render. Updates
/// each item's `asset_path` in place to the workdir copy.
pub fn materialize_assets(items: &mut Vec<SourceItem>, workdir: &JobWorkdir) -> Result<(), String> {
    for item in items.iter_mut() {
        if !matches!(item.kind, SourceKind::Asset | SourceKind::Hybrid) {
            continue;
        }
        let src = match item.asset_path.as_deref() {
            Some(p) => PathBuf::from(p),
            None => continue,
        };
        if !src.exists() {
            return Err(format!(
                "Asset missing at render time: {} (was it moved or deleted?)",
                src.display()
            ));
        }
        let copied = copy_to_assets(&src, workdir)?;
        item.asset_path = Some(copied.to_string_lossy().to_string());
    }
    Ok(())
}

/// Ingest a list of dropped paths. Errors on individual items are collected, not fatal.
pub struct IngestOutcome {
    pub items: Vec<SourceItem>,
    pub errors: Vec<String>,
}

pub fn ingest_dropped_paths(
    paths: Vec<PathBuf>,
    workdir: &JobWorkdir,
    start_order: usize,
    max_upload_mb: u32,
) -> IngestOutcome {
    let mut items = Vec::with_capacity(paths.len());
    let mut errors = Vec::new();
    for (idx, path) in paths.into_iter().enumerate() {
        match ingest_path(&path, workdir, start_order + idx, max_upload_mb) {
            Ok(item) => items.push(item),
            Err(e) => errors.push(e),
        }
    }
    IngestOutcome { items, errors }
}

/// Pull a transcript out of the PartitionedHistory and wrap it as a Content SourceItem.
pub fn ingest_history_entry(
    entry_id: &str,
    state: &AppState,
    order: usize,
) -> Result<SourceItem, String> {
    let history = state
        .history_transcribe
        .lock()
        .map_err(|e| format!("history lock poisoned: {}", e))?;
    let entry = find_history_entry(&history, entry_id)
        .ok_or_else(|| format!("history entry not found: {}", entry_id))?;

    Ok(SourceItem {
        id: next_source_id(),
        kind: SourceKind::Content,
        display_name: format!(
            "Transcript {}",
            &entry_id.chars().take(8).collect::<String>()
        ),
        original_path: None,
        extracted_text: Some(entry.text),
        asset_path: None,
        metadata: serde_json::json!({
            "source": "history",
            "entry_id": entry_id,
            "timestamp_ms": entry.timestamp_ms,
        }),
        order,
    })
}

fn find_history_entry(history: &PartitionedHistory, entry_id: &str) -> Option<HistoryEntry> {
    history
        .active
        .iter()
        .find(|entry| entry.id == entry_id)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_workdir() -> (JobWorkdir, tempdir_stub::TempDir) {
        let tmp = tempdir_stub::TempDir::new();
        let root = tmp.path().join("job_test");
        let assets_dir = root.join("assets");
        fs::create_dir_all(&assets_dir).unwrap();
        (
            JobWorkdir {
                job_id: "job_test".to_string(),
                root,
                assets_dir,
            },
            tmp,
        )
    }

    #[test]
    fn md_file_extracts_text() {
        let (workdir, _tmp) = make_workdir();
        let src = workdir.root.join("note.md");
        let mut f = fs::File::create(&src).unwrap();
        f.write_all(b"# Title\n\nHello world.").unwrap();

        let item = ingest_path(&src, &workdir, 0, 500).unwrap();
        assert_eq!(item.kind, SourceKind::Content);
        assert!(item
            .extracted_text
            .as_deref()
            .unwrap_or("")
            .contains("Hello world"));
        assert!(item.asset_path.is_none());
    }

    #[test]
    fn unknown_extension_errors() {
        let (workdir, _tmp) = make_workdir();
        let src = workdir.root.join("blob.xyz");
        fs::write(&src, b"nope").unwrap();

        let err = ingest_path(&src, &workdir, 0, 500).unwrap_err();
        assert!(err.contains("unsupported format"), "got: {}", err);
    }

    #[test]
    fn html_strips_tags() {
        let (workdir, _tmp) = make_workdir();
        let src = workdir.root.join("page.html");
        fs::write(&src, b"<p>Hello <b>World</b></p>").unwrap();

        let item = ingest_path(&src, &workdir, 0, 500).unwrap();
        let text = item.extracted_text.unwrap();
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn size_limit_rejects_oversize() {
        let (workdir, _tmp) = make_workdir();
        let src = workdir.root.join("big.md");
        fs::write(&src, vec![b'x'; 2 * 1024 * 1024]).unwrap();

        let err = ingest_path(&src, &workdir, 0, 1).unwrap_err();
        assert!(err.contains("exceeds limit"), "got: {}", err);
    }

    /// Tiny self-contained TempDir replacement — we intentionally avoid adding
    /// the `tempfile` crate to the dependency tree for a single test helper.
    mod tempdir_stub {
        use std::path::{Path, PathBuf};

        pub struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            pub fn new() -> Self {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let path = std::env::temp_dir().join(format!("trispr_ingest_test_{}", ts));
                std::fs::create_dir_all(&path).unwrap();
                Self { path }
            }

            pub fn path(&self) -> &Path {
                &self.path
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }
}
