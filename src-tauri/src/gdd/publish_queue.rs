use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tracing::warn;

use crate::gdd::confluence::{ConfluencePublishRequest, ConfluencePublishResult};
use crate::gdd::{render_storage, GddDraft};
use crate::paths::resolve_data_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddPublishOrQueueRequest {
    pub draft: GddDraft,
    pub publish_request: ConfluencePublishRequest,
    pub routing_confidence: Option<f32>,
    pub routing_reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddPendingPublishJob {
    pub job_id: String,
    pub title: String,
    pub space_key: String,
    pub parent_page_id: Option<String>,
    pub target_page_id: Option<String>,
    pub created_at_iso: String,
    pub updated_at_iso: String,
    pub retry_count: u32,
    pub last_error: String,
    pub bundle_dir: String,
    pub routing_confidence: Option<f32>,
    pub routing_reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddPublishFallbackBundle {
    pub bundle_dir: String,
    pub draft_json_path: String,
    pub markdown_path: String,
    pub confluence_html_path: String,
    pub publish_request_path: String,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddPublishAttemptResult {
    pub status: String, // "published" | "queued" | "failed"
    pub publish_result: Option<ConfluencePublishResult>,
    pub queued_job: Option<GddPendingPublishJob>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GddPublishQueueStore {
    pub jobs: Vec<GddPendingPublishJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleManifest {
    created_at_iso: String,
    updated_at_iso: String,
    reason: String,
    retries: u32,
    last_error: String,
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn gdd_root_dir(app: &AppHandle) -> PathBuf {
    let root = resolve_data_path(app, "gdd");
    if let Err(error) = fs::create_dir_all(&root) {
        warn!("Failed to create gdd root directory '{}': {}", root.display(), error);
    }
    root
}

fn queue_file_path(root: &Path) -> PathBuf {
    root.join("publish-queue.json")
}

fn bundles_root_dir(root: &Path) -> PathBuf {
    root.join("bundles")
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create directory '{}' for queue persistence: {}",
                parent.display(), error
            )
        })?;
    }
    let raw = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, raw).map_err(|error| {
        format!(
            "Failed writing temporary queue file '{}': {}",
            tmp_path.display(), error
        )
    })?;
    fs::rename(&tmp_path, path).map_err(|error| {
        format!(
            "Failed committing queue file '{}' from '{}': {}",
            path.display(),
            tmp_path.display(),
            error
        )
    })?;
    Ok(())
}

fn load_queue_store_from_root(root: &Path) -> Result<GddPublishQueueStore, String> {
    let queue_path = queue_file_path(root);
    if !queue_path.exists() {
        return Ok(GddPublishQueueStore::default());
    }

    let raw = fs::read_to_string(&queue_path).map_err(|error| {
        format!(
            "Failed reading publish queue file '{}': {}",
            queue_path.display(), error
        )
    })?;

    if raw.trim().is_empty() {
        return Ok(GddPublishQueueStore::default());
    }

    serde_json::from_str(&raw).map_err(|error| {
        format!(
            "Failed parsing publish queue file '{}': {}",
            queue_path.display(), error
        )
    })
}

fn save_queue_store_to_root(root: &Path, store: &GddPublishQueueStore) -> Result<(), String> {
    let queue_path = queue_file_path(root);
    write_json_atomic(&queue_path, store)
}

fn parse_http_status_from_error(error: &str) -> Option<u16> {
    let marker = "(HTTP ";
    let start = error.find(marker)? + marker.len();
    let digits = error[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u16>().ok()
}

pub fn is_queueable_publish_error(error: &str) -> bool {
    if let Some(status) = parse_http_status_from_error(error) {
        return matches!(status, 408 | 429 | 500..=599);
    }

    let lowered = error.to_ascii_lowercase();
    [
        "timed out",
        "timeout",
        "dns",
        "connection",
        "network",
        "transport",
        "temporarily unavailable",
        "service unavailable",
        "connection refused",
        "connection reset",
        "no route to host",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

pub fn list_pending_jobs(app: &AppHandle) -> Result<Vec<GddPendingPublishJob>, String> {
    let root = gdd_root_dir(app);
    let mut jobs = load_queue_store_from_root(&root)?.jobs;
    jobs.sort_by(|a, b| b.created_at_iso.cmp(&a.created_at_iso));
    Ok(jobs)
}

pub fn load_pending_job(app: &AppHandle, job_id: &str) -> Result<Option<GddPendingPublishJob>, String> {
    let root = gdd_root_dir(app);
    let store = load_queue_store_from_root(&root)?;
    Ok(store
        .jobs
        .into_iter()
        .find(|job| job.job_id == job_id.trim()))
}

pub fn persist_pending_job(app: &AppHandle, job: &GddPendingPublishJob) -> Result<(), String> {
    let root = gdd_root_dir(app);
    let mut store = load_queue_store_from_root(&root)?;
    store.jobs.retain(|entry| entry.job_id != job.job_id);
    store.jobs.push(job.clone());
    save_queue_store_to_root(&root, &store)
}

pub fn delete_pending_job(app: &AppHandle, job_id: &str) -> Result<bool, String> {
    let root = gdd_root_dir(app);
    let mut store = load_queue_store_from_root(&root)?;
    let before = store.jobs.len();
    let removed = store
        .jobs
        .iter()
        .find(|entry| entry.job_id == job_id)
        .cloned();
    store.jobs.retain(|entry| entry.job_id != job_id);
    save_queue_store_to_root(&root, &store)?;

    if let Some(job) = removed {
        let bundle_dir = PathBuf::from(&job.bundle_dir);
        if bundle_dir.exists() {
            fs::remove_dir_all(&bundle_dir).ok();
        }
    }

    Ok(store.jobs.len() != before)
}

pub fn consume_pending_job(app: &AppHandle, job_id: &str) -> Result<Option<GddPendingPublishJob>, String> {
    let root = gdd_root_dir(app);
    let mut store = load_queue_store_from_root(&root)?;
    let mut removed: Option<GddPendingPublishJob> = None;
    store.jobs.retain(|entry| {
        if entry.job_id == job_id {
            removed = Some(entry.clone());
            false
        } else {
            true
        }
    });
    save_queue_store_to_root(&root, &store)?;
    Ok(removed)
}

pub fn load_publish_request_for_job(job: &GddPendingPublishJob) -> Result<ConfluencePublishRequest, String> {
    let path = PathBuf::from(&job.bundle_dir).join("publish-request.json");
    let raw = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Failed reading queued publish request '{}': {}",
            path.display(), error
        )
    })?;
    serde_json::from_str(&raw).map_err(|error| {
        format!(
            "Failed parsing queued publish request '{}': {}",
            path.display(), error
        )
    })
}

pub fn queue_publish_request(
    app: &AppHandle,
    request: &GddPublishOrQueueRequest,
    error_reason: &str,
) -> Result<GddPendingPublishJob, String> {
    let root = gdd_root_dir(app);
    let bundles_root = bundles_root_dir(&root);
    fs::create_dir_all(&bundles_root).map_err(|error| {
        format!(
            "Failed creating bundle root directory '{}': {}",
            bundles_root.display(), error
        )
    })?;

    let job_id = format!("gddpq_{}", crate::util::now_ms());
    let bundle_dir = bundles_root.join(&job_id);
    fs::create_dir_all(&bundle_dir).map_err(|error| {
        format!(
            "Failed creating bundle directory '{}': {}",
            bundle_dir.display(), error
        )
    })?;

    let mut draft = request.draft.clone();
    if !request.publish_request.title.trim().is_empty() {
        draft.title = request.publish_request.title.trim().to_string();
    }

    let draft_json_path = bundle_dir.join("draft.json");
    let markdown_path = bundle_dir.join("draft.md");
    let confluence_html_path = bundle_dir.join("draft.confluence.html");
    let publish_request_path = bundle_dir.join("publish-request.json");
    let manifest_path = bundle_dir.join("manifest.json");

    write_json_atomic(&draft_json_path, &draft)?;
    fs::write(&markdown_path, render_storage::render_markdown(&draft)).map_err(|error| {
        format!(
            "Failed writing markdown bundle '{}': {}",
            markdown_path.display(), error
        )
    })?;
    fs::write(
        &confluence_html_path,
        request.publish_request.storage_body.as_str(),
    )
    .map_err(|error| {
        format!(
            "Failed writing Confluence storage bundle '{}': {}",
            confluence_html_path.display(), error
        )
    })?;
    write_json_atomic(&publish_request_path, &request.publish_request)?;

    let now = now_iso();
    let manifest = BundleManifest {
        created_at_iso: now.clone(),
        updated_at_iso: now.clone(),
        reason: "confluence_unreachable".to_string(),
        retries: 0,
        last_error: error_reason.to_string(),
    };
    write_json_atomic(&manifest_path, &manifest)?;

    let _bundle = GddPublishFallbackBundle {
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        draft_json_path: draft_json_path.to_string_lossy().to_string(),
        markdown_path: markdown_path.to_string_lossy().to_string(),
        confluence_html_path: confluence_html_path.to_string_lossy().to_string(),
        publish_request_path: publish_request_path.to_string_lossy().to_string(),
        manifest_path: manifest_path.to_string_lossy().to_string(),
    };

    let job = GddPendingPublishJob {
        job_id,
        title: request.publish_request.title.clone(),
        space_key: request.publish_request.space_key.clone(),
        parent_page_id: request.publish_request.parent_page_id.clone(),
        target_page_id: request.publish_request.target_page_id.clone(),
        created_at_iso: now.clone(),
        updated_at_iso: now,
        retry_count: 0,
        last_error: error_reason.to_string(),
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        routing_confidence: request.routing_confidence,
        routing_reasoning: request.routing_reasoning.clone(),
    };

    persist_pending_job(app, &job)?;
    Ok(job)
}

pub fn mark_retry_failure(job: &mut GddPendingPublishJob, error: &str) {
    job.retry_count = job.retry_count.saturating_add(1);
    job.updated_at_iso = now_iso();
    job.last_error = error.to_string();

    let manifest_path = PathBuf::from(&job.bundle_dir).join("manifest.json");
    if manifest_path.exists() {
        let manifest = BundleManifest {
            created_at_iso: job.created_at_iso.clone(),
            updated_at_iso: job.updated_at_iso.clone(),
            reason: "confluence_retry_failed".to_string(),
            retries: job.retry_count,
            last_error: job.last_error.clone(),
        };
        if let Err(error) = write_json_atomic(&manifest_path, &manifest) {
            warn!(
                "Failed updating queue manifest '{}' after retry failure: {}",
                manifest_path.display(),
                error
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queueable_status_codes_are_detected() {
        assert!(is_queueable_publish_error("Confluence request failed (HTTP 503)"));
        assert!(is_queueable_publish_error("Confluence request failed (HTTP 429): rate limited"));
        assert!(is_queueable_publish_error("Confluence request failed (HTTP 408)"));
        assert!(!is_queueable_publish_error("Confluence request failed (HTTP 401)"));
        assert!(!is_queueable_publish_error("Confluence request failed (HTTP 400)"));
    }

    #[test]
    fn queueable_transport_failures_are_detected() {
        assert!(is_queueable_publish_error("Confluence request failed: dns lookup failed"));
        assert!(is_queueable_publish_error("network timeout while connecting"));
        assert!(is_queueable_publish_error("connection refused by remote host"));
    }

    #[test]
    fn write_json_atomic_roundtrip() {
        let base = std::env::temp_dir().join(format!(
            "trispr_publish_queue_test_{}_{}",
            std::process::id(),
            crate::util::now_ms()
        ));
        fs::create_dir_all(&base).expect("create temp base");
        let path = base.join("queue.json");
        let store = GddPublishQueueStore {
            jobs: vec![GddPendingPublishJob {
                job_id: "job1".to_string(),
                title: "Doc".to_string(),
                space_key: "GAME".to_string(),
                parent_page_id: None,
                target_page_id: None,
                created_at_iso: "2026-01-01T00:00:00Z".to_string(),
                updated_at_iso: "2026-01-01T00:00:00Z".to_string(),
                retry_count: 0,
                last_error: "x".to_string(),
                bundle_dir: base.join("b").to_string_lossy().to_string(),
                routing_confidence: Some(0.8),
                routing_reasoning: Some("ok".to_string()),
            }],
        };

        write_json_atomic(&path, &store).expect("write queue json");
        let loaded: GddPublishQueueStore =
            serde_json::from_str(&fs::read_to_string(&path).expect("read queue"))
                .expect("parse queue");
        assert_eq!(loaded.jobs.len(), 1);
        assert_eq!(loaded.jobs[0].job_id, "job1");

        let _ = fs::remove_dir_all(&base);
    }
}
