use super::error::AIError;
use super::models::{RefinementOptions, RefinementResult, TierUpdate, TokenUsage};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tracing::{error, info, warn};
use url::Url;

/// Shared ureq agent for general Ollama HTTP calls (list models, ping, inference).
/// Defaults: connect 5 s, read 60 s — generous enough for inference, snappy enough
/// for metadata queries.  `ping_ollama_quick` and `pull_ollama_model_inner` have
/// fundamentally different timeout budgets and keep their own agents.
static UREQ_AGENT: OnceLock<ureq::Agent> = OnceLock::new();

// Single-flight ping cache for ping_ollama_quick.
// Prevents parallel startup threads (Whisper pre-warm, dependency preflight, frontend
// get_runtime_diagnostics) from all blocking for 300 ms simultaneously.
// TTL is 3 s — short enough to detect Ollama coming online within a few seconds.
static QUICK_PING_CACHE_TS: AtomicU64 = AtomicU64::new(0);
static QUICK_PING_CACHE_OK: AtomicBool = AtomicBool::new(false);
const QUICK_PING_CACHE_TTL_MS: u64 = 3_000;

fn shared_agent() -> &'static ureq::Agent {
    UREQ_AGENT.get_or_init(|| {
        ureq::builder()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(60))
            .build()
    })
}

/// Returns a per-request ureq agent with a short, fixed read timeout.
/// Keep this below the frontend paste fallback timeout so local inference cannot
/// keep the GPU hot after the user already received raw text.
fn refinement_agent(input_text: &str, system_prompt: &str) -> ureq::Agent {
    let timeout_secs = refinement_timeout_secs(input_text, system_prompt);
    ureq::builder()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(timeout_secs))
        .build()
}

fn refinement_timeout_secs(_input_text: &str, _system_prompt: &str) -> u64 {
    25
}

// Prompt templates optimized for local models (Ollama: qwen3, mistral-small).
// Guidelines: no translation, no explanations, preserve register and proper nouns.
pub const OLLAMA_PROMPT_EN: &str = "You are a transcript editor. Fix punctuation, capitalization, and obvious speech-to-text errors in the transcript provided in the <transcript> tags. Rules: do NOT translate; do NOT add explanations or commentary; preserve all proper nouns and technical terms exactly; preserve the original register (formal/informal); do NOT add line breaks, paragraph breaks, or tabs that are not in the original. Output ONLY the corrected text with no preamble.";

pub const OLLAMA_PROMPT_DE: &str = "Du bist ein Transkript-Editor. Korrigiere Zeichensetzung, Groß-/Kleinschreibung und offensichtliche Sprache-zu-Text-Fehler im Transkript in den <transcript> Tags. Regeln: NICHT übersetzen; KEINE Erklärungen oder Kommentare hinzufügen; alle Eigennamen und Fachbegriffe exakt beibehalten; Anredeform (Du/Sie) aus dem Original beibehalten; KEINE zusätzlichen Zeilenumbrüche, Absätze oder Tabulatoren einfügen. Gib NUR den korrigierten Text aus, ohne Einleitung.";

// Used when Whisper language is set to auto-detect. Language-neutral phrasing avoids
// the model inferring "output in English" from an English-only system prompt.
pub const OLLAMA_PROMPT_AUTO: &str = "IMPORTANT: Detect the language of the input text and output in that SAME language — never translate. You are a transcript editor. Fix punctuation, capitalization, and obvious speech-to-text errors in the transcript provided in the <transcript> tags. Rules: do NOT translate under any circumstances; do NOT add explanations; preserve all proper nouns and technical terms exactly; preserve the original register; do NOT add line breaks, paragraph breaks, or tabs not present in the original. Output ONLY the corrected text with no preamble.";

const OLLAMA_PROMPT_SUMMARY_EN: &str = "Summarize this transcript into 3 to 6 concise bullet points. Preserve key facts, numbers, names, and decisions. Do not invent information. If something is uncertain, state it cautiously. Return only the bullet list.";

const OLLAMA_PROMPT_SUMMARY_DE: &str = "Fasse dieses Transkript in 3 bis 6 praegnanten Stichpunkten zusammen. Behalte wichtige Fakten, Zahlen, Namen und Entscheidungen bei. Keine Informationen erfinden. Unsichere Inhalte vorsichtig formulieren. Gib nur die Stichpunktliste zurueck.";

const OLLAMA_PROMPT_TECHNICAL_EN: &str = "Rewrite this transcript in technical specification style. Keep exact numbers, units, versions, APIs, constraints, and file paths. Structure output with short sections: Goal, Inputs, Outputs, Constraints, Open Questions. Do not invent missing values. Return only the structured result.";

const OLLAMA_PROMPT_TECHNICAL_DE: &str = "Formuliere dieses Transkript als technische Spezifikation um. Behalte exakte Zahlen, Einheiten, Versionen, APIs, Rahmenbedingungen und Dateipfade. Strukturiere die Ausgabe mit kurzen Abschnitten: Ziel, Eingaben, Ausgaben, Constraints, Offene Fragen. Keine fehlenden Werte erfinden. Gib nur das strukturierte Ergebnis zurueck.";

const OLLAMA_PROMPT_ACTION_ITEMS_EN: &str = "Convert this transcript into actionable tasks. Use bullets with format: [Action] [Owner?] [Due?] [Notes]. Preserve technical wording and constraints. If owner or due date is missing, mark it as unknown. Return only the action list.";

const OLLAMA_PROMPT_ACTION_ITEMS_DE: &str = "Wandle dieses Transkript in konkrete Aufgaben um. Nutze Stichpunkte im Format: [Aktion] [Owner?] [Faellig?] [Hinweise]. Behalte technische Begriffe und Rahmenbedingungen bei. Wenn Owner oder Datum fehlen, mit unknown markieren. Gib nur die Aufgabenliste zurueck.";

const OLLAMA_PROMPT_LLM_PROMPT_EN: &str = "You are an expert prompt engineer. Convert the transcript in the <transcript> tags into a precise, high-quality prompt for a large language model.\n\nA well-structured prompt must include:\n1. A clear role or persona (e.g. \"You are a...\")\n2. An unambiguous task description\n3. Relevant context, background, or constraints\n4. Desired output format (if applicable)\n\nRules:\n- Always write the resulting prompt in English, regardless of the input language.\n- Do not explain, comment, or add preamble.\n- Do not address the speaker or reference this conversation.\n- Return only the final ready-to-use prompt, nothing else.";

const OLLAMA_PROMPT_LLM_PROMPT_DE: &str = "Du bist ein erfahrener Prompt-Engineer. Wandle das Transkript in den <transcript> Tags in einen praezisen, einsatzbereiten Prompt fuer ein grosses Sprachmodell um.\n\nEin guter Prompt enthaelt:\n1. Eine klare Rolle oder Persona (z.B. \"You are a...\")\n2. Eine eindeutige Aufgabenbeschreibung\n3. Relevanten Kontext, Hintergrund oder Einschraenkungen\n4. Das gewuenschte Ausgabeformat (falls zutreffend)\n\nRegeln:\n- Schreibe den fertigen Prompt immer auf Englisch, unabhaengig von der Eingabesprache.\n- Keine Erklaerungen, Kommentare oder Vorbemerkungen.\n- Den Sprecher nicht adressieren und nicht auf dieses Gespraech verweisen.\n- Gib nur den fertigen Prompt zurueck, nichts weiteres.";

fn normalize_ollama_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "http://127.0.0.1:11434".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Check if an endpoint targets a known SSRF-sensitive address
/// (cloud metadata services, link-local ranges).
pub fn is_ssrf_target(endpoint: &str) -> bool {
    let normalized = normalize_ollama_endpoint(endpoint);
    let parsed = match Url::parse(&normalized) {
        Ok(url) => url,
        Err(_) => return true, // Fail-closed: unparseable URLs treated as SSRF targets
    };
    // Use typed host parsing to avoid IPv6 bracket handling quirks in host_str().
    match parsed.host() {
        None => return true, // Fail-closed: parsed URL without host
        Some(url::Host::Domain(host)) => {
            let host = host.to_ascii_lowercase();
            // Block cloud metadata endpoint (AWS/GCP/Azure)
            if host == "169.254.169.254" || host == "metadata.google.internal" {
                return true;
            }
        }
        Some(url::Host::Ipv4(ip)) => {
            // Block IPv4 link-local range (169.254.x.x) entirely.
            if ip.octets()[0] == 169 && ip.octets()[1] == 254 {
                return true;
            }
        }
        Some(url::Host::Ipv6(ip)) => {
            // fe80::/10 — first 10 bits are 1111111010
            if (ip.segments()[0] & 0xffc0) == 0xfe80 {
                return true;
            }
            // ::ffff:169.254.x.x — IPv4-mapped link-local
            if let Some(ipv4) = ip.to_ipv4_mapped() {
                if ipv4.octets()[0] == 169 && ipv4.octets()[1] == 254 {
                    return true;
                }
            }
        }
    }
    false
}

/// Return preferred endpoint plus a localhost/127.0.0.1 fallback variant.
pub fn ollama_endpoint_candidates(endpoint: &str) -> Vec<String> {
    let primary = normalize_ollama_endpoint(endpoint);
    let mut candidates = Vec::with_capacity(2);
    let mut push_unique = |value: String| {
        if !candidates.iter().any(|existing| existing == &value) {
            candidates.push(value);
        }
    };

    if let Ok(parsed) = Url::parse(&primary) {
        let host = parsed.host_str().map(|h| h.to_ascii_lowercase());
        if matches!(host.as_deref(), Some("localhost") | Some("127.0.0.1")) {
            // Prefer numeric loopback first. On some Windows setups, localhost
            // resolution for /api/tags can add ~2s per call.
            let mut ipv4 = parsed.clone();
            if ipv4.set_host(Some("127.0.0.1")).is_ok() {
                push_unique(ipv4.to_string().trim_end_matches('/').to_string());
            }

            let mut localhost = parsed.clone();
            if localhost.set_host(Some("localhost")).is_ok() {
                push_unique(localhost.to_string().trim_end_matches('/').to_string());
            }

            push_unique(primary);
            return candidates;
        }
    }

    push_unique(primary);
    candidates
}

/// Fetch model list from a running Ollama instance via GET /api/tags.
/// Returns an empty Vec on any error (Ollama not running, network issue, etc.).
pub fn list_ollama_models(endpoint: &str) -> Vec<String> {
    list_ollama_models_with_size(endpoint)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// Fetch model list with size information from a running Ollama instance.
/// Each entry is (model_name, size_bytes). Returns empty Vec on any error.
pub fn list_ollama_models_with_size(endpoint: &str) -> Vec<(String, u64)> {
    let agent = shared_agent();

    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/tags", candidate);
        let resp = match agent.get(&url).call() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let json: serde_json::Value = match resp.into_json() {
            Ok(j) => j,
            Err(_) => continue,
        };
        return json["models"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let name = m["name"].as_str()?.to_string();
                        let size = m["size"].as_u64().unwrap_or(0);
                        Some((name, size))
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    vec![]
}

/// Full inventory from `/api/tags`: name, size, digest, pull time and model details.
/// Returns an empty Vec on any error.
pub fn list_ollama_models_detailed(endpoint: &str) -> Vec<serde_json::Value> {
    let agent = shared_agent();

    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/tags", candidate);
        let Ok(resp) = agent.get(&url).call() else {
            continue;
        };
        let Ok(json) = resp.into_json::<serde_json::Value>() else {
            continue;
        };
        return json["models"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let name = m["name"].as_str()?;
                        Some(serde_json::json!({
                            "name": name,
                            "size_bytes": m["size"].as_u64().unwrap_or(0),
                            "digest": m["digest"].as_str().unwrap_or(""),
                            "modified_at": m["modified_at"].as_str().unwrap_or(""),
                            "family": m["details"]["family"].as_str().unwrap_or(""),
                            "parameter_size": m["details"]["parameter_size"].as_str().unwrap_or(""),
                            "quantization_level": m["details"]["quantization_level"].as_str().unwrap_or(""),
                        }))
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    vec![]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OllamaModelLoadStatus {
    pub loaded: bool,
    pub size_vram: u64,
}

fn ollama_model_name_matches(candidate: &str, target: &str) -> bool {
    !target.trim().is_empty() && candidate.trim().eq_ignore_ascii_case(target.trim())
}

fn parse_ollama_model_load_status(
    json: &serde_json::Value,
    model: &str,
) -> Option<OllamaModelLoadStatus> {
    let models = json["models"].as_array()?;
    let mut loaded = false;
    let mut size_vram = 0u64;

    for entry in models {
        let name_matches = entry["name"]
            .as_str()
            .map(|name| ollama_model_name_matches(name, model))
            .unwrap_or(false);
        let model_matches = entry["model"]
            .as_str()
            .map(|name| ollama_model_name_matches(name, model))
            .unwrap_or(false);

        if name_matches || model_matches {
            loaded = true;
            size_vram = size_vram.saturating_add(entry["size_vram"].as_u64().unwrap_or(0));
        }
    }

    Some(OllamaModelLoadStatus { loaded, size_vram })
}

/// Return whether the configured Ollama model is currently loaded via /api/ps.
/// `size_vram` is GPU/offload telemetry only; CPU-loaded models report 0.
pub fn fetch_ollama_model_load_status(
    endpoint: &str,
    model: &str,
) -> Option<OllamaModelLoadStatus> {
    let agent = ureq::builder()
        .timeout_connect(Duration::from_millis(500))
        .timeout_read(Duration::from_millis(750))
        .build();
    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/ps", candidate);
        let Ok(resp) = agent.get(&url).call() else {
            continue;
        };
        let Ok(json) = resp.into_json::<serde_json::Value>() else {
            continue;
        };
        if let Some(status) = parse_ollama_model_load_status(&json, model) {
            return Some(status);
        }
    }
    None
}

/// Test whether Ollama is reachable at the given endpoint.
/// Returns Ok(()) if the server responds, Err(AIError::OllamaNotRunning) otherwise.
pub fn ping_ollama(endpoint: &str) -> Result<(), AIError> {
    let agent = shared_agent();

    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/tags", candidate);
        if agent.get(&url).call().is_ok() {
            return Ok(());
        }
    }

    Err(AIError::OllamaNotRunning)
}

/// Quick reachability check for use in UI detection paths (e.g. detect_ollama_runtime).
/// Uses a 300 ms timeout — enough for localhost, never blocks the UI thread noticeably.
///
/// Results are cached for QUICK_PING_CACHE_TTL_MS (3 s) so that parallel startup
/// threads (Whisper pre-warm, dependency preflight, frontend status poll) all share
/// the same network round-trip instead of each blocking independently.
pub fn ping_ollama_quick(endpoint: &str) -> Result<(), AIError> {
    let now = crate::util::now_ms();
    let cached_ts = QUICK_PING_CACHE_TS.load(Ordering::Relaxed);
    if now.saturating_sub(cached_ts) < QUICK_PING_CACHE_TTL_MS {
        return if QUICK_PING_CACHE_OK.load(Ordering::Relaxed) {
            Ok(())
        } else {
            Err(AIError::OllamaNotRunning)
        };
    }

    let result = ping_ollama_quick_fresh(endpoint);
    QUICK_PING_CACHE_OK.store(result.is_ok(), Ordering::Relaxed);
    QUICK_PING_CACHE_TS.store(now, Ordering::Relaxed);
    result
}

/// The probe of `ping_ollama_quick` without the cache, and without touching it. Whoever starts
/// or stops a server must ask this: the cached answer (3 s, not even per endpoint) still
/// describes the server as it was before, so a freshly spawned server looked "ready" at once
/// (measured: after a stop, a new process that cannot even start was reported ready after
/// 5 microseconds) and a stopped one still looked alive.
pub fn ping_ollama_quick_fresh(endpoint: &str) -> Result<(), AIError> {
    let agent = ureq::builder()
        .timeout_connect(Duration::from_millis(300))
        .timeout_read(Duration::from_millis(300))
        .build();

    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/tags", candidate);
        if agent.get(&url).call().is_ok() {
            return Ok(());
        }
    }
    Err(AIError::OllamaNotRunning)
}

/// Forgets the cached `ping_ollama_quick` result. Called after a server was started or
/// stopped, so the next status poll shows the server as it is now.
pub fn reset_quick_ping_cache() {
    QUICK_PING_CACHE_TS.store(0, Ordering::Relaxed);
}

pub fn default_prompt_for_language(language: &str) -> &'static str {
    match language.trim().to_lowercase().as_str() {
        "de" | "german" => OLLAMA_PROMPT_DE,
        "auto" | "" => OLLAMA_PROMPT_AUTO,
        _ => OLLAMA_PROMPT_EN,
    }
}

fn language_lock_suffix(language: &str) -> &'static str {
    match language.trim().to_lowercase().as_str() {
        "auto" => {
            "Detect the input language and keep it unchanged. Do not translate. If the input is mixed-language, preserve each segment in its original language."
        }
        "de" | "german" => {
            "Behalte die Ausgabe in derselben Sprache wie die Eingabe. Nicht uebersetzen."
        }
        _ => "Keep the output in the same language as the input. Do not translate.",
    }
}

fn with_language_lock(prompt: &str, language: &str, preserve_source_language: bool) -> String {
    let normalized = prompt.trim();
    if normalized.is_empty() {
        return String::new();
    }
    if !preserve_source_language {
        return normalized.to_string();
    }
    // Prepend the language lock so it has higher precedence than the main prompt body.
    // Models weight early instructions more heavily than late ones.
    format!("{}\n\n{}", language_lock_suffix(language), normalized)
}

pub fn normalize_prompt_profile(profile: &str) -> &'static str {
    let id = crate::ai_fallback::models::normalize_prompt_profile_id(profile);
    if id.is_empty() {
        "wording"
    } else {
        id
    }
}

pub fn prompt_for_profile(
    profile: &str,
    language: &str,
    custom_prompt: Option<&str>,
    preserve_source_language: bool,
) -> Option<String> {
    if normalize_prompt_profile(profile) == "custom" {
        let normalized = custom_prompt.unwrap_or("").trim();
        if normalized.is_empty() {
            return Some(with_language_lock(
                default_prompt_for_language(language),
                language,
                preserve_source_language,
            ));
        }
        // Custom prompts are user-owned and must remain unchanged.
        return Some(normalized.to_string());
    }

    let base = match normalize_prompt_profile(profile) {
        "summary" => match language.trim().to_lowercase().as_str() {
            "de" | "german" => OLLAMA_PROMPT_SUMMARY_DE,
            _ => OLLAMA_PROMPT_SUMMARY_EN,
        }
        .to_string(),
        "technical_specs" => match language.trim().to_lowercase().as_str() {
            "de" | "german" => OLLAMA_PROMPT_TECHNICAL_DE,
            _ => OLLAMA_PROMPT_TECHNICAL_EN,
        }
        .to_string(),
        "action_items" => match language.trim().to_lowercase().as_str() {
            "de" | "german" => OLLAMA_PROMPT_ACTION_ITEMS_DE,
            _ => OLLAMA_PROMPT_ACTION_ITEMS_EN,
        }
        .to_string(),
        "llm_prompt" => match language.trim().to_lowercase().as_str() {
            "de" | "german" => OLLAMA_PROMPT_LLM_PROMPT_DE,
            _ => OLLAMA_PROMPT_LLM_PROMPT_EN,
        }
        .to_string(),
        _ => default_prompt_for_language(language).to_string(),
    };

    // llm_prompt always outputs in English, so skip language lock
    if normalize_prompt_profile(profile) == "llm_prompt" {
        return if base.is_empty() { None } else { Some(base) };
    }

    let effective = with_language_lock(&base, language, preserve_source_language);
    if effective.is_empty() {
        None
    } else {
        Some(effective)
    }
}

fn rough_token_estimate(text: &str) -> usize {
    let words = text.split_whitespace().count();
    (words as f64 * 1.35).ceil().max(1.0) as usize
}

pub(super) fn extract_ollama_error_message(response: ureq::Response) -> String {
    let body = response.into_string().unwrap_or_default();
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(message) = json["error"].as_str() {
            return message.to_string();
        }
        if let Some(message) = json["message"].as_str() {
            return message.to_string();
        }
    }
    trimmed.to_string()
}

fn is_ollama_model_not_found(detail: &str) -> bool {
    let detail_lc = detail.to_ascii_lowercase();
    detail_lc.contains("model") && detail_lc.contains("not found")
}

fn is_missing_chat_route(code: u16, detail: &str) -> bool {
    if code == 404 {
        return true;
    }
    let detail_lc = detail.to_ascii_lowercase();
    detail_lc.contains("/api/chat")
        || detail_lc.contains("unknown route")
        || detail_lc.contains("not found")
        || detail_lc.contains("messages not supported")
}

fn parse_ollama_refined_text(json: &serde_json::Value) -> Option<String> {
    json["message"]["content"]
        .as_str()
        .or_else(|| json["response"].as_str())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

struct OllamaUsage {
    input_tokens: usize,
    output_tokens: usize,
    load_ms: Option<u64>,
    prompt_eval_ms: Option<u64>,
    eval_ms: Option<u64>,
}

fn parse_ollama_usage(json: &serde_json::Value) -> OllamaUsage {
    let ns_to_ms = |ns: u64| ns / 1_000_000;
    OllamaUsage {
        input_tokens: json["prompt_eval_count"].as_u64().unwrap_or(0) as usize,
        output_tokens: json["eval_count"].as_u64().unwrap_or(0) as usize,
        load_ms: json["load_duration"].as_u64().map(ns_to_ms),
        prompt_eval_ms: json["prompt_eval_duration"].as_u64().map(ns_to_ms),
        eval_ms: json["eval_duration"].as_u64().map(ns_to_ms),
    }
}

fn parse_env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }
        trimmed.parse::<usize>().ok()
    })
}

fn default_ollama_num_thread() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    // Keep headroom for UI/audio threads.
    (cores / 2).max(2).clamp(2, 8)
}

/// Output-budget tiers for `num_predict`, shared by every prompt profile.
/// Growing to a higher tier costs nothing extra — unlike `num_ctx`, changing
/// `num_predict` does NOT change the Ollama runner identity, so no reload.
/// Tier 1 (384) matches the old fixed ceiling for "summary"/"action_items",
/// kept as the starting point so behavior is unchanged until a longer input
/// actually demonstrates the need for more.
pub const NUM_PREDICT_TIERS: [u32; 6] = [160, 384, 768, 1536, 3072, 6144];

/// Context-window tiers for `num_ctx`. Unlike NUM_PREDICT_TIERS, stepping up
/// here forces a full model reload (~4-5 s) because Ollama keys the loaded
/// runner on (model, num_ctx). Tier 0 (2048) is the long-standing default,
/// sized for typical dictation transcripts; higher tiers are only requested
/// when a transcript genuinely threatens to overflow the current window.
pub const NUM_CTX_TIERS: [usize; 4] = [2048, 4096, 8192, 16384];

/// Fraction of the current `num_ctx` tier that input+output may occupy before
/// stepping up. Leaves headroom for chat-template overhead and the XML
/// wrapper tags, which `rough_token_estimate` (word-based) doesn't account for.
const NUM_CTX_SAFETY_MARGIN: f64 = 0.85;

/// Resolves a persisted tier index to its `num_ctx` value. Used by the PTT
/// warmup path, which must send the byte-identical `num_ctx` the refinement
/// path will use next — otherwise warmup warms the wrong runner and the next
/// real refinement pays the reload cost anyway (see `ollama_runner_defining_options`).
pub fn num_ctx_for_tier(tier_idx: usize) -> usize {
    NUM_CTX_TIERS[tier_idx.min(NUM_CTX_TIERS.len() - 1)]
}

/// Smallest tier index whose value covers `need`; the last tier if none do.
fn tier_index_for_need_u32(tiers: &[u32], need: u32) -> usize {
    tiers
        .iter()
        .position(|&tier| tier >= need)
        .unwrap_or(tiers.len() - 1)
}

fn tier_index_for_need_usize(tiers: &[usize], need: usize) -> usize {
    tiers
        .iter()
        .position(|&tier| tier >= need)
        .unwrap_or(tiers.len() - 1)
}

/// Monotonic tier growth: never steps down. When `need_idx` outgrows the
/// persisted tier, jumps one tier PAST what's needed right now, so the next
/// slightly-longer input doesn't immediately hit the ceiling again.
fn grow_tier(persisted_idx: usize, need_idx: usize, max_idx: usize) -> usize {
    if need_idx <= persisted_idx {
        persisted_idx.min(max_idx)
    } else {
        (need_idx + 1).min(max_idx)
    }
}

/// Raw output-length need for a given profile, independent of any ceiling.
/// Basing this on input size (not the system prompt) keeps tiny jobs tiny —
/// see `local_wording_num_predict_uses_input_not_prompt_size` below.
fn adaptive_num_predict_need(
    input_text: &str,
    prompt_profile: &str,
    low_latency_mode: bool,
) -> u32 {
    let input_tokens = rough_token_estimate(input_text);
    let profile = normalize_prompt_profile(prompt_profile);
    match profile {
        "wording" => {
            let multiplier = if low_latency_mode { 2 } else { 3 };
            ((input_tokens * multiplier) + 32).max(48) as u32
        }
        "summary" | "action_items" => ((input_tokens * 2) + 96).max(96) as u32,
        "technical_specs" | "llm_prompt" => ((input_tokens * 3) + 160).max(160) as u32,
        "custom" => ((input_tokens * 3) + 96).max(96) as u32,
        _ => ((input_tokens * 3) + 32).max(64) as u32,
    }
}

/// Resolves the `num_predict` value to send for THIS request and the tier
/// index to persist going forward. `configured_max` (the user's own "Token
/// Limit" setting) remains the absolute ceiling: tier growth only expands the
/// room available below it, it never overrides an explicit user cap.
pub fn resolve_num_predict(
    input_text: &str,
    prompt_profile: &str,
    configured_max: u32,
    low_latency_mode: bool,
    persisted_tier_idx: usize,
) -> (u32, usize) {
    let configured = configured_max.clamp(128, 8192);
    let need = adaptive_num_predict_need(input_text, prompt_profile, low_latency_mode);
    let max_idx = NUM_PREDICT_TIERS.len() - 1;
    let need_idx = tier_index_for_need_u32(&NUM_PREDICT_TIERS, need);
    let tier_idx = grow_tier(persisted_tier_idx.min(max_idx), need_idx, max_idx);
    let num_predict = need
        .min(NUM_PREDICT_TIERS[tier_idx])
        .min(configured)
        .max(48);
    (num_predict, tier_idx)
}

/// Resolves the `num_ctx` value to send for THIS request and the tier index
/// to persist. Unlike `resolve_num_predict`, stepping up here is expensive
/// (forces a runner reload), so it only happens when input+prompt+output
/// genuinely threatens to exceed NUM_CTX_SAFETY_MARGIN of the current tier.
pub fn resolve_num_ctx(
    system_prompt: &str,
    input_text: &str,
    num_predict: u32,
    persisted_tier_idx: usize,
) -> (usize, usize) {
    let max_idx = NUM_CTX_TIERS.len() - 1;
    let current_idx = persisted_tier_idx.min(max_idx);
    let estimated_total = rough_token_estimate(system_prompt)
        + rough_token_estimate(input_text)
        + num_predict as usize;
    let safe_budget = (NUM_CTX_TIERS[current_idx] as f64 * NUM_CTX_SAFETY_MARGIN) as usize;
    let tier_idx = if estimated_total > safe_budget {
        let need = (estimated_total as f64 / NUM_CTX_SAFETY_MARGIN).ceil() as usize;
        let need_idx = tier_index_for_need_usize(&NUM_CTX_TIERS, need);
        grow_tier(current_idx, need_idx, max_idx)
    } else {
        current_idx
    };
    (NUM_CTX_TIERS[tier_idx], tier_idx)
}

/// The subset of OLLAMA options that determine *which runner* is loaded.
/// Changing any of these — num_ctx, num_thread, num_gpu — forces a full model
/// reload (~4-5 s for a 9B model). The warmup path and the refinement path MUST
/// send byte-identical values here, otherwise the warmed runner is evicted and
/// the first real refinement pays the cold-load cost, defeating the warmup.
/// Sampling options (temperature, num_predict) do NOT affect runner identity and
/// are layered on top by the refinement path only.
pub fn ollama_runner_defining_options(
    num_ctx: usize,
) -> serde_json::Map<String, serde_json::Value> {
    let mut payload = serde_json::Map::new();

    // An env override must also be a single stable value — varying it
    // per-request reintroduces reloads. Takes precedence over tiering.
    let num_ctx = parse_env_usize("TRISPR_OLLAMA_NUM_CTX")
        .map(|n| n.clamp(1024, 16384))
        .unwrap_or(num_ctx);
    payload.insert("num_ctx".to_string(), serde_json::json!(num_ctx));

    let num_thread =
        parse_env_usize("TRISPR_OLLAMA_NUM_THREAD").unwrap_or_else(default_ollama_num_thread);
    payload.insert("num_thread".to_string(), serde_json::json!(num_thread));

    // Optional advanced override. Example: TRISPR_OLLAMA_NUM_GPU=999
    // to request aggressive GPU offload if supported by local runtime.
    if let Some(num_gpu) = parse_env_usize("TRISPR_OLLAMA_NUM_GPU") {
        payload.insert("num_gpu".to_string(), serde_json::json!(num_gpu));
    }

    payload
}

fn build_ollama_options_payload(
    options: &RefinementOptions,
    input_text: &str,
    system_prompt: &str,
) -> (serde_json::Value, TierUpdate) {
    let (num_predict, predict_tier_idx) = resolve_num_predict(
        input_text,
        &options.prompt_profile,
        options.max_tokens,
        options.low_latency_mode,
        options.num_predict_tier_idx,
    );
    let (num_ctx, ctx_tier_idx) = resolve_num_ctx(
        system_prompt,
        input_text,
        num_predict,
        options.num_ctx_tier_idx,
    );

    // Start from the runner-defining options so the warmup and refinement
    // runners are identical, then layer sampling options on top.
    let mut payload = ollama_runner_defining_options(num_ctx);
    payload.insert(
        "temperature".to_string(),
        serde_json::json!(options.temperature),
    );
    payload.insert("num_predict".to_string(), serde_json::json!(num_predict));

    (
        serde_json::Value::Object(payload),
        TierUpdate {
            num_predict_tier_idx: predict_tier_idx,
            num_ctx_tier_idx: ctx_tier_idx,
        },
    )
}

pub(super) fn wrap_transcript_in_xml(text: &str) -> String {
    // Use HTML entity escaping to prevent XML tag injection through spoken text
    let escaped = text.replace("</transcript>", "&lt;/transcript&gt;");
    format!("<transcript>\n{}\n</transcript>", escaped)
}

fn collapse_single_newlines(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = String::with_capacity(text.len());
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            out.push_str(line);
        } else {
            let prev_empty = lines[i - 1].trim().is_empty();
            let curr_empty = line.trim().is_empty();
            if prev_empty || curr_empty {
                out.push('\n');
                out.push_str(line);
            } else {
                out.push(' ');
                out.push_str(line.trim_start());
            }
        }
    }
    out
}

fn collapse_excessive_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut newline_streak = 0usize;

    for ch in text.chars() {
        if ch == '\n' {
            newline_streak += 1;
            if newline_streak <= 2 {
                out.push(ch);
            }
        } else {
            newline_streak = 0;
            out.push(ch);
        }
    }

    out
}

fn extract_word_set(text: &str) -> HashSet<String> {
    text.split_whitespace()
        .filter_map(|raw| {
            let normalized = raw
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase();
            if normalized.len() < 2 {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn shared_word_ratio(original: &str, refined: &str) -> f64 {
    let original_words = extract_word_set(original);
    if original_words.is_empty() {
        return 1.0;
    }
    let refined_words = extract_word_set(refined);
    let shared = original_words
        .iter()
        .filter(|word| refined_words.contains(*word))
        .count();
    shared as f64 / original_words.len() as f64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptFamily {
    Latin,
    Cyrillic,
    Cjk,
    Arabic,
}

fn classify_script_family(ch: char) -> Option<ScriptFamily> {
    let code = ch as u32;
    if (0x0041..=0x024F).contains(&code) || (0x1E00..=0x1EFF).contains(&code) {
        return Some(ScriptFamily::Latin);
    }
    if (0x0400..=0x052F).contains(&code)
        || (0x2DE0..=0x2DFF).contains(&code)
        || (0xA640..=0xA69F).contains(&code)
    {
        return Some(ScriptFamily::Cyrillic);
    }
    if (0x3040..=0x30FF).contains(&code)
        || (0x31F0..=0x31FF).contains(&code)
        || (0x3400..=0x4DBF).contains(&code)
        || (0x4E00..=0x9FFF).contains(&code)
        || (0xAC00..=0xD7AF).contains(&code)
    {
        return Some(ScriptFamily::Cjk);
    }
    if (0x0600..=0x06FF).contains(&code)
        || (0x0750..=0x077F).contains(&code)
        || (0x08A0..=0x08FF).contains(&code)
        || (0xFB50..=0xFDFF).contains(&code)
        || (0xFE70..=0xFEFF).contains(&code)
    {
        return Some(ScriptFamily::Arabic);
    }
    None
}

fn script_family_name(family: ScriptFamily) -> &'static str {
    match family {
        ScriptFamily::Latin => "latin",
        ScriptFamily::Cyrillic => "cyrillic",
        ScriptFamily::Cjk => "cjk",
        ScriptFamily::Arabic => "arabic",
    }
}

fn dominant_script_family(text: &str) -> Option<ScriptFamily> {
    let mut latin = 0usize;
    let mut cyrillic = 0usize;
    let mut cjk = 0usize;
    let mut arabic = 0usize;

    for ch in text.chars() {
        match classify_script_family(ch) {
            Some(ScriptFamily::Latin) => latin += 1,
            Some(ScriptFamily::Cyrillic) => cyrillic += 1,
            Some(ScriptFamily::Cjk) => cjk += 1,
            Some(ScriptFamily::Arabic) => arabic += 1,
            None => {}
        }
    }

    let total = latin + cyrillic + cjk + arabic;
    if total < 12 {
        return None;
    }

    let counts = [
        (ScriptFamily::Latin, latin),
        (ScriptFamily::Cyrillic, cyrillic),
        (ScriptFamily::Cjk, cjk),
        (ScriptFamily::Arabic, arabic),
    ];
    let (family, max_count) = counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .unwrap_or((ScriptFamily::Latin, 0));

    if max_count * 100 >= total * 70 {
        Some(family)
    } else {
        None
    }
}

fn tokenize_language_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphabetic())
        .filter_map(|token| {
            let trimmed = token.trim();
            if trimmed.len() < 2 {
                None
            } else {
                Some(trimmed.to_lowercase())
            }
        })
        .collect()
}

fn stopword_hits(words: &[String], stopwords: &[&str]) -> usize {
    words
        .iter()
        .filter(|word| stopwords.contains(&word.as_str()))
        .count()
}

fn detect_en_de_language_hint(text: &str) -> Option<&'static str> {
    const EN_STOPWORDS: &[&str] = &[
        "the", "and", "is", "are", "to", "of", "in", "for", "with", "that", "this", "it", "on",
        "we", "you", "can", "will", "not",
    ];
    const DE_STOPWORDS: &[&str] = &[
        "der", "die", "das", "und", "ist", "sind", "nicht", "mit", "ein", "eine", "ich", "du",
        "wir", "sie", "auf", "im", "den", "zu", "fuer",
    ];

    let words = tokenize_language_words(text);
    if words.len() < 4 {
        return None;
    }

    let en_hits = stopword_hits(&words, EN_STOPWORDS);
    let de_hits = stopword_hits(&words, DE_STOPWORDS);

    if en_hits >= 2 && en_hits >= de_hits + 1 {
        Some("en")
    } else if de_hits >= 2 && de_hits >= en_hits + 1 {
        Some("de")
    } else {
        None
    }
}

fn likely_mixed_en_de(text: &str) -> bool {
    const EN_STOPWORDS: &[&str] = &[
        "the", "and", "is", "are", "to", "of", "in", "for", "with", "that", "this", "it", "on",
        "we", "you", "can", "will", "not",
    ];
    const DE_STOPWORDS: &[&str] = &[
        "der", "die", "das", "und", "ist", "sind", "nicht", "mit", "ein", "eine", "ich", "du",
        "wir", "sie", "auf", "im", "den", "zu", "fuer",
    ];

    let words = tokenize_language_words(text);
    if words.len() < 8 {
        return false;
    }
    let en_hits = stopword_hits(&words, EN_STOPWORDS);
    let de_hits = stopword_hits(&words, DE_STOPWORDS);
    en_hits >= 2 && de_hits >= 2
}

fn detect_language_drift_reason(original: &str, refined: &str) -> Option<String> {
    // Skip language-drift check for very short texts — too few words to detect
    // language reliably, and false positives are common in mixed-language environments.
    let original_words = tokenize_language_words(original);
    let refined_words = tokenize_language_words(refined);
    if original_words.len() < 8 || refined_words.len() < 8 {
        return None;
    }
    if likely_mixed_en_de(original) {
        return None;
    }

    let original_script = dominant_script_family(original);
    let refined_script = dominant_script_family(refined);
    if let (Some(orig), Some(new)) = (original_script, refined_script) {
        if orig != new && !likely_mixed_en_de(refined) {
            return Some(format!(
                "script-family mismatch ({} -> {})",
                script_family_name(orig),
                script_family_name(new)
            ));
        }
    }

    let original_lang = detect_en_de_language_hint(original);
    let refined_lang = detect_en_de_language_hint(refined);
    if let (Some(orig), Some(new)) = (original_lang, refined_lang) {
        if orig != new && !likely_mixed_en_de(refined) {
            let overlap = shared_word_ratio(original, refined);
            // Raised from 0.65 to 0.50 — EN/DE mixed sessions produce low word-overlap
            // naturally (stopwords differ) and were triggering false positives.
            if overlap < 0.50 {
                return Some(format!(
                    "stopword-language mismatch ({} -> {}, overlap={:.2})",
                    orig, new, overlap
                ));
            }
        }
    }

    None
}

fn suspicious_refinement_shape(original: &str, refined: &str) -> bool {
    let original_trimmed = original.trim();
    let refined_trimmed = refined.trim();
    if refined_trimmed.is_empty() {
        return true;
    }
    if original_trimmed.is_empty() {
        return false;
    }

    let original_chars = original_trimmed
        .chars()
        .filter(|c| !c.is_whitespace())
        .count();
    let refined_chars = refined_trimmed
        .chars()
        .filter(|c| !c.is_whitespace())
        .count();

    let original_blank_lines = original_trimmed
        .lines()
        .filter(|line| line.trim().is_empty())
        .count();
    let refined_blank_lines = refined_trimmed
        .lines()
        .filter(|line| line.trim().is_empty())
        .count();

    let severe_shrink = original_chars >= 80 && refined_chars * 100 < original_chars * 45;
    let blank_line_spike =
        refined_blank_lines >= original_blank_lines + 4 && refined_blank_lines >= 6;
    let overlap_ratio = shared_word_ratio(original_trimmed, refined_trimmed);
    let low_overlap = original_trimmed.split_whitespace().count() >= 20 && overlap_ratio < 0.40;

    (severe_shrink && low_overlap) || (blank_line_spike && severe_shrink)
}

/// Strip `<think>…</think>` blocks that reasoning models (Qwen3, DeepSeek-R1,
/// etc.) inject before the actual answer.  Works for both single-line and
/// multi-line think blocks.  Returns input unchanged if no `<think>` tag found.
pub(super) fn strip_thinking_tags(text: &str) -> String {
    if !text.contains("<think>") {
        return text.to_string();
    }
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?s)<think>.*?</think>\s*").unwrap());
    RE.replace_all(text, "").trim().to_string()
}

const TRANSCRIPT_OPEN_TAG: &str = "<transcript>";
const TRANSCRIPT_CLOSE_TAG: &str = "</transcript>";

fn strip_suffix_ignore_ascii_case<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let start = text.len().checked_sub(suffix.len())?;
    let tail = text.get(start..)?;
    tail.eq_ignore_ascii_case(suffix).then(|| &text[..start])
}

/// Start of a closing tag cut off at the very end of `text` (`</tra`, `</transcript`).
fn truncated_close_tag_start(text: &str) -> Option<usize> {
    let idx = text.rfind("</")?;
    let tail = &text[idx..];
    let is_cut_tag = tail.len() >= 3
        && tail.len() < TRANSCRIPT_CLOSE_TAG.len()
        && TRANSCRIPT_CLOSE_TAG[..tail.len()].eq_ignore_ascii_case(tail);
    is_cut_tag.then_some(idx)
}

/// The rest of `text` after an opening tag at its start, whole (`<transcript>`) or cut off
/// (`<transcr>`, `<transcript` followed by whitespace).
fn strip_open_tag_at_start(text: &str) -> Option<&str> {
    let after_bracket = text.strip_prefix('<')?;
    let word = &TRANSCRIPT_OPEN_TAG[1..TRANSCRIPT_OPEN_TAG.len() - 1];
    let fragment_len = after_bracket
        .bytes()
        .take_while(|b| b.is_ascii_alphabetic())
        .count();
    let fragment = &after_bracket[..fragment_len];
    if fragment.len() < 3
        || fragment.len() > word.len()
        || !word[..fragment.len()].eq_ignore_ascii_case(fragment)
    {
        return None;
    }
    let rest = &after_bracket[fragment_len..];
    match rest.chars().next() {
        Some('>') => Some(&rest[1..]),
        Some(c) if c.is_whitespace() => Some(rest),
        None => Some(rest),
        Some(_) => None,
    }
}

/// `<bitte schick …`: the first word of the text glued to the `<` of a cut-off opening tag.
fn starts_with_mangled_open_bracket(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next() == Some('<')
        && chars.next().is_some_and(char::is_alphabetic)
        && !text
            .split_whitespace()
            .next()
            .is_some_and(|first_token| first_token.contains('>'))
}

/// Removes the `<transcript>` wrapper a model echoed back from the prompt: a closing tag at the
/// very end (whole or cut off), an opening tag at the very start (whole or cut off) and, only
/// together with a closing tag, the lone `<` of an opening tag cut off to `<bitte …`.
/// Text without such tags comes back unchanged; tags or the word "transcript" inside the text
/// are never touched.
pub(super) fn strip_transcript_wrapper_tags(text: &str) -> String {
    let mut body = text.trim();
    let mut stripped = false;
    let mut had_close_tag = false;

    if let Some(rest) = strip_suffix_ignore_ascii_case(body, TRANSCRIPT_CLOSE_TAG) {
        body = rest.trim_end();
        stripped = true;
        had_close_tag = true;
    } else if let Some(idx) = truncated_close_tag_start(body) {
        body = body[..idx].trim_end();
        stripped = true;
        had_close_tag = true;
    }

    if let Some(rest) = strip_open_tag_at_start(body) {
        body = rest.trim_start();
        stripped = true;
    } else if had_close_tag && starts_with_mangled_open_bracket(body) {
        body = body[1..].trim_start();
        stripped = true;
    }

    if stripped {
        body.to_string()
    } else {
        text.to_string()
    }
}

fn sanitize_ollama_refinement_output(
    original: &str,
    refined: &str,
    options: &RefinementOptions,
) -> String {
    // Some models (lfm3, bonsai) echo the <transcript> wrapper of the prompt back.
    let normalized = strip_transcript_wrapper_tags(refined)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', " "); // Tabs → Space (Modell soll keine Tabs einfügen)
    let profile = normalize_prompt_profile(&options.prompt_profile);
    let collapsed = if matches!(profile, "wording" | "custom") {
        collapse_single_newlines(&collapse_excessive_blank_lines(normalized.trim()))
    } else {
        collapse_excessive_blank_lines(normalized.trim())
    };
    if suspicious_refinement_shape(original, &collapsed) {
        warn!(
            "Ignoring suspicious Ollama refinement output and keeping original transcript (orig_len={}, refined_len={})",
            original.len(),
            collapsed.len()
        );
        return original.to_string();
    }
    if options.enforce_language_guard {
        if let Some(reason) = detect_language_drift_reason(original, &collapsed) {
            warn!(
                "Discarding refinement due to language drift guard (reason={}, orig_len={}, refined_len={})",
                reason,
                original.len(),
                collapsed.len()
            );
            return original.to_string();
        }
    }
    if collapsed.is_empty() {
        return original.to_string();
    }
    collapsed
}

fn map_ollama_http_error(code: u16, detail: &str) -> AIError {
    if detail.is_empty() {
        return AIError::NetworkError(format!("Ollama returned HTTP {}", code));
    }
    AIError::NetworkError(format!("Ollama returned HTTP {}: {}", code, detail))
}

#[cfg(test)]
fn is_ollama_runner_command_line(process_name: &str, command_line: &str) -> bool {
    let name = process_name.trim().to_ascii_lowercase();
    if name != "ollama.exe" && name != "ollama" {
        return false;
    }
    command_line.split_whitespace().any(|part| {
        let token = part
            .trim_matches(|ch| matches!(ch, '"' | '\''))
            .to_ascii_lowercase();
        token == "runner"
            || token == "runner.exe"
            || token.ends_with("\\runner")
            || token.ends_with("\\runner.exe")
            || token.ends_with("/runner")
            || token.ends_with("/runner.exe")
    })
}

#[derive(Clone)]
pub struct OllamaProvider {
    endpoint: String,
}

impl OllamaProvider {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    pub fn id(&self) -> &'static str {
        "ollama"
    }

    pub fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
    ) -> Result<RefinementResult, AIError> {
        let start = Instant::now();
        let diagnostics_enabled = crate::state::diagnostic_logging_enabled();
        let model = model.trim();
        if model.is_empty() {
            return Err(AIError::NetworkError(
                "No Ollama model selected. Set an active model in AI Refinement > Models."
                    .to_string(),
            ));
        }

        let system_prompt = options
            .custom_prompt
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                let lang = options.language.as_deref().unwrap_or("en");
                default_prompt_for_language(lang)
            });

        // Limit input size for local inference to prevent multi-minute refinement times
        let effective_text = truncate_for_local_inference(text, 2000);
        let text = effective_text.as_str();

        // "think": false disables extended chain-of-thought mode on reasoning models
        // (e.g. qwen3, deepseek-r1). Without this, thinking models generate internal
        // reasoning tokens indefinitely before producing any output, causing apparent hangs.
        let (ollama_options, tier_result) =
            build_ollama_options_payload(options, text, system_prompt);
        // Only report growth the caller doesn't already know about — avoids a
        // redundant settings write when the persisted tiers already cover this.
        let tier_update = if tier_result.num_predict_tier_idx != options.num_predict_tier_idx
            || tier_result.num_ctx_tier_idx != options.num_ctx_tier_idx
        {
            Some(tier_result)
        } else {
            None
        };
        // Keep the model loaded indefinitely between requests by default so the
        // next hotkey press never has to wait for a cold reload. Operators can
        // override with TRISPR_OLLAMA_KEEP_ALIVE (e.g. "30m" for a shorter window).
        // Trispr Flow's own idle-release (4 h) is the primary unload mechanism.
        let keep_alive_raw = std::env::var("TRISPR_OLLAMA_KEEP_ALIVE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "-1".to_string());
        // OLLAMA parses keep_alive as a Go duration string OR as a JSON number.
        // "-1" as a string is rejected ("missing unit in duration"); -1 as an
        // integer is accepted and means "keep forever". Parse here so callers can
        // use env-var overrides with either integers ("-1") or durations ("30m").
        let keep_alive: serde_json::Value = if let Ok(n) = keep_alive_raw.trim().parse::<i64>() {
            serde_json::json!(n)
        } else {
            serde_json::json!(keep_alive_raw)
        };

        let chat_body = serde_json::json!({
            "model": model,
            "stream": false,
            "think": false,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": wrap_transcript_in_xml(text) }
            ],
            "options": ollama_options.clone(),
            "keep_alive": keep_alive.clone()
        });

        let generate_body = serde_json::json!({
            "model": model,
            "prompt": format!("{}\n\n{}", system_prompt, wrap_transcript_in_xml(text)),
            "stream": false,
            "think": false,
            "options": ollama_options,
            "keep_alive": keep_alive
        });

        // Per-request agent with timeout scaled to input + prompt complexity.
        // Short inputs: 60 s; long/multilingual prompts: up to 120 s.
        let timeout_secs = refinement_timeout_secs(text, system_prompt);
        if diagnostics_enabled {
            info!(
                "[ollama.refine] start endpoint={} model={} input_bytes={} prompt_bytes={} profile={} low_latency={} max_tokens={} temperature={} keep_alive={} timeout_s={}",
                self.endpoint,
                model,
                text.len(),
                system_prompt.len(),
                options.prompt_profile,
                options.low_latency_mode,
                options.max_tokens,
                options.temperature,
                keep_alive,
                timeout_secs
            );
        }
        let agent = refinement_agent(text, system_prompt);

        let mut last_transport_error: Option<AIError> = None;

        for candidate in ollama_endpoint_candidates(&self.endpoint) {
            let chat_url = format!("{}/api/chat", candidate);
            let chat_response = match agent
                .post(&chat_url)
                .set("Content-Type", "application/json")
                .send_json(chat_body.clone())
            {
                Ok(resp) => Some(resp),
                Err(ureq::Error::Status(code, response)) => {
                    let detail = extract_ollama_error_message(response);
                    if is_ollama_model_not_found(&detail) {
                        return Err(AIError::NetworkError(format!(
                            "Model '{}' not found in Ollama. Pull it first with: ollama pull {}",
                            model, model
                        )));
                    }
                    if !is_missing_chat_route(code, &detail) {
                        return Err(map_ollama_http_error(code, &detail));
                    }
                    None
                }
                Err(ureq::Error::Transport(t)) => {
                    let msg = t.to_string().to_lowercase();
                    if msg.contains("timed out") || msg.contains("timeout") {
                        warn!(
                            "[ollama.refine] timeout route=chat candidate={} model={} elapsed_ms={} — no endpoint retry; scheduling local abort",
                            candidate,
                            model,
                            start.elapsed().as_millis()
                        );
                        return Err(AIError::Timeout);
                    }
                    last_transport_error = Some(AIError::OllamaNotRunning);
                    continue;
                }
            };

            if let Some(resp) = chat_response {
                let json: serde_json::Value = resp.into_json().map_err(|e| {
                    AIError::NetworkError(format!("Failed to parse Ollama response: {}", e))
                })?;
                let refined_text = parse_ollama_refined_text(&json).ok_or_else(|| {
                    AIError::NetworkError(
                        "Unexpected Ollama response format: missing message.content/response"
                            .to_string(),
                    )
                })?;
                let refined_text = strip_thinking_tags(&refined_text);
                let refined_text = sanitize_ollama_refinement_output(text, &refined_text, options);
                let usage = parse_ollama_usage(&json);
                let elapsed_ms = start.elapsed().as_millis() as u64;
                if diagnostics_enabled {
                    info!(
                        "[ollama.refine] success route=chat candidate={} elapsed_ms={} input_tokens={} output_tokens={} load_ms={:?} eval_ms={:?} output_bytes={}",
                        candidate,
                        elapsed_ms,
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.load_ms,
                        usage.eval_ms,
                        refined_text.len()
                    );
                }
                return Ok(RefinementResult {
                    text: refined_text,
                    usage: TokenUsage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        total_cost_usd: 0.0,
                    },
                    provider: self.id().to_string(),
                    model: model.to_string(),
                    execution_time_ms: elapsed_ms,
                    ollama_load_ms: usage.load_ms,
                    ollama_prompt_eval_ms: usage.prompt_eval_ms,
                    ollama_eval_ms: usage.eval_ms,
                    tier_update,
                });
            }

            let generate_url = format!("{}/api/generate", candidate);
            let generate_response = match agent
                .post(&generate_url)
                .set("Content-Type", "application/json")
                .send_json(generate_body.clone())
            {
                Ok(resp) => resp,
                Err(ureq::Error::Status(code, response)) => {
                    let detail = extract_ollama_error_message(response);
                    if is_ollama_model_not_found(&detail) {
                        return Err(AIError::NetworkError(format!(
                            "Model '{}' not found in Ollama. Pull it first with: ollama pull {}",
                            model, model
                        )));
                    }
                    if code == 404 {
                        warn!(
                            "[ollama.refine] chat route missing on candidate={} model={} — falling back to /api/generate",
                            candidate,
                            model
                        );
                        return Err(AIError::NetworkError(
                            "Ollama route not found: neither /api/chat nor /api/generate is available."
                                .to_string(),
                        ));
                    }
                    return Err(map_ollama_http_error(code, &detail));
                }
                Err(ureq::Error::Transport(t)) => {
                    let msg = t.to_string().to_lowercase();
                    if msg.contains("timed out") || msg.contains("timeout") {
                        warn!(
                            "[ollama.refine] timeout route=generate candidate={} model={} elapsed_ms={} — no endpoint retry; scheduling local abort",
                            candidate,
                            model,
                            start.elapsed().as_millis()
                        );
                        return Err(AIError::Timeout);
                    }
                    last_transport_error = Some(AIError::OllamaNotRunning);
                    continue;
                }
            };

            let json: serde_json::Value = generate_response.into_json().map_err(|e| {
                AIError::NetworkError(format!("Failed to parse Ollama response: {}", e))
            })?;
            let refined_text = parse_ollama_refined_text(&json).ok_or_else(|| {
                AIError::NetworkError(
                    "Unexpected Ollama response format: missing message.content/response"
                        .to_string(),
                )
            })?;
            let refined_text = strip_thinking_tags(&refined_text);
            let refined_text = sanitize_ollama_refinement_output(text, &refined_text, options);
            let usage = parse_ollama_usage(&json);
            let elapsed_ms = start.elapsed().as_millis() as u64;
            if diagnostics_enabled {
                info!(
                    "[ollama.refine] success route=generate candidate={} elapsed_ms={} input_tokens={} output_tokens={} load_ms={:?} eval_ms={:?} output_bytes={}",
                    candidate,
                    elapsed_ms,
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.load_ms,
                    usage.eval_ms,
                    refined_text.len()
                );
            }

            return Ok(RefinementResult {
                text: refined_text,
                usage: TokenUsage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    total_cost_usd: 0.0,
                },
                provider: self.id().to_string(),
                model: model.to_string(),
                execution_time_ms: elapsed_ms,
                ollama_load_ms: usage.load_ms,
                ollama_prompt_eval_ms: usage.prompt_eval_ms,
                ollama_eval_ms: usage.eval_ms,
                tier_update,
            });
        }

        let error = last_transport_error.unwrap_or(AIError::OllamaNotRunning);
        error!(
            "[ollama.refine] failed endpoint={} model={} elapsed_ms={} error={}",
            self.endpoint,
            model,
            start.elapsed().as_millis(),
            error
        );
        Err(error)
    }
}

/// Truncate input text for local inference to prevent extremely long refinement
/// times on CPU-bound models.
fn truncate_for_local_inference(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        return text.to_string();
    }
    let truncated = words[..max_words].join(" ");
    // Try to cut at the last sentence boundary for clean output
    if let Some(pos) = truncated.rfind(|c| c == '.' || c == '!' || c == '?') {
        format!("{} [truncated]", &truncated[..=pos])
    } else {
        format!("{} [truncated]", truncated)
    }
}

// ============================================================================
// Ollama Pull API — Structs and streaming handler
// ============================================================================

use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
pub struct OllamaPullProgress {
    pub model: String,
    pub status: String,
    pub digest: Option<String>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaPullComplete {
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaPullError {
    pub model: String,
    pub error: String,
}

const OLLAMA_REGISTRY_MODEL_BASE: &str = "https://ollama.com/library";

fn map_ollama_registry_precheck_error(
    model: &str,
    status_code: Option<u16>,
    transport_error: Option<&str>,
) -> String {
    if let Some(code) = status_code {
        if code == 404 {
            return format!(
                "Model tag '{}' is not available in the Ollama registry. Check name/tag and retry.",
                model
            );
        }
        if code >= 500 {
            return format!(
                "Ollama registry is currently unavailable (HTTP {}). Please retry later.",
                code
            );
        }
        return format!(
            "Ollama registry precheck failed for '{}' (HTTP {}).",
            model, code
        );
    }

    let transport = transport_error.unwrap_or_default().trim();
    if !transport.is_empty() {
        let lower = transport.to_ascii_lowercase();
        if lower.contains("timed out") || lower.contains("timeout") {
            return format!(
                "Ollama registry precheck timed out for '{}'. Check network connectivity and retry.",
                model
            );
        }
        return format!(
            "Ollama registry precheck could not reach the endpoint for '{}': {}",
            model, transport
        );
    }

    format!("Ollama registry precheck failed for '{}'.", model)
}

pub fn precheck_ollama_registry_model_tag(model: &str) -> Result<(), String> {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("Model name cannot be empty".to_string());
    }

    let url = format!("{}/{}", OLLAMA_REGISTRY_MODEL_BASE, normalized);
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(4))
        .timeout_read(Duration::from_secs(6))
        .build();

    match agent
        .get(&url)
        .set("User-Agent", "TrisprFlow/OllamaRegistryPrecheck")
        .call()
    {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, _)) => Err(map_ollama_registry_precheck_error(
            &normalized,
            Some(code),
            None,
        )),
        Err(ureq::Error::Transport(transport)) => {
            let message = transport.to_string();
            Err(map_ollama_registry_precheck_error(
                &normalized,
                None,
                Some(&message),
            ))
        }
    }
}

/// Validate an Ollama model name against known safe characters.
/// Allowed: alphanumeric, ':' (for tags), '.' (for versions), '-', '_'
pub fn validate_ollama_model_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Model name cannot be empty".to_string());
    }
    if !name.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == ':' || c == '.' || c == '-' || c == '_' || c == '/'
    }) {
        return Err("Invalid characters in model name".to_string());
    }
    if name.len() > 200 {
        return Err("Model name too long (max 200 chars)".to_string());
    }
    Ok(())
}

fn format_ollama_pull_snapshot(
    status: &str,
    digest: Option<&str>,
    completed: Option<u64>,
    total: Option<u64>,
) -> String {
    let mut parts = vec![format!("status={}", status)];
    if let Some(digest) = digest.filter(|value| !value.trim().is_empty()) {
        parts.push(format!("digest={}", digest));
    }
    if let Some(completed) = completed {
        parts.push(format!("completed={}B", completed));
    }
    if let Some(total) = total {
        parts.push(format!("total={}B", total));
    }
    if let (Some(completed), Some(total)) = (completed, total) {
        if total > 0 {
            let pct = ((completed.saturating_mul(100)) / total).min(100);
            parts.push(format!("pct={}%", pct));
        }
    }
    parts.join(" ")
}

/// One progress line of `/api/pull`.
pub(super) struct PullLine {
    pub status: String,
    pub digest: Option<String>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
    pub error: Option<String>,
}

/// How a pull stream ended.
#[derive(Debug, PartialEq)]
pub(super) enum PullStreamEnd {
    /// The `success` status arrived.
    Success { lines: u64 },
    /// Ollama sent an error line.
    Failed { error: String, lines: u64 },
    /// The connection broke (reset, server killed, cut off) before the pull finished.
    Broken { error: String, lines: u64 },
    /// The stream ended cleanly without `success`.
    EndedEarly { lines: u64 },
}

/// Reads the newline-delimited JSON of `/api/pull` to its end. `on_line(line, count)` sees every
/// line (logging), `on_progress(line)` every line that is not the final success (progress
/// events). A read error ends the pull: a broken connection keeps failing on every further read,
/// so skipping it would spin forever at full speed (measured: 100,000 errors in 0.13 s) with the
/// pull marked "in progress" until the app restarts.
pub(super) fn consume_pull_stream<R: std::io::BufRead>(
    reader: R,
    on_line: &mut dyn FnMut(&PullLine, u64),
    on_progress: &mut dyn FnMut(&PullLine),
) -> PullStreamEnd {
    let mut lines = 0u64;
    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(error) => {
                return PullStreamEnd::Broken {
                    error: error.to_string(),
                    lines,
                }
            }
        };
        let json: serde_json::Value = match serde_json::from_str(&line) {
            Ok(j) => j,
            Err(_) => continue,
        };
        lines += 1;
        let pull_line = PullLine {
            status: json["status"].as_str().unwrap_or("").to_string(),
            digest: json["digest"].as_str().map(|s| s.to_string()),
            total: json["total"].as_u64(),
            completed: json["completed"].as_u64(),
            error: json["error"].as_str().map(|s| s.to_string()),
        };
        on_line(&pull_line, lines);
        if pull_line.status == "success" {
            return PullStreamEnd::Success { lines };
        }
        on_progress(&pull_line);
        if let Some(error) = pull_line.error {
            return PullStreamEnd::Failed { error, lines };
        }
    }
    PullStreamEnd::EndedEarly { lines }
}

/// Pull an Ollama model and stream progress via Tauri events.
/// Spawned in a background thread.
///
/// Events emitted:
/// - "ollama:pull-progress": OllamaPullProgress (every ~250ms during download)
/// - "ollama:pull-complete": OllamaPullComplete (on success)
/// - "ollama:pull-error": OllamaPullError (on failure)
pub fn pull_ollama_model_inner(app: AppHandle, model: String, endpoint: String) {
    let diagnostics_enabled = crate::state::diagnostic_logging_enabled();
    let body = serde_json::json!({ "model": model, "stream": true });
    let pull_started = Instant::now();

    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(5))
        // Per read, not per pull: Ollama reports progress every few seconds, and even the
        // checksum pass over a big blob stays far below this. A connection that went silent
        // ends the pull after 10 minutes instead of holding the "pull in progress" entry for
        // an hour.
        .timeout_read(Duration::from_secs(600))
        .build();

    let mut last_connect_error: Option<String> = None;
    let mut attempted_candidates = 0usize;

    for candidate in ollama_endpoint_candidates(&endpoint) {
        attempted_candidates += 1;
        let url = format!("{}/api/pull", candidate);
        if diagnostics_enabled {
            info!(
                "[ollama.pull] request start model={} candidate={} url={} elapsed_ms={}",
                model,
                candidate,
                url,
                pull_started.elapsed().as_millis()
            );
        }
        let response = match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(body.clone())
        {
            Ok(r) => r,
            Err(ureq::Error::Transport(t)) => {
                last_connect_error = Some(t.to_string());
                warn!(
                    "[ollama.pull] request transport error model={} candidate={} elapsed_ms={} error={}",
                    model,
                    candidate,
                    pull_started.elapsed().as_millis(),
                    t
                );
                continue;
            }
            Err(e) => {
                error!(
                    "[ollama.pull] request failed model={} candidate={} elapsed_ms={} error={}",
                    model,
                    candidate,
                    pull_started.elapsed().as_millis(),
                    e
                );
                let _ = app.emit(
                    "ollama:pull-error",
                    OllamaPullError {
                        model: model.clone(),
                        error: format!("Connection failed: {}", e),
                    },
                );
                return;
            }
        };

        if diagnostics_enabled {
            info!(
                "[ollama.pull] stream open model={} candidate={} elapsed_ms={}",
                model,
                candidate,
                pull_started.elapsed().as_millis()
            );
        }
        let reader = response.into_reader();
        let buffered = std::io::BufReader::new(reader);
        let mut last_emit = Instant::now();
        let mut last_status: Option<String> = None;
        let mut last_digest: Option<String> = None;
        let mut last_completed: Option<u64> = None;
        let mut last_total: Option<u64> = None;

        let end = consume_pull_stream(
            buffered,
            &mut |line, line_count| {
                let should_log = line_count <= 3
                    || last_status.as_deref() != Some(line.status.as_str())
                    || last_digest.as_deref() != line.digest.as_deref()
                    || last_completed != line.completed
                    || last_total != line.total
                    || line.error.is_some();
                if should_log && diagnostics_enabled {
                    info!(
                        "[ollama.pull] event model={} candidate={} line={} elapsed_ms={} {}",
                        model,
                        candidate,
                        line_count,
                        pull_started.elapsed().as_millis(),
                        format_ollama_pull_snapshot(
                            &line.status,
                            line.digest.as_deref(),
                            line.completed,
                            line.total,
                        ),
                    );
                }
                last_status = Some(line.status.clone());
                last_digest = line.digest.clone();
                last_completed = line.completed;
                last_total = line.total;
            },
            &mut |line| {
                // Emit progress every 250ms
                if last_emit.elapsed() >= Duration::from_millis(250) {
                    let _ = app.emit(
                        "ollama:pull-progress",
                        OllamaPullProgress {
                            model: model.clone(),
                            status: line.status.clone(),
                            digest: line.digest.clone(),
                            total: line.total,
                            completed: line.completed,
                        },
                    );
                    last_emit = Instant::now();
                }
            },
        );

        match end {
            PullStreamEnd::Success { lines } => {
                if diagnostics_enabled {
                    info!(
                        "[ollama.pull] success model={} candidate={} elapsed_ms={} lines={} attempts={}",
                        model,
                        candidate,
                        pull_started.elapsed().as_millis(),
                        lines,
                        attempted_candidates
                    );
                }
                let _ = app.emit(
                    "ollama:pull-complete",
                    OllamaPullComplete {
                        model: model.clone(),
                    },
                );
            }
            PullStreamEnd::Failed { error, lines } => {
                warn!(
                    "[ollama.pull] error model={} candidate={} elapsed_ms={} lines={} error={}",
                    model,
                    candidate,
                    pull_started.elapsed().as_millis(),
                    lines,
                    error
                );
                let _ = app.emit(
                    "ollama:pull-error",
                    OllamaPullError {
                        model: model.clone(),
                        error,
                    },
                );
            }
            PullStreamEnd::Broken { error, lines } => {
                warn!(
                    "[ollama.pull] connection lost model={} candidate={} elapsed_ms={} lines={} error={}",
                    model,
                    candidate,
                    pull_started.elapsed().as_millis(),
                    lines,
                    error
                );
                let _ = app.emit(
                    "ollama:pull-error",
                    OllamaPullError {
                        model: model.clone(),
                        error: format!(
                            "The connection to Ollama was lost during the download ({}). Partial downloads are kept: start the pull again to resume.",
                            error
                        ),
                    },
                );
            }
            PullStreamEnd::EndedEarly { lines } => {
                // Stream ended without "success" status.
                warn!(
                    "[ollama.pull] stream ended unexpectedly model={} candidate={} elapsed_ms={} lines={}",
                    model,
                    candidate,
                    pull_started.elapsed().as_millis(),
                    lines
                );
                let _ = app.emit(
                    "ollama:pull-error",
                    OllamaPullError {
                        model: model.clone(),
                        error: "Stream ended unexpectedly without success status".to_string(),
                    },
                );
            }
        }
        return;
    }

    warn!(
        "[ollama.pull] all candidates failed model={} endpoint={} attempts={} elapsed_ms={} last_connect_error={}",
        model,
        endpoint,
        attempted_candidates,
        pull_started.elapsed().as_millis(),
        last_connect_error.as_deref().unwrap_or("none")
    );
    let _ = app.emit(
        "ollama:pull-error",
        OllamaPullError {
            model: model.clone(),
            error: format!(
                "Connection failed: {}",
                last_connect_error.unwrap_or_else(|| "unable to reach Ollama endpoint".to_string())
            ),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_options(enforce_language_guard: bool) -> RefinementOptions {
        test_options_for_profile(enforce_language_guard, "wording")
    }

    fn test_options_for_profile(
        enforce_language_guard: bool,
        prompt_profile: &str,
    ) -> RefinementOptions {
        RefinementOptions {
            temperature: 0.3,
            max_tokens: 512,
            low_latency_mode: false,
            language: Some("en".to_string()),
            custom_prompt: None,
            enforce_language_guard,
            prompt_profile: prompt_profile.to_string(),
            num_predict_tier_idx: 0,
            num_ctx_tier_idx: 0,
        }
    }

    #[test]
    fn endpoint_candidates_include_ipv4_fallback_for_localhost() {
        let candidates = ollama_endpoint_candidates("http://localhost:11434");
        assert_eq!(candidates[0], "http://127.0.0.1:11434");
        assert!(candidates.iter().any(|c| c == "http://127.0.0.1:11434"));
        assert!(candidates.iter().any(|c| c == "http://localhost:11434"));
    }

    #[test]
    fn endpoint_candidates_include_localhost_fallback_for_ipv4() {
        let candidates = ollama_endpoint_candidates("http://127.0.0.1:11434");
        assert_eq!(candidates[0], "http://127.0.0.1:11434");
        assert!(candidates.iter().any(|c| c == "http://localhost:11434"));
    }

    #[test]
    fn ollama_ps_model_with_zero_vram_is_loaded() {
        let json = serde_json::json!({
            "models": [
                {
                    "name": "qwen3.5:4b",
                    "model": "qwen3.5:4b",
                    "size_vram": 0,
                    "context_length": 2048
                }
            ]
        });

        let status = parse_ollama_model_load_status(&json, "qwen3.5:4b").unwrap();

        assert!(status.loaded);
        assert_eq!(status.size_vram, 0);
    }

    #[test]
    fn ollama_ps_model_with_vram_is_loaded_with_telemetry() {
        let json = serde_json::json!({
            "models": [
                {
                    "name": "qwen3.5:4b",
                    "model": "qwen3.5:4b",
                    "size_vram": 1_234_567_u64
                }
            ]
        });

        let status = parse_ollama_model_load_status(&json, "QWEN3.5:4B").unwrap();

        assert!(status.loaded);
        assert_eq!(status.size_vram, 1_234_567);
    }

    #[test]
    fn ollama_ps_missing_active_model_is_not_loaded() {
        let json = serde_json::json!({
            "models": [
                {
                    "name": "other:latest",
                    "model": "other:latest",
                    "size_vram": 9_999_u64
                }
            ]
        });

        let status = parse_ollama_model_load_status(&json, "qwen3.5:4b").unwrap();

        assert!(!status.loaded);
        assert_eq!(status.size_vram, 0);
    }

    #[test]
    fn ollama_ps_empty_models_is_known_cold_state() {
        let json = serde_json::json!({ "models": [] });

        let status = parse_ollama_model_load_status(&json, "qwen3.5:4b").unwrap();

        assert!(!status.loaded);
        assert_eq!(status.size_vram, 0);
    }

    // --- Ollama: unreachable endpoint returns empty model list (no panic) ---
    #[test]
    fn list_ollama_models_returns_empty_on_bad_endpoint() {
        let models = list_ollama_models("http://127.0.0.1:19999");
        assert!(
            models.is_empty(),
            "expected empty list for unreachable endpoint"
        );
    }

    // --- Ollama: unreachable endpoint returns OllamaNotRunning (no panic) ---
    #[test]
    fn ping_ollama_returns_error_on_bad_endpoint() {
        let result = ping_ollama("http://127.0.0.1:19999");
        assert!(
            matches!(result, Err(AIError::OllamaNotRunning)),
            "expected OllamaNotRunning for unreachable endpoint, got: {:?}",
            result
        );
    }

    #[test]
    fn registry_precheck_maps_tag_missing() {
        let message = map_ollama_registry_precheck_error("qwen3:999b", Some(404), None);
        assert!(message.contains("not available"));
        assert!(message.contains("qwen3:999b"));
    }

    #[test]
    fn registry_precheck_maps_endpoint_down() {
        let message = map_ollama_registry_precheck_error("qwen3:4b", Some(503), None);
        assert!(message.contains("currently unavailable"));
        assert!(message.contains("503"));
    }

    #[test]
    fn registry_precheck_maps_timeout_transport() {
        let message = map_ollama_registry_precheck_error(
            "qwen3:4b",
            None,
            Some("operation timed out while connecting"),
        );
        assert!(message.contains("timed out"));
    }

    // --- Prompt selection by language ---
    #[test]
    fn default_prompt_uses_english_for_unknown_language() {
        assert_eq!(default_prompt_for_language("en"), OLLAMA_PROMPT_EN);
        assert_eq!(default_prompt_for_language("fr"), OLLAMA_PROMPT_EN);
    }

    #[test]
    fn default_prompt_uses_auto_for_empty_or_auto_language() {
        assert_eq!(default_prompt_for_language(""), OLLAMA_PROMPT_AUTO);
        assert_eq!(default_prompt_for_language("auto"), OLLAMA_PROMPT_AUTO);
        assert!(OLLAMA_PROMPT_AUTO.contains("Detect the language"));
        assert!(OLLAMA_PROMPT_AUTO.contains("never translate"));
    }

    #[test]
    fn default_prompt_uses_german_for_de() {
        assert_eq!(default_prompt_for_language("de"), OLLAMA_PROMPT_DE);
        assert_eq!(default_prompt_for_language("german"), OLLAMA_PROMPT_DE);
        assert_eq!(default_prompt_for_language("DE"), OLLAMA_PROMPT_DE);
    }

    #[test]
    fn custom_profile_prompt_is_not_modified_by_language_lock() {
        for preserve_source_language in [false, true] {
            let prompt = prompt_for_profile(
                "custom",
                "en",
                Some("Custom prompt stays unchanged."),
                preserve_source_language,
            );
            assert_eq!(prompt.as_deref(), Some("Custom prompt stays unchanged."));
        }
    }

    #[test]
    fn built_in_prompt_language_lock_tracks_preserve_flag() {
        let locked = prompt_for_profile("wording", "de", None, true).unwrap_or_default();
        let unlocked = prompt_for_profile("wording", "de", None, false).unwrap_or_default();

        assert!(locked.starts_with("Behalte die Ausgabe in derselben Sprache"));
        assert!(locked.ends_with(OLLAMA_PROMPT_DE));
        assert_eq!(unlocked, OLLAMA_PROMPT_DE);
    }

    #[test]
    fn empty_custom_prompt_falls_back_to_localized_default_with_optional_lock() {
        let locked = prompt_for_profile("custom", "de", Some("  \n\t "), true).unwrap();
        let unlocked = prompt_for_profile("custom", "de", Some("  \n\t "), false).unwrap();

        assert!(locked.starts_with("Behalte die Ausgabe in derselben Sprache"));
        assert!(locked.ends_with(OLLAMA_PROMPT_DE));
        assert_eq!(unlocked, OLLAMA_PROMPT_DE);
    }

    #[test]
    fn llm_prompt_profile_skips_language_lock_with_either_preserve_setting() {
        let locked = prompt_for_profile("llm_prompt", "de", None, true).unwrap();
        let unlocked = prompt_for_profile("llm_prompt", "de", None, false).unwrap();

        assert_eq!(locked, unlocked);
        assert_eq!(locked, super::OLLAMA_PROMPT_LLM_PROMPT_DE);
        assert!(!locked.contains("Behalte die Ausgabe in derselben Sprache"));
    }

    #[test]
    fn auto_language_prompt_has_language_preservation() {
        // When language="auto", the auto prompt is used which leads with language detection.
        // The language lock prefix is prepended before the base prompt body.
        let prompt = prompt_for_profile("wording", "auto", None, true).unwrap_or_default();
        // The auto base prompt already embeds a strong language guard at the start.
        assert!(
            prompt.contains("Detect the language") || prompt.contains("Detect the input language")
        );
        assert!(prompt.contains("never translate") || prompt.contains("keep it unchanged"));
    }

    // --- rough_token_estimate is sensible ---
    #[test]
    fn token_estimate_is_positive_for_non_empty_text() {
        let estimate = rough_token_estimate("hello world foo bar baz");
        assert!(estimate >= 5, "should be at least as many as words");
    }

    #[test]
    fn token_estimate_is_at_least_one_for_empty_text() {
        assert!(rough_token_estimate("") >= 1);
    }

    #[test]
    fn local_wording_num_predict_uses_input_not_prompt_size() {
        let prompt = "long system prompt ".repeat(80);
        let (predicted, _tier_idx) =
            resolve_num_predict("kurzer test text", "wording", 512, false, 0);
        let inflated_prompt_tokens = rough_token_estimate(&prompt);
        assert!(inflated_prompt_tokens > 100);
        assert!(
            predicted <= 80,
            "short transcript should not receive large generation budget, got {}",
            predicted
        );
    }

    #[test]
    fn summary_num_predict_grows_past_old_384_hardcap_for_long_input() {
        // ~600 words ~= 810 rough tokens; the old fixed .clamp(96, 384) truncated
        // this regardless of the user's configured max_tokens. With tiering and
        // a high enough configured_max, the summary should get real headroom.
        let long_transcript = "word ".repeat(600);
        let (predicted, tier_idx) =
            resolve_num_predict(&long_transcript, "summary", 8192, false, 0);
        assert!(
            predicted > 384,
            "long summary input should grow past the old hardcoded ceiling, got {}",
            predicted
        );
        assert!(
            tier_idx > 1,
            "tier should have grown beyond the starting tier"
        );
    }

    #[test]
    fn num_predict_tier_growth_is_monotonic_and_one_step_ahead() {
        let long_transcript = "word ".repeat(600);
        let (_predicted, grown_idx) =
            resolve_num_predict(&long_transcript, "summary", 8192, false, 0);
        // A subsequent short input must not shrink the persisted tier back down.
        let (_predicted2, idx_after_short) =
            resolve_num_predict("hi", "summary", 8192, false, grown_idx);
        assert_eq!(
            idx_after_short, grown_idx,
            "tier must never shrink automatically"
        );
    }

    #[test]
    fn num_predict_configured_max_still_caps_output() {
        let long_transcript = "word ".repeat(600);
        let (predicted, _tier_idx) =
            resolve_num_predict(&long_transcript, "summary", 200, false, 0);
        assert!(
            predicted <= 200,
            "user's explicit Token Limit setting must remain the absolute ceiling, got {}",
            predicted
        );
    }

    #[test]
    fn num_ctx_stays_at_baseline_for_typical_dictation() {
        let (num_ctx, tier_idx) = resolve_num_ctx(OLLAMA_PROMPT_EN, "a short dictation", 160, 0);
        assert_eq!(num_ctx, NUM_CTX_TIERS[0]);
        assert_eq!(tier_idx, 0);
    }

    #[test]
    fn num_ctx_grows_when_transcript_threatens_to_overflow_current_tier() {
        // ~3000 words is comfortably beyond what fits in the 2048 baseline tier
        // once the system prompt and output budget are added on top.
        let huge_transcript = "word ".repeat(3000);
        let (num_ctx, tier_idx) = resolve_num_ctx(OLLAMA_PROMPT_EN, &huge_transcript, 384, 0);
        assert!(
            num_ctx > NUM_CTX_TIERS[0],
            "should step up from the 2048 baseline, got {}",
            num_ctx
        );
        assert!(tier_idx > 0);
    }

    #[test]
    fn local_refinement_timeout_stays_below_paste_fallback_budget() {
        assert!(refinement_timeout_secs("hello world", OLLAMA_PROMPT_AUTO) < 30);
    }

    #[test]
    fn ollama_runner_command_detection_matches_runner_only() {
        assert!(is_ollama_runner_command_line(
            "ollama.exe",
            r#""C:\Users\trist\AppData\Local\Trispr Flow\ollama-runtime\0.24.0\ollama.exe" runner --model blob --port 54914"#
        ));
        assert!(is_ollama_runner_command_line(
            "ollama",
            "/usr/bin/ollama runner --model blob"
        ));
        assert!(!is_ollama_runner_command_line(
            "ollama.exe",
            r#""C:\Users\trist\AppData\Local\Trispr Flow\ollama-runtime\0.24.0\ollama.exe" serve"#
        ));
        assert!(!is_ollama_runner_command_line(
            "ollama.exe",
            r#""C:\ollama.exe" serve --runner-dir C:\runners"#
        ));
        assert!(!is_ollama_runner_command_line(
            "other.exe",
            r#""C:\other.exe" runner"#
        ));
    }

    // --- Task 32: list_ollama_models_with_size returns empty on bad endpoint ---
    #[test]
    fn list_ollama_models_with_size_returns_empty_on_bad_endpoint() {
        let models = list_ollama_models_with_size("http://127.0.0.1:19999");
        assert!(
            models.is_empty(),
            "expected empty list for unreachable endpoint"
        );
    }

    // --- Task 32: list_ollama_models delegates to list_ollama_models_with_size ---
    #[test]
    fn list_ollama_models_consistent_with_with_size() {
        let names = list_ollama_models("http://127.0.0.1:19999");
        let with_size = list_ollama_models_with_size("http://127.0.0.1:19999");
        assert_eq!(names.len(), with_size.len());
    }

    // --- Task 32: OllamaProvider refine_transcript maps connection refused to OllamaNotRunning ---
    #[test]
    fn ollama_provider_connection_refused_returns_not_running() {
        let provider = OllamaProvider::new("http://127.0.0.1:19999".to_string());
        let options = test_options(false);
        let result = provider.refine_transcript("hello world", "qwen3:14b", &options);
        // Accept both OllamaNotRunning and Timeout: some systems (filtered ports/firewall)
        // return a timeout instead of connection-refused for a closed local port. Both
        // indicate Ollama is not reachable.
        assert!(
            matches!(
                result,
                Err(AIError::OllamaNotRunning) | Err(AIError::Timeout)
            ),
            "unreachable endpoint should map to OllamaNotRunning or Timeout, got: {:?}",
            result
        );
    }

    // --- Task 35: English prompt contains key guard instructions ---
    #[test]
    fn prompt_en_contains_no_translate_guard() {
        assert!(
            OLLAMA_PROMPT_EN.contains("do NOT translate"),
            "EN prompt must explicitly forbid translation"
        );
    }

    #[test]
    fn prompt_en_contains_output_only_instruction() {
        assert!(
            OLLAMA_PROMPT_EN.contains("ONLY"),
            "EN prompt must tell the model to output only the corrected text"
        );
    }

    #[test]
    fn prompt_en_mentions_proper_nouns() {
        assert!(
            OLLAMA_PROMPT_EN.contains("proper nouns"),
            "EN prompt must mention preserving proper nouns"
        );
    }

    // --- Task 35: German prompt contains key guard instructions ---
    #[test]
    fn prompt_de_contains_no_translate_guard() {
        assert!(
            OLLAMA_PROMPT_DE.contains("NICHT übersetzen"),
            "DE prompt must explicitly forbid translation"
        );
    }

    #[test]
    fn prompt_de_contains_output_only_instruction() {
        assert!(
            OLLAMA_PROMPT_DE.contains("NUR"),
            "DE prompt must tell the model to output only the corrected text"
        );
    }

    #[test]
    fn prompt_de_contains_register_preservation() {
        assert!(
            OLLAMA_PROMPT_DE.contains("Anredeform"),
            "DE prompt must mention preserving formal/informal register (Du/Sie)"
        );
    }

    #[test]
    fn parse_chat_response_content() {
        let payload = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": "Refined text from chat endpoint."
            }
        });
        let parsed = parse_ollama_refined_text(&payload);
        assert_eq!(parsed.as_deref(), Some("Refined text from chat endpoint."));
    }

    #[test]
    fn parse_generate_response_content() {
        let payload = serde_json::json!({
            "response": "Refined text from generate endpoint."
        });
        let parsed = parse_ollama_refined_text(&payload);
        assert_eq!(
            parsed.as_deref(),
            Some("Refined text from generate endpoint.")
        );
    }

    #[test]
    fn chat_route_detection_handles_missing_route_patterns() {
        assert!(is_missing_chat_route(404, "not found"));
        assert!(is_missing_chat_route(
            400,
            "unknown route /api/chat on this server"
        ));
        assert!(!is_missing_chat_route(500, "internal server error"));
    }

    #[test]
    fn suspicious_refinement_falls_back_to_original_text() {
        let original = "This is a fairly long transcript sentence with multiple technical terms and enough context to detect aggressive truncation in refinement output.";
        let refined = "summary only";
        let sanitized = sanitize_ollama_refinement_output(original, refined, &test_options(false));
        assert_eq!(sanitized, original);
    }

    #[test]
    fn refinement_output_collapses_excessive_blank_lines() {
        let original = "First line\nSecond line\nThird line";
        let refined = "First line\n\n\n\nSecond line\n\n\nThird line";
        let sanitized = sanitize_ollama_refinement_output(original, refined, &test_options(false));
        assert_eq!(sanitized, "First line\n\nSecond line\n\nThird line");
    }

    #[test]
    fn echoed_transcript_wrapper_is_removed_from_the_output() {
        assert_eq!(
            strip_transcript_wrapper_tags("<transcript>\nText.\n</transcript>"),
            "Text."
        );
        assert_eq!(
            strip_transcript_wrapper_tags("  <transcript> Text. </transcript>\n"),
            "Text."
        );
        assert_eq!(
            strip_transcript_wrapper_tags("<Transcript>Text.</TRANSCRIPT>"),
            "Text."
        );
        // inner line breaks of a multi-line answer survive
        assert_eq!(
            strip_transcript_wrapper_tags("<transcript>\nEins.\nZwei.\n</transcript>"),
            "Eins.\nZwei."
        );
    }

    #[test]
    fn a_lone_closing_or_opening_tag_is_removed() {
        assert_eq!(
            strip_transcript_wrapper_tags("Text.\n</transcript>"),
            "Text."
        );
        assert_eq!(
            strip_transcript_wrapper_tags("<transcript>\nText."),
            "Text."
        );
        assert_eq!(strip_transcript_wrapper_tags("<transcript>Text."), "Text.");
    }

    #[test]
    fn cut_off_wrapper_tags_are_removed() {
        // opening tag cut down to its `<`, closing tag intact
        assert_eq!(
            strip_transcript_wrapper_tags("<bitte schick mir die Unterlagen</transcript>"),
            "bitte schick mir die Unterlagen"
        );
        assert_eq!(
            strip_transcript_wrapper_tags("<bitte schick mir die Unterlagen\n</transcript>"),
            "bitte schick mir die Unterlagen"
        );
        // shortened opening tag
        assert_eq!(strip_transcript_wrapper_tags("<transcr>\nText."), "Text.");
        assert_eq!(strip_transcript_wrapper_tags("<transcript\nText."), "Text.");
        // closing tag cut off at the end of the output
        assert_eq!(strip_transcript_wrapper_tags("Text.\n</transcr"), "Text.");
        assert_eq!(strip_transcript_wrapper_tags("Text.</transcript"), "Text.");
        assert_eq!(
            strip_transcript_wrapper_tags("<transcript>\nText.\n</trans"),
            "Text."
        );
    }

    #[test]
    fn text_without_wrapper_tags_is_returned_unchanged() {
        for text in [
            "Das ist ein Satz.",
            "  mit Rand und\nZeilenumbruch  \n",
            "Das transcript ist fertig.",
            "Ich lese das Transcript noch einmal vor.",
            "Transcript: erste Zeile",
            "Der Tag <transcript> steht mitten im Satz.",
            "Der Tag </transcript> steht mitten im Satz.",
            "Liste <transcripts> bleibt",
            "<b>fett</b> und normal",
            // a leading `<` alone is user text unless a closing tag proves the echo
            "<bitte schick mir das",
            "a < b und c > d",
            "Ende mit </div>",
            "Ende mit </",
            "<t> kurz",
        ] {
            assert_eq!(strip_transcript_wrapper_tags(text), text);
        }
        // a closing tag proves the echo only for a dangling `<word`, never for a real tag
        assert_eq!(
            strip_transcript_wrapper_tags("<b>fett</b> normal</transcript>"),
            "<b>fett</b> normal"
        );
    }

    #[test]
    fn sanitizer_removes_the_echoed_wrapper_before_judging_the_answer() {
        let original = "bitte schick mir die unterlagen";
        for refined in [
            "<transcript>\nBitte schick mir die Unterlagen.\n</transcript>",
            "Bitte schick mir die Unterlagen.\n</transcript>",
            "<transcript>\nBitte schick mir die Unterlagen.",
            "<Bitte schick mir die Unterlagen.</transcript>",
        ] {
            let sanitized =
                sanitize_ollama_refinement_output(original, refined, &test_options(false));
            assert!(
                !sanitized.contains("transcript>"),
                "{} -> {}",
                refined,
                sanitized
            );
            assert!(
                sanitized.ends_with("Unterlagen."),
                "{} -> {}",
                refined,
                sanitized
            );
        }
        assert_eq!(
            sanitize_ollama_refinement_output(
                original,
                "<transcript>\nBitte schick mir die Unterlagen.\n</transcript>",
                &test_options(false)
            ),
            "Bitte schick mir die Unterlagen."
        );
        // nothing but the tags is no answer: the original text is kept
        assert_eq!(
            sanitize_ollama_refinement_output(
                original,
                "<transcript>\n</transcript>",
                &test_options(false)
            ),
            original
        );
        // user text that merely mentions the word stays as dictated
        let mention = "das transcript ist fertig";
        assert_eq!(
            sanitize_ollama_refinement_output(
                mention,
                "Das transcript ist fertig.",
                &test_options(false)
            ),
            "Das transcript ist fertig."
        );
    }

    #[test]
    fn language_guard_rejects_high_confidence_de_to_en_drift() {
        let original = "das ist ein test und wir sind im meeting und die aufgabe ist nicht offen";
        let refined = "this is a test and we are in the meeting and the task is not open";
        let sanitized = sanitize_ollama_refinement_output(original, refined, &test_options(true));
        assert_eq!(sanitized, original);
    }

    #[test]
    fn language_guard_allows_drift_when_guard_disabled() {
        let original = "das ist ein test und wir sind im meeting und die aufgabe ist nicht offen";
        let refined = "this is a test and we are in the meeting and the task is not open";
        let sanitized = sanitize_ollama_refinement_output(original, refined, &test_options(false));
        assert_eq!(sanitized, refined);
    }

    #[test]
    fn language_guard_rejects_high_confidence_de_to_en_drift_for_custom_profile() {
        let original = "das ist ein test und wir sind im meeting und die aufgabe ist nicht offen";
        let refined = "this is a test and we are in the meeting and the task is not open";
        let options = test_options_for_profile(true, "custom");
        let sanitized = sanitize_ollama_refinement_output(original, refined, &options);
        assert_eq!(sanitized, original);
    }

    #[test]
    fn language_guard_allows_custom_profile_drift_when_disabled() {
        let original = "das ist ein test und wir sind im meeting und die aufgabe ist nicht offen";
        let refined = "this is a test and we are in the meeting and the task is not open";
        let options = test_options_for_profile(false, "custom");
        let sanitized = sanitize_ollama_refinement_output(original, refined, &options);
        assert_eq!(sanitized, refined);
    }

    #[test]
    fn language_guard_output_boundary_matrix_uses_profile_predicate() {
        let original = "das ist ein test und wir sind im meeting und die aufgabe ist nicht offen";
        let refined = "this is a test and we are in the meeting and the task is not open";
        let cases = [
            ("wording", true),
            ("wording", false),
            ("custom", true),
            ("custom", false),
            ("llm_prompt", true),
            ("llm_prompt", false),
        ];

        for (profile, preserve_source_language) in cases {
            let expected_guard =
                super::super::should_enforce_language_guard(preserve_source_language, profile);
            let mut options = test_options_for_profile(expected_guard, profile);
            options.enforce_language_guard = expected_guard;
            let sanitized = sanitize_ollama_refinement_output(original, refined, &options);
            let expected = if expected_guard { original } else { refined };
            assert_eq!(
                sanitized, expected,
                "profile={profile} preserve={preserve_source_language}"
            );
        }
    }

    #[test]
    fn llm_prompt_output_boundary_accepts_english_refinement() {
        let original = "erstelle einen prompt fuer das meeting";
        let refined = "Create a concise prompt for the meeting.";
        let options = test_options_for_profile(false, "llm_prompt");
        let sanitized = sanitize_ollama_refinement_output(original, refined, &options);
        assert_eq!(sanitized, refined);
    }

    #[test]
    fn language_guard_does_not_trip_on_mixed_language_input() {
        let original =
            "wir deployen den service and we monitor the logs im dashboard und in production";
        let refined = "we deploy the service and monitor the logs in the dashboard and production";
        let sanitized = sanitize_ollama_refinement_output(original, refined, &test_options(true));
        assert_eq!(sanitized, refined);
    }

    // --- SSRF guard: cloud metadata + link-local must be blocked, loopback + LAN must pass ---
    #[test]
    fn ssrf_target_blocks_aws_metadata_ip() {
        assert!(is_ssrf_target("http://169.254.169.254:11434"));
    }

    #[test]
    fn ssrf_target_blocks_gcp_metadata_hostname() {
        assert!(is_ssrf_target("http://metadata.google.internal:11434"));
    }

    #[test]
    fn ssrf_target_blocks_arbitrary_link_local_ipv4() {
        assert!(is_ssrf_target("http://169.254.100.200:11434"));
    }

    #[test]
    fn ssrf_target_blocks_ipv6_link_local() {
        assert!(is_ssrf_target("http://[fe80::1]:11434"));
    }

    #[test]
    fn ssrf_target_fails_closed_on_unparseable_endpoint() {
        // Unparseable input must be treated as a target (fail-closed),
        // not silently allowed through.
        assert!(is_ssrf_target("not-a-url"));
    }

    #[test]
    fn ssrf_target_allows_loopback() {
        assert!(!is_ssrf_target("http://127.0.0.1:11434"));
    }

    #[test]
    fn ssrf_target_allows_private_lan() {
        // Private LAN ranges (RFC 1918) are not in the SSRF block list.
        // Operators may legitimately run Ollama on a LAN host.
        assert!(!is_ssrf_target("http://192.168.1.20:11434"));
    }

    #[test]
    fn ssrf_target_blocks_ipv4_mapped_ipv6_link_local() {
        // ::ffff:169.254.x.x is the IPv6 representation of an IPv4 link-local
        // address. The function explicitly handles this via to_ipv4_mapped().
        assert!(is_ssrf_target("http://[::ffff:169.254.169.254]:11434"));
    }
}

#[cfg(test)]
mod pull_stream_tests {
    use super::*;
    use crate::ai_fallback::test_support::{MockReply, MockServer};
    use std::io::BufRead;
    use std::time::Instant;

    fn progress_line(completed: u64) -> String {
        format!(
            "{{\"status\":\"pulling 370c2879f176\",\"digest\":\"sha256:370c\",\"total\":1000,\"completed\":{}}}",
            completed
        )
    }

    /// Runs the consumer over `reader`, returning how it ended and what the callbacks saw.
    fn consume<R: BufRead>(reader: R) -> (PullStreamEnd, u64, u64) {
        let mut logged = 0u64;
        let mut progressed = 0u64;
        let end = consume_pull_stream(reader, &mut |_, _| logged += 1, &mut |_| progressed += 1);
        (end, logged, progressed)
    }

    fn text(lines: &[String]) -> std::io::Cursor<Vec<u8>> {
        std::io::Cursor::new((lines.join("\n") + "\n").into_bytes())
    }

    #[test]
    fn a_pull_that_reaches_success_ends_with_success() {
        let reader = text(&[
            "{\"status\":\"pulling manifest\"}".to_string(),
            progress_line(500),
            "{\"status\":\"success\"}".to_string(),
        ]);
        let (end, logged, progressed) = consume(reader);
        assert_eq!(end, PullStreamEnd::Success { lines: 3 });
        assert_eq!(logged, 3, "every line is logged");
        assert_eq!(
            progressed, 2,
            "the final success line is not a progress event"
        );
    }

    #[test]
    fn an_error_line_ends_the_pull_after_its_progress_event() {
        let reader = text(&[
            progress_line(1),
            "{\"error\":\"pull model manifest: file does not exist\"}".to_string(),
            progress_line(2),
        ]);
        let (end, _, progressed) = consume(reader);
        assert_eq!(
            end,
            PullStreamEnd::Failed {
                error: "pull model manifest: file does not exist".to_string(),
                lines: 2
            }
        );
        assert_eq!(progressed, 2);
    }

    #[test]
    fn blank_and_garbage_lines_are_skipped_and_not_counted() {
        let reader = text(&[
            String::new(),
            "   ".to_string(),
            "this is not json".to_string(),
            "{\"status\":\"pulling manifest\"}".to_string(),
            "<html>".to_string(),
        ]);
        let (end, logged, _) = consume(reader);
        assert_eq!(end, PullStreamEnd::EndedEarly { lines: 1 });
        assert_eq!(logged, 1);
    }

    #[test]
    fn a_stream_that_ends_cleanly_without_success_is_ended_early() {
        let (end, _, _) = consume(text(&[progress_line(10), progress_line(20)]));
        assert_eq!(end, PullStreamEnd::EndedEarly { lines: 2 });
        let (end, logged, _) = consume(std::io::Cursor::new(Vec::<u8>::new()));
        assert_eq!(end, PullStreamEnd::EndedEarly { lines: 0 });
        assert_eq!(logged, 0);
    }

    /// 50 pairs of progress lines (~5 KB), announced in full but cut off half way: Ollama was
    /// killed or restarted, the network dropped.
    fn broken_pull_server() -> (MockServer, usize) {
        let body = (1..=100u64)
            .map(|i| progress_line(i * 10) + "\n")
            .collect::<String>();
        let half = body.len() / 2;
        let served = body.clone();
        (
            MockServer::start(move |_| MockReply::text(200, &served).cut_at(half)),
            half,
        )
    }

    #[test]
    fn a_connection_that_breaks_mid_pull_ends_the_pull_instead_of_spinning() {
        let (server, _) = broken_pull_server();
        let response = ureq::get(&format!("{}/api/pull", server.base_url))
            .call()
            .unwrap();
        let started = Instant::now();

        let mut seen = 0u64;
        let end = consume_pull_stream(
            std::io::BufReader::new(response.into_reader()),
            &mut |_, _| seen += 1,
            &mut |_| {},
        );

        match end {
            PullStreamEnd::Broken { error, lines } => {
                assert!(
                    lines > 10 && lines < 100,
                    "the complete lines before the cut: {}",
                    lines
                );
                assert!(!error.is_empty());
                assert_eq!(seen, lines);
            }
            other => panic!(
                "a broken connection must end the pull as Broken: {:?}",
                other
            ),
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn control_a_broken_reader_keeps_failing_so_skipping_its_errors_never_ends() {
        // Why `consume_pull_stream` returns on the first read error: `lines()` over a broken
        // connection yields an error on every call, forever. The old loop skipped errors with
        // `continue`, which turned this into a busy loop (measured: 100,000 errors in 0.13 s)
        // that kept the model "pull in progress" until the app was restarted.
        let (server, _) = broken_pull_server();
        let response = ureq::get(&format!("{}/api/pull", server.base_url))
            .call()
            .unwrap();
        let buffered = std::io::BufReader::new(response.into_reader());
        let mut errors = 0usize;
        let mut calls = 0usize;
        for line in buffered.lines() {
            calls += 1;
            if line.is_err() {
                errors += 1;
            }
            if calls >= 1000 {
                break;
            }
        }
        assert_eq!(calls, 1000, "the iterator never ran dry");
        assert!(errors > 900, "errors: {}", errors);
    }
}

#[cfg(test)]
mod quick_ping_tests {
    use super::*;
    use crate::ai_fallback::test_support::{MockReply, MockServer};

    /// The cache is process-wide and these two tests are the only ones that use it: they take
    /// turns, and whatever they leave in it is cleared again.
    static CACHE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct CacheReset;

    impl Drop for CacheReset {
        fn drop(&mut self) {
            reset_quick_ping_cache();
        }
    }

    #[test]
    fn the_cached_quick_ping_outlives_the_server_but_the_fresh_ping_does_not() {
        let _turn = CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _reset = CacheReset;
        reset_quick_ping_cache();
        let server = MockServer::start(|_| MockReply::ok(serde_json::json!({ "models": [] })));
        let endpoint = server.base_url.clone();
        assert!(ping_ollama_quick(&endpoint).is_ok(), "the server answers");

        drop(server); // the server is gone

        assert!(
            ping_ollama_quick(&endpoint).is_ok(),
            "control: within 3 s the cache still says it is there (why start/stop must not use it)"
        );
        assert!(
            ping_ollama_quick_fresh(&endpoint).is_err(),
            "the fresh ping sees that nothing answers any more"
        );
        reset_quick_ping_cache();
        assert!(
            ping_ollama_quick(&endpoint).is_err(),
            "after a reset the next cached check looks again"
        );
    }

    #[test]
    fn the_fresh_ping_sees_a_server_that_just_came_up_and_leaves_the_cache_alone() {
        let _turn = CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _reset = CacheReset;
        reset_quick_ping_cache();
        let server = MockServer::start(|_| MockReply::ok(serde_json::json!({ "models": [] })));
        assert!(ping_ollama_quick_fresh(&server.base_url).is_ok());
        assert_eq!(
            QUICK_PING_CACHE_TS.load(Ordering::Relaxed),
            0,
            "the fresh ping does not write the cache"
        );
        assert!(ping_ollama_quick_fresh("http://127.0.0.1:1").is_err());
    }
}
