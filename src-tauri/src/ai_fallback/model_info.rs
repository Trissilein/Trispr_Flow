//! Details for the expanded model card: what the local Ollama knows about a model (`/api/show`)
//! plus what its public page says (Hugging Face API / Ollama library page).
//!
//! The frontend may not talk to external hosts (CSP), so this module does. Rules:
//! - only `huggingface.co` and `ollama.com`, HTTPS only, no credentials, no other port;
//! - every redirect hop is checked against the same rule (a redirect to a foreign host is refused);
//! - total time limit per fetch and a response size cap; everything read is untrusted text;
//! - successful remote answers are cached on disk for 24 h (written atomically, a damaged cache
//!   is ignored); a failure falls back to an older cache entry, else to the local data alone;
//! - one remote fetch per model at a time;
//! - with strict local mode on, nothing leaves the machine.

use super::model_registry::{parse_model_ref, ModelRef, ModelRegistry};
use super::provider::{ollama_endpoint_candidates, validate_ollama_model_name};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, State};
use url::Url;

const HUGGINGFACE_BASE: &str = "https://huggingface.co";
const OLLAMA_BASE: &str = "https://ollama.com";
const REMOTE_TOTAL_TIMEOUT: Duration = Duration::from_secs(8);
const HF_JSON_CAP_BYTES: usize = 256 * 1024;
const OLLAMA_PAGE_CAP_BYTES: usize = 512 * 1024;
const SHOW_CAP_BYTES: usize = 8 * 1024 * 1024;
const MAX_REDIRECTS: u8 = 3;
const CACHE_TTL_MS: u64 = 24 * 60 * 60 * 1000;
const CACHE_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;
const CACHE_MAX_FILE_BYTES: u64 = 64 * 1024;
const CACHE_VERSION: u32 = 1;
const USER_AGENT: &str = "TrisprFlow/ModelDetails";

// ───────────────────────────────────── result types ─────────────────────────────────────

/// What the local Ollama reports for an installed model. Every field may be missing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LocalModelInfo {
    /// Short license name (`apache-2.0`) or the first line of the license text.
    pub license: Option<String>,
    pub capabilities: Vec<String>,
    pub family: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
    pub format: Option<String>,
    pub context_length: Option<u64>,
    pub architecture: Option<String>,
    pub parameter_count: Option<u64>,
    /// "Gemma 4 E4B (Google)" from the GGUF metadata.
    pub base_model: Option<String>,
    /// Minimum Ollama version, as the model file declares it.
    pub requires: Option<String>,
    pub modified_at: Option<String>,
}

/// What the model's public page says. `source` is `huggingface` or `ollama`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RemoteModelInfo {
    pub source: String,
    /// Hugging Face: when the repository was created. The Ollama library has no release date.
    pub released_at: Option<String>,
    /// Hugging Face: last repository change. Ollama: last update of the library entry.
    pub updated_at: Option<String>,
    pub downloads: Option<u64>,
    /// Ollama shows rounded counts ("28.4M"), kept as the page's text.
    pub downloads_text: Option<String>,
    pub likes: Option<u64>,
    pub license: Option<String>,
    pub pipeline_tag: Option<String>,
    pub languages: Vec<String>,
    pub base_model: Option<String>,
    pub description: Option<String>,
    pub gated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelDetails {
    pub model: String,
    pub local: Option<LocalModelInfo>,
    /// Why `local` is missing (Ollama not running, model not installed).
    pub local_error: Option<String>,
    pub remote: Option<RemoteModelInfo>,
    /// Why `remote` is missing or what to know about it ("Strict local mode ...", "Offline ...").
    pub remote_note: Option<String>,
    /// `remote` came from the disk cache.
    pub remote_cached: bool,
    /// `remote` is an expired cache entry shown because the fetch failed.
    pub remote_stale: bool,
    pub remote_fetched_at_ms: Option<u64>,
}

// ───────────────────────────────────── text hygiene ─────────────────────────────────────

/// Untrusted text: control characters become spaces, whitespace collapses, the result is cut at
/// `max_chars`. `None` when nothing is left.
fn clean_text(raw: &str, max_chars: usize) -> Option<String> {
    let mut out = String::new();
    let mut last_space = true;
    let mut count = 0usize;
    for ch in raw.chars() {
        let ch = if ch.is_control() || ch.is_whitespace() {
            ' '
        } else {
            ch
        };
        if ch == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        if count >= max_chars {
            break;
        }
        out.push(ch);
        count += 1;
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

/// ISO-8601-looking timestamp (`2026-04-01T14:42:04.000Z`, `...+01:00`) or `None`.
fn clean_timestamp(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.len() < 10 || raw.len() > 40 {
        return None;
    }
    if !raw
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '-' | ':' | 'T' | 'Z' | '.' | '+'))
    {
        return None;
    }
    let bytes = raw.as_bytes();
    let date_ok = bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit);
    date_ok.then(|| raw.to_string())
}

fn string_field(json: &serde_json::Value, key: &str, max_chars: usize) -> Option<String> {
    json.get(key)?
        .as_str()
        .and_then(|s| clean_text(s, max_chars))
}

fn push_unique(list: &mut Vec<String>, value: String, max_items: usize) {
    if list.len() < max_items && !list.iter().any(|existing| existing == &value) {
        list.push(value);
    }
}

// ─────────────────────────────── local: /api/show ───────────────────────────────

/// Reads an `/api/show` answer. Never panics; unknown shapes yield empty fields.
pub fn parse_show_response(json: &serde_json::Value) -> LocalModelInfo {
    let details = json.get("details");
    let detail = |key: &str| details.and_then(|d| string_field(d, key, 40));
    let model_info = json.get("model_info").and_then(|v| v.as_object());
    let info_text = |key: &str, max: usize| {
        model_info
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_str())
            .and_then(|s| clean_text(s, max))
    };
    let info_number = |key: &str| {
        model_info
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_u64())
            .filter(|n| *n > 0 && *n < 1_000_000_000_000_000)
    };

    let architecture = info_text("general.architecture", 40);
    let context_length = architecture
        .as_ref()
        .and_then(|arch| info_number(&format!("{}.context_length", arch)))
        .or_else(|| {
            model_info.and_then(|m| {
                m.iter()
                    .find(|(key, _)| key.ends_with(".context_length"))
                    .and_then(|(_, value)| value.as_u64())
                    .filter(|n| *n > 0 && *n < 1_000_000_000)
            })
        });

    let license = info_text("general.license", 60).or_else(|| {
        json.get("license")
            .and_then(|v| v.as_str())
            .and_then(|text| {
                text.lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .and_then(|line| clean_text(line, 80))
            })
    });

    let base_model = info_text("general.base_model.0.name", 80).map(|name| {
        match info_text("general.base_model.0.organization", 60) {
            Some(org) => format!("{} ({})", name, org),
            None => name,
        }
    });

    let mut capabilities = Vec::new();
    if let Some(items) = json.get("capabilities").and_then(|v| v.as_array()) {
        for item in items {
            if let Some(cap) = item.as_str().and_then(|s| clean_text(s, 24)) {
                push_unique(&mut capabilities, cap, 12);
            }
        }
    }

    LocalModelInfo {
        license,
        capabilities,
        family: detail("family"),
        parameter_size: detail("parameter_size"),
        quantization_level: detail("quantization_level"),
        format: detail("format"),
        context_length,
        architecture,
        parameter_count: info_number("general.parameter_count"),
        base_model,
        requires: string_field(json, "requires", 32),
        modified_at: json
            .get("modified_at")
            .and_then(|v| v.as_str())
            .and_then(clean_timestamp),
    }
}

fn fetch_local_info(endpoint: &str, model: &str) -> Result<LocalModelInfo, String> {
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(8))
        .build();
    let mut last_error: Option<String> = None;
    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/show", candidate);
        match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(serde_json::json!({ "model": model }))
        {
            Ok(response) => {
                let mut body = Vec::new();
                response
                    .into_reader()
                    .take(SHOW_CAP_BYTES as u64 + 1)
                    .read_to_end(&mut body)
                    .map_err(|e| format!("Could not read the model details: {}", e))?;
                if body.len() > SHOW_CAP_BYTES {
                    return Err(
                        "Ollama answered with an unexpectedly large model description.".into(),
                    );
                }
                let json: serde_json::Value = serde_json::from_slice(&body)
                    .map_err(|e| format!("Ollama's model description is not valid JSON: {}", e))?;
                return Ok(parse_show_response(&json));
            }
            Err(ureq::Error::Status(404, _)) => {
                return Err(format!("'{}' is not installed.", model));
            }
            Err(ureq::Error::Status(code, _)) => {
                return Err(format!("Ollama returned HTTP {}.", code));
            }
            Err(ureq::Error::Transport(transport)) => last_error = Some(transport.to_string()),
        }
    }
    Err(format!(
        "Ollama is not reachable: {}",
        last_error.unwrap_or_else(|| "no answer".to_string())
    ))
}

// ─────────────────────────────── remote: guarded HTTP ───────────────────────────────

pub(crate) type UrlPolicy = dyn Fn(&Url) -> bool + Send + Sync;

/// The only URLs this module (and the "open page" command) will ever use.
pub(crate) fn is_allowed_remote_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && matches!(url.host_str(), Some("huggingface.co") | Some("ollama.com"))
}

#[derive(Debug, Clone)]
pub(crate) struct HttpLimits {
    pub total: Duration,
    pub cap_bytes: usize,
    pub max_redirects: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HttpFail {
    /// 404 / 400.
    NotFound,
    /// 401 / 403: private, gated or removed (Hugging Face answers so for repos that do not exist).
    Denied,
    Status(u16),
    Timeout,
    Transport(String),
    TooLarge,
    /// The answer arrived but is not what was asked for (not JSON, not a model, no readable details).
    Unreadable(String),
    /// A redirect (or the start URL) leaves the allowed hosts, or the redirect chain is too long.
    BadRedirect(String),
}

impl HttpFail {
    fn describe(&self) -> String {
        match self {
            HttpFail::NotFound => "not found".to_string(),
            HttpFail::Denied => "not accessible (private, gated or removed)".to_string(),
            HttpFail::Status(code) => format!("the server answered HTTP {}", code),
            HttpFail::Timeout => "the request timed out".to_string(),
            HttpFail::Transport(message) => format!("no connection ({})", message),
            HttpFail::TooLarge => "the answer was unexpectedly large".to_string(),
            HttpFail::Unreadable(why) => format!("unreadable answer ({})", why),
            HttpFail::BadRedirect(why) => format!("refused: {}", why),
        }
    }
}

#[derive(Debug)]
pub(crate) struct HttpBody {
    pub bytes: Vec<u8>,
    /// The answer was longer than the cap and was cut (only with `allow_truncation`).
    #[allow(dead_code)]
    pub truncated: bool,
}

fn is_timeout_io(error: &std::io::Error) -> bool {
    // 10060 = WSAETIMEDOUT, 110 = ETIMEDOUT: the OS text is localised, the code is not.
    matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) || matches!(error.raw_os_error(), Some(10060) | Some(110))
}

/// A transport error is a timeout when an I/O error anywhere in its cause chain says so.
fn is_timeout_transport(error: &ureq::Transport) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = source {
        if let Some(io) = current.downcast_ref::<std::io::Error>() {
            if is_timeout_io(io) {
                return true;
            }
        }
        source = current.source();
    }
    let lower = error.to_string().to_ascii_lowercase();
    lower.contains("timed out") || lower.contains("timeout") || lower.contains("os error 10060")
}

/// GET with redirects followed by hand (each hop must pass `policy`), a total time limit and a size
/// cap. Without `allow_truncation` an answer over the cap is an error, because cut-off JSON is junk;
/// with it (HTML pages whose interesting part is at the top) the prefix is returned.
pub(crate) fn http_get_guarded(
    url: &str,
    policy: &UrlPolicy,
    accept: &str,
    limits: &HttpLimits,
    allow_truncation: bool,
) -> Result<HttpBody, HttpFail> {
    let started = Instant::now();
    let mut current =
        Url::parse(url).map_err(|e| HttpFail::BadRedirect(format!("bad URL ({})", e)))?;
    if !policy(&current) {
        return Err(HttpFail::BadRedirect("the host is not allowed".to_string()));
    }
    for _hop in 0..=limits.max_redirects {
        let remaining = limits
            .total
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or(HttpFail::Timeout)?;
        let agent = ureq::builder()
            .redirects(0)
            .timeout(remaining)
            .timeout_connect(remaining.min(Duration::from_secs(4)))
            .build();
        let response = match agent
            .get(current.as_str())
            .set("Accept", accept)
            .set("User-Agent", USER_AGENT)
            .call()
        {
            Ok(response) => response,
            Err(ureq::Error::Status(404, _)) | Err(ureq::Error::Status(400, _)) => {
                return Err(HttpFail::NotFound)
            }
            Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
                return Err(HttpFail::Denied)
            }
            Err(ureq::Error::Status(code, _)) => return Err(HttpFail::Status(code)),
            Err(ureq::Error::Transport(transport)) => {
                return Err(if is_timeout_transport(&transport) {
                    HttpFail::Timeout
                } else {
                    HttpFail::Transport(transport.to_string())
                });
            }
        };

        let status = response.status();
        if (300..400).contains(&status) {
            let location = response
                .header("location")
                .map(str::to_string)
                .ok_or_else(|| HttpFail::BadRedirect("redirect without a target".to_string()))?;
            let next = current
                .join(location.trim())
                .map_err(|_| HttpFail::BadRedirect("unreadable redirect target".to_string()))?;
            if !policy(&next) {
                return Err(HttpFail::BadRedirect(
                    "the redirect leaves the allowed hosts".to_string(),
                ));
            }
            current = next;
            continue;
        }

        if !allow_truncation {
            let announced = response
                .header("content-length")
                .and_then(|value| value.trim().parse::<usize>().ok());
            if announced.is_some_and(|len| len > limits.cap_bytes) {
                return Err(HttpFail::TooLarge);
            }
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(limits.cap_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| {
                if is_timeout_io(&e) || e.to_string().to_ascii_lowercase().contains("timed out") {
                    HttpFail::Timeout
                } else {
                    HttpFail::Transport(e.to_string())
                }
            })?;
        let truncated = bytes.len() > limits.cap_bytes;
        if truncated {
            if !allow_truncation {
                return Err(HttpFail::TooLarge);
            }
            bytes.truncate(limits.cap_bytes);
        }
        return Ok(HttpBody { bytes, truncated });
    }
    Err(HttpFail::BadRedirect("too many redirects".to_string()))
}

// ─────────────────────────────── remote: Hugging Face ───────────────────────────────

/// Only the fields this module reads; keeps the answer under a kilobyte instead of ~20 KB.
const HF_EXPAND_QUERY: &str = "expand%5B%5D=createdAt&expand%5B%5D=lastModified&expand%5B%5D=downloads&expand%5B%5D=likes&expand%5B%5D=tags&expand%5B%5D=pipeline_tag&expand%5B%5D=cardData&expand%5B%5D=gated";

fn first_text_of(value: Option<&serde_json::Value>, max_chars: usize) -> Option<String> {
    match value? {
        serde_json::Value::String(text) => clean_text(text, max_chars),
        serde_json::Value::Array(items) => items
            .iter()
            .find_map(|item| item.as_str().and_then(|s| clean_text(s, max_chars))),
        _ => None,
    }
}

/// `GET /api/models/<org>/<repo>` answer.
pub fn parse_hf_model(body: &[u8]) -> Result<RemoteModelInfo, String> {
    let json: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| format!("not valid JSON ({})", e))?;
    let Some(object) = json.as_object() else {
        return Err("not a model description".to_string());
    };
    // A real model has at least an id; an HTML error page that happened to be JSON does not.
    if !object.get("id").is_some_and(|id| id.is_string()) && !object.contains_key("modelId") {
        return Err("not a model description".to_string());
    }
    let card = object.get("cardData");
    let tags: Vec<&str> = object
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(|t| t.as_str()).collect())
        .unwrap_or_default();

    let license = tags
        .iter()
        .find_map(|tag| tag.strip_prefix("license:"))
        .and_then(|value| clean_text(value, 60))
        .or_else(|| first_text_of(card.and_then(|c| c.get("license")), 60));

    let base_model = first_text_of(card.and_then(|c| c.get("base_model")), 120).or_else(|| {
        tags.iter().find_map(|tag| {
            let rest = tag.strip_prefix("base_model:")?;
            // `base_model:quantized:org/x` / `finetune:` / `adapter:` / `merge:` repeat the plain one.
            if rest.contains(':') {
                return None;
            }
            clean_text(rest, 120)
        })
    });

    let mut languages = Vec::new();
    match card.and_then(|c| c.get("language")) {
        Some(serde_json::Value::String(code)) => {
            if let Some(code) = clean_text(code, 12) {
                languages.push(code);
            }
        }
        Some(serde_json::Value::Array(items)) => {
            for item in items {
                if let Some(code) = item.as_str().and_then(|s| clean_text(s, 12)) {
                    push_unique(&mut languages, code, 8);
                }
            }
        }
        _ => {}
    }

    let gated = match object.get("gated") {
        Some(serde_json::Value::Bool(flag)) => Some(*flag),
        Some(serde_json::Value::String(mode)) if !mode.is_empty() => Some(true),
        _ => None,
    };

    Ok(RemoteModelInfo {
        source: "huggingface".to_string(),
        released_at: object
            .get("createdAt")
            .and_then(|v| v.as_str())
            .and_then(clean_timestamp),
        updated_at: object
            .get("lastModified")
            .and_then(|v| v.as_str())
            .and_then(clean_timestamp),
        downloads: object.get("downloads").and_then(|v| v.as_u64()),
        downloads_text: None,
        likes: object.get("likes").and_then(|v| v.as_u64()),
        license,
        pipeline_tag: string_field(&json, "pipeline_tag", 40),
        languages,
        base_model,
        description: None,
        gated,
    })
}

fn fetch_hf_details(
    base: &str,
    path: &str,
    policy: &UrlPolicy,
    total: Duration,
) -> Result<RemoteModelInfo, HttpFail> {
    let url = format!("{}/api/models/{}?{}", base, path, HF_EXPAND_QUERY);
    let limits = HttpLimits {
        total,
        cap_bytes: HF_JSON_CAP_BYTES,
        max_redirects: MAX_REDIRECTS,
    };
    let body = http_get_guarded(&url, policy, "application/json", &limits, false)?;
    parse_hf_model(&body.bytes).map_err(HttpFail::Unreadable)
}

// ─────────────────────────────── remote: Ollama library ───────────────────────────────

/// Decodes the handful of entities a meta tag uses. Anything unknown stays as it is.
fn decode_entities(text: &str) -> String {
    static ENTITY: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"&(#x[0-9a-fA-F]{1,6}|#[0-9]{1,7}|[a-zA-Z]{2,6});").unwrap()
    });
    ENTITY
        .replace_all(text, |caps: &regex::Captures| {
            let name = &caps[1];
            let decoded = if let Some(hex) = name.strip_prefix("#x") {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(dec) = name.strip_prefix('#') {
                dec.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => None,
                }
            };
            decoded
                .map(|c| c.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned()
}

/// "Oct 12, 2026 10:18 PM UTC" -> "2026-10-12T22:18:00Z"; anything else -> `None`.
fn parse_library_timestamp(text: &str) -> Option<String> {
    static STAMP: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^([A-Z][a-z]{2}) (\d{1,2}), (\d{4}) (\d{1,2}):(\d{2}) (AM|PM) UTC$")
            .unwrap()
    });
    let caps = STAMP.captures(text.trim())?;
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = MONTHS.iter().position(|m| *m == &caps[1])? as u32 + 1;
    let day: u32 = caps[2].parse().ok()?;
    let year: u32 = caps[3].parse().ok()?;
    let hour12: u32 = caps[4].parse().ok()?;
    let minute: u32 = caps[5].parse().ok()?;
    if !(1..=31).contains(&day) || !(1..=12).contains(&hour12) || minute > 59 || year < 2000 {
        return None;
    }
    let hour = match (&caps[6], hour12) {
        ("AM", 12) => 0,
        ("AM", h) => h,
        ("PM", 12) => 12,
        ("PM", h) => h + 12,
        _ => return None,
    };
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:00Z",
        year, month, day, hour, minute
    ))
}

/// The three facts an Ollama library page offers in a form that can be read without guessing: the
/// `description` meta tag, the download count next to "Downloads" and the exact time behind
/// "Updated" (a `title` attribute; the visible text only says "2 weeks ago"). Each is validated
/// on its own, so a redesigned page drops a field instead of producing a wrong one. There is no
/// release date on the page, so none is reported.
pub fn parse_ollama_library_page(html: &str) -> RemoteModelInfo {
    static DESCRIPTION: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"(?is)<meta\s+name="description"\s+content="([^"]*)""#).unwrap()
    });
    static DOWNLOADS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?s)<span\s*>\s*([0-9][0-9.,]{0,9}[KMB]?)\s*</span>\s*<span[^>]*>\s*(?:&nbsp;|\s)*Downloads")
            .unwrap()
    });
    static UPDATED: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r#"(?s)title="([^"]{1,40})"[^>]*>\s*<svg.{0,1500}?</svg>\s*<span[^>]*>\s*Updated"#,
        )
        .unwrap()
    });
    static COUNT: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[0-9]+(?:[.,][0-9]+)?[KMB]?$").unwrap());

    let description = DESCRIPTION
        .captures(html)
        .and_then(|caps| clean_text(&decode_entities(&caps[1]), 500));
    let downloads_text = DOWNLOADS
        .captures(html)
        .map(|caps| caps[1].to_string())
        .filter(|text| COUNT.is_match(text));
    let updated_at = UPDATED
        .captures(html)
        .and_then(|caps| parse_library_timestamp(&decode_entities(&caps[1])));
    RemoteModelInfo {
        source: "ollama".to_string(),
        released_at: None,
        updated_at,
        downloads: None,
        downloads_text,
        likes: None,
        license: None,
        pipeline_tag: None,
        languages: Vec::new(),
        base_model: None,
        description,
        gated: None,
    }
}

fn fetch_ollama_library_details(
    base: &str,
    model: &ModelRef,
    policy: &UrlPolicy,
    total: Duration,
) -> Result<RemoteModelInfo, HttpFail> {
    let url = format!("{}/{}:{}", base, model.path, model.tag);
    let limits = HttpLimits {
        total,
        cap_bytes: OLLAMA_PAGE_CAP_BYTES,
        max_redirects: MAX_REDIRECTS,
    };
    let body = http_get_guarded(&url, policy, "text/html", &limits, true)?;
    let html = String::from_utf8_lossy(&body.bytes);
    let info = parse_ollama_library_page(&html);
    if info.description.is_none() && info.updated_at.is_none() && info.downloads_text.is_none() {
        return Err(HttpFail::Unreadable(
            "the page has no readable details".to_string(),
        ));
    }
    Ok(info)
}

// ─────────────────────────────── disk cache ───────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    model: String,
    fetched_at_ms: u64,
    remote: RemoteModelInfo,
}

fn cache_file_for(dir: &Path, model: &str) -> PathBuf {
    let digest = Sha256::digest(model.trim().to_ascii_lowercase().as_bytes());
    dir.join(format!("{}.json", &hex::encode(digest)[..32]))
}

/// The cached entry for `model`, or `None` for a missing, oversized, damaged or foreign file.
fn load_cache(dir: &Path, model: &str) -> Option<CacheFile> {
    let path = cache_file_for(dir, model);
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() == 0 || meta.len() > CACHE_MAX_FILE_BYTES {
        return None;
    }
    let raw = std::fs::read_to_string(&path).ok()?;
    let cache: CacheFile = serde_json::from_str(&raw).ok()?;
    if cache.version != CACHE_VERSION || !cache.model.eq_ignore_ascii_case(model.trim()) {
        return None;
    }
    Some(cache)
}

fn cache_is_fresh(cache: &CacheFile, now_ms: u64) -> bool {
    cache.fetched_at_ms <= now_ms.saturating_add(CACHE_CLOCK_SKEW_MS)
        && now_ms.saturating_sub(cache.fetched_at_ms) < CACHE_TTL_MS
}

/// Write next to the target, then rename: a crash never leaves half a file behind.
fn save_cache(
    dir: &Path,
    model: &str,
    remote: &RemoteModelInfo,
    now_ms: u64,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {}", dir.display(), e))?;
    let path = cache_file_for(dir, model);
    let raw = serde_json::to_string(&CacheFile {
        version: CACHE_VERSION,
        model: model.trim().to_string(),
        fetched_at_ms: now_ms,
        remote: remote.clone(),
    })
    .map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("json.tmp-{}-{}", std::process::id(), now_ms));
    std::fs::write(&tmp, raw).map_err(|e| format!("write {}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("replace {}: {}", path.display(), e)
    })
}

// ─────────────────────────────── orchestration ───────────────────────────────

/// Where the remote data comes from. Production uses the two public hosts; tests point it at a
/// local mock server (and widen `policy` to that one address).
pub(crate) struct RemoteSources {
    pub hf_base: String,
    pub ollama_base: String,
    pub policy: Box<UrlPolicy>,
    pub total_timeout: Duration,
}

impl RemoteSources {
    pub fn production() -> Self {
        Self {
            hf_base: HUGGINGFACE_BASE.to_string(),
            ollama_base: OLLAMA_BASE.to_string(),
            policy: Box::new(is_allowed_remote_url),
            total_timeout: REMOTE_TOTAL_TIMEOUT,
        }
    }
}

type FlightMap = Mutex<HashMap<String, Arc<Mutex<()>>>>;
static FLIGHTS: LazyLock<FlightMap> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// One remote fetch per model at a time: a second caller waits, then finds the cache the first wrote.
fn flight_lock(model: &str) -> Arc<Mutex<()>> {
    let mut flights = FLIGHTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if flights.len() > 128 {
        flights.retain(|_, lock| Arc::strong_count(lock) > 1);
    }
    flights
        .entry(model.trim().to_ascii_lowercase())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

struct RemoteOutcome {
    info: Option<RemoteModelInfo>,
    note: Option<String>,
    cached: bool,
    stale: bool,
    fetched_at_ms: Option<u64>,
}

impl RemoteOutcome {
    fn none(note: impl Into<String>) -> Self {
        Self {
            info: None,
            note: Some(note.into()),
            cached: false,
            stale: false,
            fetched_at_ms: None,
        }
    }
}

fn host_name(registry: ModelRegistry) -> &'static str {
    match registry {
        ModelRegistry::HuggingFace => "Hugging Face",
        ModelRegistry::Ollama => "ollama.com",
    }
}

fn remote_outcome(
    model: &str,
    cache_dir: Option<&Path>,
    sources: &RemoteSources,
    now_ms: u64,
) -> RemoteOutcome {
    let Ok(parsed) = parse_model_ref(model) else {
        return RemoteOutcome::none("This model name has no public page.");
    };
    let lock = flight_lock(model);
    let _flight = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let cached = cache_dir.and_then(|dir| load_cache(dir, model));
    if let Some(cache) = &cached {
        if cache_is_fresh(cache, now_ms) {
            return RemoteOutcome {
                info: Some(cache.remote.clone()),
                note: None,
                cached: true,
                stale: false,
                fetched_at_ms: Some(cache.fetched_at_ms),
            };
        }
    }

    let fetched = match parsed.registry {
        ModelRegistry::HuggingFace => fetch_hf_details(
            &sources.hf_base,
            &parsed.path,
            sources.policy.as_ref(),
            sources.total_timeout,
        ),
        ModelRegistry::Ollama => fetch_ollama_library_details(
            &sources.ollama_base,
            &parsed,
            sources.policy.as_ref(),
            sources.total_timeout,
        ),
    };
    match fetched {
        Ok(info) => {
            if let Some(dir) = cache_dir {
                if let Err(error) = save_cache(dir, model, &info, now_ms) {
                    tracing::warn!("[model-details] could not cache '{}': {}", model, error);
                }
            }
            RemoteOutcome {
                info: Some(info),
                note: None,
                cached: false,
                stale: false,
                fetched_at_ms: Some(now_ms),
            }
        }
        Err(HttpFail::NotFound) | Err(HttpFail::Denied) => RemoteOutcome::none(format!(
            "Not listed on {} (private, gated, removed or a local model).",
            host_name(parsed.registry)
        )),
        Err(failure) => match cached {
            Some(cache) => RemoteOutcome {
                info: Some(cache.remote),
                note: Some(format!(
                    "Online details could not be refreshed ({}). Showing saved ones.",
                    failure.describe()
                )),
                cached: true,
                stale: true,
                fetched_at_ms: Some(cache.fetched_at_ms),
            },
            None => RemoteOutcome::none(format!(
                "Online details unavailable: {}.",
                failure.describe()
            )),
        },
    }
}

/// Local answer plus the public page, with every failure reduced to a note.
pub(crate) fn build_model_details(
    endpoint: &str,
    model: &str,
    cache_dir: Option<&Path>,
    sources: &RemoteSources,
    now_ms: u64,
) -> ModelDetails {
    let (local, local_error) = match fetch_local_info(endpoint, model) {
        Ok(info) => (Some(info), None),
        Err(error) => (None, Some(error)),
    };
    let remote = remote_outcome(model, cache_dir, sources, now_ms);
    ModelDetails {
        model: model.to_string(),
        local,
        local_error,
        remote: remote.info,
        remote_note: remote.note,
        remote_cached: remote.cached,
        remote_stale: remote.stale,
        remote_fetched_at_ms: remote.fetched_at_ms,
    }
}

// ─────────────────────────────── Tauri commands ───────────────────────────────

/// Details of one model for its expanded card. Called on the first expansion only.
#[tauri::command]
pub(crate) async fn get_ollama_model_details(
    app: AppHandle,
    state: State<'_, AppState>,
    model: String,
) -> Result<ModelDetails, String> {
    validate_ollama_model_name(&model)?;
    let endpoint = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        settings.providers.ollama.endpoint.clone()
    };
    let cache_dir = crate::paths::resolve_data_path(&app, "runtime-cache").join("model-details");
    tauri::async_runtime::spawn_blocking(move || {
        build_model_details(
            &endpoint,
            &model,
            Some(&cache_dir),
            &RemoteSources::production(),
            crate::util::now_ms(),
        )
    })
    .await
    .map_err(|e| format!("Model details task failed: {}", e))
}

/// Opens a model page in the default browser. Only `https://huggingface.co/...` and
/// `https://ollama.com/...` pass; the URL is handed to the OS as one argument, never to a shell.
#[tauri::command]
pub(crate) fn open_model_page(url: String) -> Result<(), String> {
    let parsed = checked_page_url(&url)?;
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(parsed.as_str())
            .spawn()
            .map_err(|e| format!("Failed to open the page: {}", e))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(parsed.as_str())
            .spawn()
            .map_err(|e| format!("Failed to open the page: {}", e))?;
        Ok(())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(parsed.as_str())
            .spawn()
            .map_err(|e| format!("Failed to open the page: {}", e))?;
        Ok(())
    }
}

fn checked_page_url(raw: &str) -> Result<Url, String> {
    let raw = raw.trim();
    if raw.len() > 300 || raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Not an allowed model page.".to_string());
    }
    let parsed = Url::parse(raw).map_err(|_| "Not an allowed model page.".to_string())?;
    if !is_allowed_remote_url(&parsed) {
        return Err("Only huggingface.co and ollama.com pages can be opened.".to_string());
    }
    Ok(parsed)
}

// ─────────────────────────────── tests ───────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_fallback::test_support::{MockReply, MockServer};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "trispr-model-details-{}-{}-{}",
            label,
            std::process::id(),
            crate::util::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sources_for(server: &MockServer) -> RemoteSources {
        let allowed = Url::parse(&server.base_url).unwrap();
        RemoteSources {
            hf_base: format!("{}/hf", server.base_url),
            ollama_base: format!("{}/ol", server.base_url),
            policy: Box::new(move |url: &Url| {
                url.scheme() == "http"
                    && url.host_str() == allowed.host_str()
                    && url.port() == allowed.port()
            }),
            total_timeout: Duration::from_millis(1500),
        }
    }

    fn only_this_server(server: &MockServer) -> impl Fn(&Url) -> bool + Send + Sync + 'static {
        let allowed = Url::parse(&server.base_url).unwrap();
        move |url: &Url| {
            url.scheme() == "http"
                && url.host_str() == allowed.host_str()
                && url.port() == allowed.port()
        }
    }

    // A real answer of `/api/show` (0.40.13) for gemma4:e4b, shortened.
    fn show_sample() -> serde_json::Value {
        json!({
            "license": "                                 Apache License\n                           Version 2.0, January 2004\n",
            "capabilities": ["completion", "vision", "audio", "tools", "thinking"],
            "details": {
                "parent_model": "x", "format": "gguf", "family": "gemma4",
                "parameter_size": "7.5B", "quantization_level": "Q4_K_M"
            },
            "model_info": {
                "general.architecture": "gemma4",
                "gemma4.context_length": 131072,
                "general.license": "apache-2.0",
                "general.parameter_count": 7518069290u64,
                "general.base_model.0.name": "Gemma 4 E4B",
                "general.base_model.0.organization": "Google"
            },
            "modified_at": "2026-11-02T11:02:26.6753669+01:00",
            "requires": "0.30.9"
        })
    }

    // The HF answer shape (expand query), from the live endpoint.
    fn hf_sample() -> serde_json::Value {
        json!({
            "_id": "69cd2ebc3dc1641e52e18d1d",
            "id": "unsloth/gemma-4-E4B-it-GGUF",
            "lastModified": "2026-07-17T12:49:21.000Z",
            "downloads": 581312, "likes": 675,
            "tags": ["gguf", "gemma4", "base_model:google/gemma-4-E4B-it",
                     "base_model:quantized:google/gemma-4-E4B-it", "license:apache-2.0", "region:us"],
            "pipeline_tag": "image-text-to-text",
            "cardData": {"license": "apache-2.0", "language": ["en", "de"], "base_model": "google/gemma-4-E4B-it"},
            "gated": false,
            "createdAt": "2026-04-01T14:42:04.000Z"
        })
    }

    const OLLAMA_PAGE: &str = r##"<html><head><title>gemma4:e4b</title>
<meta name="description" content="Gemma 4 models are designed to deliver frontier-level performance &amp; more. It&#39;s fast."/>
</head><body>
<span class="flex items-center">
  <svg class="mr-1.5"><path d="M3 16.5"></path></svg>
  <span >28.4M</span>
  <span class="hidden sm:flex">&nbsp;Downloads</span>
</span>
<span class="flex items-center" title="Oct 12, 2026 10:18 PM UTC">
  <svg xmlns="http://www.w3.org/2000/svg" class="mr-1.5"><path stroke-linecap="round" d="M12 6v6"></path></svg>
  <span class="hidden sm:flex">Updated&nbsp;</span>
  <span >2 weeks ago</span>
</span>
</body></html>"##;

    // ───── pure parsers ─────

    #[test]
    fn show_response_is_read_into_plain_fields() {
        let info = parse_show_response(&show_sample());
        assert_eq!(info.license.as_deref(), Some("apache-2.0"));
        assert_eq!(
            info.capabilities,
            ["completion", "vision", "audio", "tools", "thinking"]
        );
        assert_eq!(info.family.as_deref(), Some("gemma4"));
        assert_eq!(info.parameter_size.as_deref(), Some("7.5B"));
        assert_eq!(info.quantization_level.as_deref(), Some("Q4_K_M"));
        assert_eq!(info.format.as_deref(), Some("gguf"));
        assert_eq!(info.context_length, Some(131072));
        assert_eq!(info.architecture.as_deref(), Some("gemma4"));
        assert_eq!(info.parameter_count, Some(7_518_069_290));
        assert_eq!(info.base_model.as_deref(), Some("Gemma 4 E4B (Google)"));
        assert_eq!(info.requires.as_deref(), Some("0.30.9"));
        assert!(info
            .modified_at
            .as_deref()
            .unwrap()
            .starts_with("2026-11-02T11:02"));
    }

    #[test]
    fn show_license_falls_back_to_the_first_line_of_the_text() {
        let mut sample = show_sample();
        sample["model_info"]
            .as_object_mut()
            .unwrap()
            .remove("general.license");
        let info = parse_show_response(&sample);
        assert_eq!(info.license.as_deref(), Some("Apache License"));
    }

    #[test]
    fn show_license_text_is_not_passed_on_whole() {
        let long = format!("{}\n{}", "x".repeat(500), "second line");
        let info = parse_show_response(&json!({ "license": long }));
        assert!(info.license.unwrap().chars().count() <= 80);
    }

    #[test]
    fn show_context_length_without_architecture_uses_any_context_key() {
        let info = parse_show_response(&json!({ "model_info": { "llama.context_length": 8192 } }));
        assert_eq!(info.context_length, Some(8192));
    }

    #[test]
    fn show_survives_garbage() {
        for garbage in [
            json!(null),
            json!([]),
            json!("text"),
            json!({ "details": "no", "model_info": 5, "capabilities": "x", "modified_at": 7 }),
            json!({ "model_info": { "general.architecture": 7, "x.context_length": "big" } }),
            json!({ "capabilities": [1, null, {"a": 1}, "tools", "tools"] }),
        ] {
            let info = parse_show_response(&garbage);
            assert!(info.capabilities.len() <= 1, "{garbage:?}");
        }
    }

    #[test]
    fn show_text_fields_are_cleaned_and_capped() {
        let info = parse_show_response(&json!({
            "details": { "family": "ge\u{0}mma\n4", "parameter_size": "A".repeat(200) },
            "capabilities": (0..40).map(|i| format!("cap{i}")).collect::<Vec<_>>(),
            "modified_at": "<script>alert(1)</script>"
        }));
        assert_eq!(info.family.as_deref(), Some("ge mma 4"));
        assert_eq!(info.parameter_size.unwrap().chars().count(), 40);
        assert_eq!(info.capabilities.len(), 12);
        assert_eq!(info.modified_at, None);
    }

    #[test]
    fn hf_answer_is_read() {
        let info = parse_hf_model(hf_sample().to_string().as_bytes()).unwrap();
        assert_eq!(info.source, "huggingface");
        assert_eq!(
            info.released_at.as_deref(),
            Some("2026-04-01T14:42:04.000Z")
        );
        assert_eq!(info.updated_at.as_deref(), Some("2026-07-17T12:49:21.000Z"));
        assert_eq!(info.downloads, Some(581_312));
        assert_eq!(info.likes, Some(675));
        assert_eq!(info.license.as_deref(), Some("apache-2.0"));
        assert_eq!(info.pipeline_tag.as_deref(), Some("image-text-to-text"));
        assert_eq!(info.languages, ["en", "de"]);
        assert_eq!(info.base_model.as_deref(), Some("google/gemma-4-E4B-it"));
        assert_eq!(info.gated, Some(false));
    }

    #[test]
    fn hf_license_and_base_model_fall_back_to_tags_and_card() {
        let info = parse_hf_model(
            json!({
                "id": "a/b",
                "tags": ["base_model:quantized:x/y", "base_model:org/plain", "license:mit"],
                "cardData": { "language": "en" },
                "gated": "manual"
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(info.license.as_deref(), Some("mit"));
        assert_eq!(info.base_model.as_deref(), Some("org/plain"));
        assert_eq!(info.languages, ["en"]);
        assert_eq!(info.gated, Some(true));

        let card_only = parse_hf_model(
            json!({ "id": "a/b", "cardData": { "license": ["apache-2.0"], "base_model": ["m/n"] } })
                .to_string()
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(card_only.license.as_deref(), Some("apache-2.0"));
        assert_eq!(card_only.base_model.as_deref(), Some("m/n"));
    }

    #[test]
    fn hf_rejects_what_is_not_a_model() {
        for body in [
            &b""[..],
            b"<html>captive portal</html>",
            b"[]",
            b"null",
            b"{\"error\":\"Repository not found\"}",
            b"{\"id\": 5}",
            b"{\"id\":\"a/b\"",
        ] {
            assert!(
                parse_hf_model(body).is_err(),
                "{:?}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn hf_hostile_values_are_neutralised() {
        let info = parse_hf_model(
            json!({
                "id": "a/b",
                "createdAt": "<img src=x onerror=alert(1)>",
                "lastModified": "2026-13-45T99:99:99Z\n",
                "downloads": "many", "likes": -4,
                "tags": ["license:<script>x</script>", 5, null],
                "pipeline_tag": "p".repeat(500),
                "cardData": { "language": (0..50).map(|i| format!("l{i}")).collect::<Vec<_>>() }
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(info.released_at, None);
        // Digits and separators only: the odd date passes the shape check but carries no markup.
        assert!(info.updated_at.is_none() || !info.updated_at.as_ref().unwrap().contains('\n'));
        assert_eq!(info.downloads, None);
        assert_eq!(info.likes, None);
        assert_eq!(info.pipeline_tag.unwrap().chars().count(), 40);
        assert_eq!(info.languages.len(), 8);
        // Text is stored as text; the frontend renders it with textContent.
        assert_eq!(info.license.as_deref(), Some("<script>x</script>"));
    }

    #[test]
    fn ollama_page_yields_description_downloads_and_update_time() {
        let info = parse_ollama_library_page(OLLAMA_PAGE);
        assert_eq!(info.source, "ollama");
        assert_eq!(
            info.description.as_deref(),
            Some("Gemma 4 models are designed to deliver frontier-level performance & more. It's fast.")
        );
        assert_eq!(info.downloads_text.as_deref(), Some("28.4M"));
        assert_eq!(info.updated_at.as_deref(), Some("2026-10-12T22:18:00Z"));
        assert_eq!(
            info.released_at, None,
            "the library page has no release date"
        );
    }

    #[test]
    fn ollama_page_fields_are_independent_and_validated() {
        let no_stamp = OLLAMA_PAGE.replace("Oct 12, 2026 10:18 PM UTC", "yesterday-ish");
        let info = parse_ollama_library_page(&no_stamp);
        assert_eq!(info.updated_at, None);
        assert!(info.description.is_some());

        let bad_count = OLLAMA_PAGE.replace("28.4M", "lots");
        assert_eq!(parse_ollama_library_page(&bad_count).downloads_text, None);

        let empty = parse_ollama_library_page("<html>nothing here</html>");
        assert_eq!(empty.description, None);
        assert_eq!(empty.downloads_text, None);
        assert_eq!(empty.updated_at, None);
    }

    #[test]
    fn ollama_page_description_is_text_only() {
        let page = r#"<meta name="description" content="a &lt;script&gt;alert(1)&lt;/script&gt; b&#10;c"/>"#;
        let info = parse_ollama_library_page(page);
        // Decoded to plain text (control characters flattened); rendering is the frontend's job.
        assert_eq!(
            info.description.as_deref(),
            Some("a <script>alert(1)</script> b c")
        );
    }

    #[test]
    fn library_timestamps_convert_and_reject() {
        assert_eq!(
            parse_library_timestamp("Oct 12, 2026 10:18 PM UTC").as_deref(),
            Some("2026-10-12T22:18:00Z")
        );
        assert_eq!(
            parse_library_timestamp("Jan 1, 2026 12:05 AM UTC").as_deref(),
            Some("2026-01-01T00:05:00Z")
        );
        assert_eq!(
            parse_library_timestamp("Jan 1, 2026 12:05 PM UTC").as_deref(),
            Some("2026-01-01T12:05:00Z")
        );
        for bad in [
            "",
            "Oct 32, 2026 10:18 PM UTC",
            "Foo 1, 2026 10:18 PM UTC",
            "Oct 1, 2026 13:18 PM UTC",
            "Oct 1, 2026 10:60 AM UTC",
            "2 weeks ago",
        ] {
            assert_eq!(parse_library_timestamp(bad), None, "{bad}");
        }
    }

    #[test]
    fn clean_text_flattens_and_cuts() {
        assert_eq!(
            clean_text("  a \n\t b\u{7}c  ", 50).as_deref(),
            Some("a b c")
        );
        assert_eq!(clean_text("   ", 50), None);
        assert_eq!(clean_text("äöüß", 2).as_deref(), Some("äö"));
    }

    #[test]
    fn only_the_two_public_https_hosts_are_allowed() {
        let ok = |s: &str| is_allowed_remote_url(&Url::parse(s).unwrap());
        assert!(ok("https://huggingface.co/api/models/a/b"));
        assert!(ok("https://ollama.com/library/gemma4:e4b"));
        for bad in [
            "http://huggingface.co/x",
            "https://evil.com/",
            "https://huggingface.co.evil.com/",
            "https://evil.com/huggingface.co",
            "https://user:pw@huggingface.co/",
            "https://huggingface.co:8443/",
            "https://registry.ollama.ai/v2/x",
            "https://www.ollama.com/",
            "https://huggingface.co./x",
            "ftp://ollama.com/",
            "file:///C:/Windows/system.ini",
        ] {
            assert!(!ok(bad), "{bad}");
        }
    }

    #[test]
    fn page_urls_for_the_open_command_are_checked() {
        assert!(checked_page_url("https://ollama.com/library/gemma4:e4b").is_ok());
        assert!(checked_page_url(" https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF ").is_ok());
        for bad in [
            "",
            "javascript:alert(1)",
            "https://evil.com/",
            "https://ollama.com/ x",
            "https://ollama.com/\nx",
            "file:///C:/x",
            "cmd /c calc",
            "https://ollama.com@evil.com/",
        ] {
            assert!(checked_page_url(bad).is_err(), "{bad:?}");
        }
        assert!(checked_page_url(&format!("https://ollama.com/{}", "a".repeat(400))).is_err());
    }

    // ───── guarded HTTP against a mock server ─────

    fn limits(total_ms: u64, cap: usize) -> HttpLimits {
        HttpLimits {
            total: Duration::from_millis(total_ms),
            cap_bytes: cap,
            max_redirects: 3,
        }
    }

    #[test]
    fn http_returns_the_body() {
        let server = MockServer::start(|_| MockReply::text(200, "hello"));
        let policy = only_this_server(&server);
        let body = http_get_guarded(
            &format!("{}/x", server.base_url),
            &policy,
            "text/plain",
            &limits(2000, 100),
            false,
        )
        .unwrap();
        assert_eq!(body.bytes, b"hello");
        assert!(!body.truncated);
    }

    #[test]
    fn http_maps_statuses() {
        let server = MockServer::start(|request| match request.path.as_str() {
            "/404" => MockReply::error(404, "x"),
            "/400" => MockReply::error(400, "x"),
            "/401" => MockReply::error(401, "x"),
            "/403" => MockReply::error(403, "x"),
            "/500" => MockReply::error(500, "x"),
            _ => MockReply::text(200, "ok"),
        });
        let policy = only_this_server(&server);
        let get = |path: &str| {
            http_get_guarded(
                &format!("{}{}", server.base_url, path),
                &policy,
                "*/*",
                &limits(2000, 100),
                false,
            )
        };
        assert_eq!(get("/404").unwrap_err(), HttpFail::NotFound);
        assert_eq!(get("/400").unwrap_err(), HttpFail::NotFound);
        assert_eq!(get("/401").unwrap_err(), HttpFail::Denied);
        assert_eq!(get("/403").unwrap_err(), HttpFail::Denied);
        assert_eq!(get("/500").unwrap_err(), HttpFail::Status(500));
    }

    #[test]
    fn http_times_out_on_a_slow_server() {
        let server = MockServer::start(|_| {
            MockReply::text(200, "late").delayed(Duration::from_millis(1500))
        });
        let policy = only_this_server(&server);
        let started = Instant::now();
        let result = http_get_guarded(
            &format!("{}/x", server.base_url),
            &policy,
            "*/*",
            &limits(300, 100),
            false,
        );
        assert_eq!(result.unwrap_err(), HttpFail::Timeout);
        assert!(started.elapsed() < Duration::from_millis(1400));
    }

    #[test]
    fn http_times_out_on_a_stalled_body() {
        let server = MockServer::start(|_| {
            MockReply::text(200, &"x".repeat(1000))
                .cut_at(10)
                .then_stall(Duration::from_millis(2000))
        });
        let policy = only_this_server(&server);
        let started = Instant::now();
        let result = http_get_guarded(
            &format!("{}/x", server.base_url),
            &policy,
            "*/*",
            &limits(400, 5000),
            false,
        );
        assert!(
            matches!(result, Err(HttpFail::Timeout) | Err(HttpFail::Transport(_))),
            "{result:?}"
        );
        assert!(started.elapsed() < Duration::from_millis(1800));
    }

    #[test]
    fn http_refuses_an_oversized_answer() {
        let server = MockServer::start(|_| MockReply::text(200, &"x".repeat(200_000)));
        let policy = only_this_server(&server);
        let result = http_get_guarded(
            &format!("{}/x", server.base_url),
            &policy,
            "*/*",
            &limits(2000, 1000),
            false,
        );
        assert_eq!(result.unwrap_err(), HttpFail::TooLarge);
    }

    #[test]
    fn http_can_return_the_prefix_of_a_long_page() {
        let server = MockServer::start(|_| MockReply::text(200, &"x".repeat(200_000)));
        let policy = only_this_server(&server);
        let body = http_get_guarded(
            &format!("{}/x", server.base_url),
            &policy,
            "*/*",
            &limits(2000, 1000),
            true,
        )
        .unwrap();
        assert_eq!(body.bytes.len(), 1000);
        assert!(body.truncated);
    }

    #[test]
    fn http_follows_a_redirect_inside_the_allowed_host() {
        let server = MockServer::start(|request| match request.path.as_str() {
            "/old" => MockReply::text(301, "").with_header("Location", "/new"),
            "/new" => MockReply::text(200, "arrived"),
            _ => MockReply::error(404, "x"),
        });
        let policy = only_this_server(&server);
        let body = http_get_guarded(
            &format!("{}/old", server.base_url),
            &policy,
            "*/*",
            &limits(2000, 100),
            false,
        )
        .unwrap();
        assert_eq!(body.bytes, b"arrived");
    }

    #[test]
    fn http_refuses_a_redirect_to_a_foreign_host() {
        let evil = MockServer::start(|_| MockReply::text(200, "evil"));
        let evil_url = format!("{}/loot", evil.base_url);
        let server =
            MockServer::start(move |_| MockReply::text(302, "").with_header("Location", &evil_url));
        let policy = only_this_server(&server);
        let result = http_get_guarded(
            &format!("{}/x", server.base_url),
            &policy,
            "*/*",
            &limits(2000, 100),
            false,
        );
        assert!(
            matches!(result, Err(HttpFail::BadRedirect(_))),
            "{result:?}"
        );
        assert!(
            evil.requests().is_empty(),
            "the foreign host must never be contacted"
        );
    }

    #[test]
    fn http_refuses_a_scheme_downgrade_and_an_endless_chain() {
        let server =
            MockServer::start(|_| MockReply::text(302, "").with_header("Location", "/again"));
        let policy = only_this_server(&server);
        let result = http_get_guarded(
            &format!("{}/x", server.base_url),
            &policy,
            "*/*",
            &limits(2000, 100),
            false,
        );
        assert!(
            matches!(result, Err(HttpFail::BadRedirect(_))),
            "{result:?}"
        );

        let downgrade = MockServer::start(|_| {
            MockReply::text(302, "").with_header("Location", "ftp://127.0.0.1/x")
        });
        let policy = only_this_server(&downgrade);
        let result = http_get_guarded(
            &format!("{}/x", downgrade.base_url),
            &policy,
            "*/*",
            &limits(2000, 100),
            false,
        );
        assert!(
            matches!(result, Err(HttpFail::BadRedirect(_))),
            "{result:?}"
        );
    }

    #[test]
    fn http_refuses_a_start_url_outside_the_policy() {
        let policy = |url: &Url| is_allowed_remote_url(url);
        let result = http_get_guarded(
            "https://evil.example/x",
            &policy,
            "*/*",
            &limits(500, 100),
            false,
        );
        assert!(matches!(result, Err(HttpFail::BadRedirect(_))));
        let result = http_get_guarded("not a url", &policy, "*/*", &limits(500, 100), false);
        assert!(matches!(result, Err(HttpFail::BadRedirect(_))));
    }

    // ───── cache ─────

    #[test]
    fn cache_round_trips_and_expires() {
        let dir = temp_dir("cache");
        let remote = parse_hf_model(hf_sample().to_string().as_bytes()).unwrap();
        save_cache(&dir, "hf.co/a/b:Q4", &remote, 1_000_000).unwrap();
        let loaded = load_cache(&dir, "HF.co/a/b:q4").expect("case-insensitive key");
        assert_eq!(loaded.remote, remote);
        assert!(cache_is_fresh(&loaded, 1_000_000 + CACHE_TTL_MS - 1));
        assert!(!cache_is_fresh(&loaded, 1_000_000 + CACHE_TTL_MS));
        // A cache stamped far in the future is distrusted.
        assert!(!cache_is_fresh(
            &loaded,
            1_000_000 - CACHE_CLOCK_SKEW_MS - 1
        ));
        assert!(load_cache(&dir, "other:model").is_none());
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "no temp file may stay behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_caches_are_ignored() {
        let dir = temp_dir("damaged");
        let model = "gemma4:e4b";
        let path = cache_file_for(&dir, model);
        for content in ["", "not json", "{\"version\":1}", "{\"version\":99,\"model\":\"gemma4:e4b\",\"fetched_at_ms\":1,\"remote\":{\"source\":\"x\",\"languages\":[]}}", "\u{0}\u{0}\u{0}"] {
            std::fs::write(&path, content).unwrap();
            assert!(load_cache(&dir, model).is_none(), "{content:?}");
        }
        std::fs::write(&path, vec![b'x'; (CACHE_MAX_FILE_BYTES + 1) as usize]).unwrap();
        assert!(load_cache(&dir, model).is_none());
        // A file that belongs to another model (hash collision / copied file) is not used.
        let remote = RemoteModelInfo {
            source: "ollama".into(),
            ..Default::default()
        };
        save_cache(&dir, "other:model", &remote, 5).unwrap();
        std::fs::copy(cache_file_for(&dir, "other:model"), &path).unwrap();
        assert!(load_cache(&dir, model).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ───── whole flow ─────

    fn ollama_endpoint_mock() -> MockServer {
        MockServer::start(|request| {
            if request.path == "/api/show" {
                if request.json()["model"] == "missing:model" {
                    MockReply::error(404, "model not found")
                } else {
                    MockReply::ok(show_sample())
                }
            } else {
                MockReply::error(404, "x")
            }
        })
    }

    #[test]
    fn details_combine_local_and_hugging_face() {
        let ollama = ollama_endpoint_mock();
        let remote = MockServer::start(|request| {
            if request
                .path
                .starts_with("/hf/api/models/unsloth/gemma-4-E4B-it-GGUF")
            {
                MockReply::ok(hf_sample())
            } else {
                MockReply::error(404, "x")
            }
        });
        let dir = temp_dir("flow-hf");
        let details = build_model_details(
            &ollama.base_url,
            "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M",
            Some(&dir),
            &sources_for(&remote),
            5_000,
        );
        assert!(details.local.is_some());
        let remote_info = details.remote.expect("hugging face data");
        assert_eq!(remote_info.downloads, Some(581_312));
        assert!(!details.remote_cached);
        assert_eq!(details.remote_note, None);
        // Second call: served from the cache, no second request.
        let again = build_model_details(
            &ollama.base_url,
            "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M",
            Some(&dir),
            &sources_for(&remote),
            6_000,
        );
        assert!(again.remote_cached);
        assert_eq!(remote.requests().len(), 1);
        // The query asks for the small field set only.
        assert!(remote.requests()[0].path.contains("expand%5B%5D=createdAt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn details_for_a_library_model_read_the_library_page() {
        let ollama = ollama_endpoint_mock();
        let remote = MockServer::start(|request| {
            if request.path == "/ol/library/gemma4:e4b" {
                MockReply::text(200, OLLAMA_PAGE)
            } else {
                MockReply::error(404, "x")
            }
        });
        let details = build_model_details(
            &ollama.base_url,
            "gemma4:e4b",
            None,
            &sources_for(&remote),
            1,
        );
        let info = details.remote.expect("library data");
        assert_eq!(info.source, "ollama");
        assert_eq!(info.updated_at.as_deref(), Some("2026-10-12T22:18:00Z"));
        assert_eq!(details.local.unwrap().context_length, Some(131072));
    }

    #[test]
    fn offline_remote_leaves_the_local_data_and_a_note() {
        let ollama = ollama_endpoint_mock();
        let dead = {
            // A port nothing listens on.
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            format!("http://127.0.0.1:{}", port)
        };
        let sources = RemoteSources {
            hf_base: dead.clone(),
            ollama_base: dead,
            policy: Box::new(|url: &Url| url.scheme() == "http"),
            total_timeout: Duration::from_millis(800),
        };
        let details = build_model_details(&ollama.base_url, "gemma4:e4b", None, &sources, 1);
        assert!(details.local.is_some());
        assert!(details.remote.is_none());
        assert!(details
            .remote_note
            .unwrap()
            .starts_with("Online details unavailable"));
    }

    #[test]
    fn a_failed_refresh_shows_the_older_cache_as_stale() {
        let ollama = ollama_endpoint_mock();
        let dir = temp_dir("stale");
        let good = MockServer::start(|_| MockReply::ok(hf_sample()));
        let model = "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M";
        build_model_details(
            &ollama.base_url,
            model,
            Some(&dir),
            &sources_for(&good),
            1_000,
        );
        let broken = MockServer::start(|_| MockReply::error(503, "down"));
        let later = 1_000 + CACHE_TTL_MS + 10;
        let details = build_model_details(
            &ollama.base_url,
            model,
            Some(&dir),
            &sources_for(&broken),
            later,
        );
        assert!(details.remote.is_some());
        assert!(details.remote_stale);
        assert!(details.remote_cached);
        assert_eq!(details.remote_fetched_at_ms, Some(1_000));
        assert!(details.remote_note.unwrap().contains("Showing saved ones"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_remote_page_is_a_quiet_note_not_an_error() {
        let ollama = ollama_endpoint_mock();
        let remote = MockServer::start(|_| MockReply::error(404, "x"));
        let details = build_model_details(
            &ollama.base_url,
            "mine:custom",
            None,
            &sources_for(&remote),
            1,
        );
        assert!(details.remote.is_none());
        assert!(details.remote_note.unwrap().starts_with("Not listed on"));
    }

    #[test]
    fn hostile_remote_answers_are_survived() {
        let ollama = ollama_endpoint_mock();
        for (label, reply) in [
            ("html", MockReply::text(200, "<html>login</html>")),
            ("giant", MockReply::text(200, &"{".repeat(400_000))),
            ("401", MockReply::error(401, "x")),
            (
                "redirect",
                MockReply::text(302, "").with_header("Location", "http://evil.invalid/x"),
            ),
        ] {
            let reply = Arc::new(Mutex::new(Some(reply)));
            let remote = MockServer::start(move |_| {
                let mut slot = reply.lock().unwrap();
                match slot.take() {
                    Some(reply) => reply,
                    None => MockReply::error(500, "again"),
                }
            });
            let details = build_model_details(
                &ollama.base_url,
                "hf.co/a/b:Q4",
                None,
                &sources_for(&remote),
                1,
            );
            assert!(details.remote.is_none(), "{label}");
            assert!(details.remote_note.is_some(), "{label}");
            assert!(details.local.is_some(), "{label}");
        }
    }

    #[test]
    fn ollama_down_leaves_a_local_error_but_still_tries_the_web() {
        let remote = MockServer::start(|_| MockReply::ok(hf_sample()));
        let dead = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            format!("http://127.0.0.1:{}", port)
        };
        let details = build_model_details(&dead, "hf.co/a/b:Q4", None, &sources_for(&remote), 1);
        assert!(details.local.is_none());
        assert!(details.local_error.unwrap().contains("not reachable"));
        assert!(details.remote.is_some());
    }

    #[test]
    fn model_not_installed_reports_that() {
        let ollama = ollama_endpoint_mock();
        let remote = MockServer::start(|_| MockReply::error(404, "x"));
        let details = build_model_details(
            &ollama.base_url,
            "missing:model",
            None,
            &sources_for(&remote),
            1,
        );
        assert!(details.local.is_none());
        assert!(details.local_error.unwrap().contains("not installed"));
    }

    #[test]
    fn concurrent_requests_for_one_model_fetch_once() {
        let ollama = ollama_endpoint_mock();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let remote = MockServer::start(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            MockReply::ok(hf_sample()).delayed(Duration::from_millis(250))
        });
        let dir = temp_dir("flight");
        let ollama_url = ollama.base_url.clone();
        let remote_url = remote.base_url.clone();
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let dir = dir.clone();
                let ollama_url = ollama_url.clone();
                let remote_url = remote_url.clone();
                std::thread::spawn(move || {
                    let allowed = Url::parse(&remote_url).unwrap();
                    let sources = RemoteSources {
                        hf_base: format!("{}/hf", remote_url),
                        ollama_base: format!("{}/ol", remote_url),
                        policy: Box::new(move |url: &Url| {
                            url.host_str() == allowed.host_str() && url.port() == allowed.port()
                        }),
                        total_timeout: Duration::from_secs(3),
                    };
                    build_model_details(
                        &ollama_url,
                        "hf.co/single/flight:Q4",
                        Some(&dir),
                        &sources,
                        1_000,
                    )
                })
            })
            .collect();
        for handle in handles {
            assert!(handle.join().unwrap().remote.is_some());
        }
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "one fetch, the others read the cache"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unparseable_names_have_no_remote_page() {
        let ollama = ollama_endpoint_mock();
        let details = build_model_details(
            &ollama.base_url,
            "a/b/c/d",
            None,
            &RemoteSources::production(),
            1,
        );
        assert!(details.remote.is_none());
        assert!(details.remote_note.unwrap().contains("no public page"));
    }

    // ───── live (real endpoints) ─────

    /// `cargo test --manifest-path src-tauri/Cargo.toml --lib -- live_model_details --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_model_details_read_the_real_endpoints() {
        let sources = RemoteSources::production();
        let hf = fetch_hf_details(
            HUGGINGFACE_BASE,
            "unsloth/gemma-4-E4B-it-GGUF",
            sources.policy.as_ref(),
            Duration::from_secs(8),
        )
        .expect("Hugging Face API");
        println!("HF: {hf:#?}");
        assert!(hf
            .released_at
            .as_deref()
            .is_some_and(|d| d.starts_with("2026-")));
        assert!(hf.downloads.unwrap_or(0) > 1000);
        assert_eq!(hf.license.as_deref(), Some("apache-2.0"));

        let model = parse_model_ref("gemma4:e4b").unwrap();
        let library = fetch_ollama_library_details(
            OLLAMA_BASE,
            &model,
            sources.policy.as_ref(),
            Duration::from_secs(8),
        )
        .expect("ollama.com library page");
        println!("Ollama: {library:#?}");
        assert!(library.description.is_some(), "meta description");
        assert!(library.updated_at.is_some(), "Updated title attribute");
        assert!(library.downloads_text.is_some(), "download count");

        let missing = fetch_hf_details(
            HUGGINGFACE_BASE,
            "nonexistent-org/zzz-nope",
            sources.policy.as_ref(),
            Duration::from_secs(8),
        );
        assert_eq!(missing.unwrap_err(), HttpFail::Denied);
        let missing_page = fetch_ollama_library_details(
            OLLAMA_BASE,
            &parse_model_ref("zzzznope:latest").unwrap(),
            sources.policy.as_ref(),
            Duration::from_secs(8),
        );
        assert_eq!(missing_page.unwrap_err(), HttpFail::NotFound);
    }

    /// Local + public data for a model that is installed on a real Ollama.
    /// `TRISPR_LIVE_OLLAMA=http://127.0.0.1:11500 TRISPR_LIVE_MODEL=gemma4:e4b cargo test --lib -- live_model_details_full --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_model_details_full() {
        let endpoint = std::env::var("TRISPR_LIVE_OLLAMA")
            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
        let model = std::env::var("TRISPR_LIVE_MODEL").unwrap_or_else(|_| "gemma4:e4b".to_string());
        let dir = temp_dir("live-full");
        let details = build_model_details(
            &endpoint,
            &model,
            Some(&dir),
            &RemoteSources::production(),
            crate::util::now_ms(),
        );
        println!("{details:#?}");
        let local = details.local.expect("local data from /api/show");
        assert!(local.context_length.is_some());
        assert!(details.remote.is_some(), "{:?}", details.remote_note);
        let again = build_model_details(
            &endpoint,
            &model,
            Some(&dir),
            &RemoteSources::production(),
            crate::util::now_ms(),
        );
        assert!(again.remote_cached, "second call comes from the disk cache");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
