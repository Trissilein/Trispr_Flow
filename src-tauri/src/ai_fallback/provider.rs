use super::error::AIError;
use super::models::{RefinementOptions, RefinementResult, TokenUsage};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tracing::warn;
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

/// Returns a per-request ureq agent whose read timeout scales with the combined
/// token count of input text and system prompt.
/// Range: 60 s (short inputs) … 120 s (long/multilingual prompts).
fn refinement_agent(input_text: &str, system_prompt: &str) -> ureq::Agent {
    let total_tokens = rough_token_estimate(input_text) + rough_token_estimate(system_prompt);
    let timeout_secs = ((total_tokens as f64 / 500.0 * 1.5) + 30.0).clamp(60.0, 120.0) as u64;
    ureq::builder()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(timeout_secs))
        .build()
}

// Prompt templates optimized for local models (Ollama: qwen3, mistral-small).
// Guidelines: no translation, no explanations, preserve register and proper nouns.
pub const OLLAMA_PROMPT_EN: &str = "You are a transcript editor. Fix punctuation, capitalization, and obvious speech-to-text errors in the text below. Rules: do NOT translate; do NOT add explanations or commentary; preserve all proper nouns and technical terms exactly; preserve the original register (formal/informal); do NOT add line breaks, paragraph breaks, or tabs that are not in the original. Output ONLY the corrected text with no preamble.";

pub const OLLAMA_PROMPT_DE: &str = "Du bist ein Transkript-Editor. Korrigiere Zeichensetzung, Groß-/Kleinschreibung und offensichtliche Sprache-zu-Text-Fehler im Text unten. Regeln: NICHT übersetzen; KEINE Erklärungen oder Kommentare hinzufügen; alle Eigennamen und Fachbegriffe exakt beibehalten; Anredeform (Du/Sie) aus dem Original beibehalten; KEINE zusätzlichen Zeilenumbrüche, Absätze oder Tabulatoren einfügen. Gib NUR den korrigierten Text aus, ohne Einleitung.";

// Used when Whisper language is set to auto-detect. Language-neutral phrasing avoids
// the model inferring "output in English" from an English-only system prompt.
pub const OLLAMA_PROMPT_AUTO: &str = "IMPORTANT: Detect the language of the input text and output in that SAME language — never translate. You are a transcript editor. Fix punctuation, capitalization, and obvious speech-to-text errors. Rules: do NOT translate under any circumstances; do NOT add explanations; preserve all proper nouns and technical terms exactly; preserve the original register; do NOT add line breaks, paragraph breaks, or tabs not present in the original. Output ONLY the corrected text with no preamble.";

const OLLAMA_PROMPT_SUMMARY_EN: &str = "Summarize this transcript into 3 to 6 concise bullet points. Preserve key facts, numbers, names, and decisions. Do not invent information. If something is uncertain, state it cautiously. Return only the bullet list.";

const OLLAMA_PROMPT_SUMMARY_DE: &str = "Fasse dieses Transkript in 3 bis 6 praegnanten Stichpunkten zusammen. Behalte wichtige Fakten, Zahlen, Namen und Entscheidungen bei. Keine Informationen erfinden. Unsichere Inhalte vorsichtig formulieren. Gib nur die Stichpunktliste zurueck.";

const OLLAMA_PROMPT_TECHNICAL_EN: &str = "Rewrite this transcript in technical specification style. Keep exact numbers, units, versions, APIs, constraints, and file paths. Structure output with short sections: Goal, Inputs, Outputs, Constraints, Open Questions. Do not invent missing values. Return only the structured result.";

const OLLAMA_PROMPT_TECHNICAL_DE: &str = "Formuliere dieses Transkript als technische Spezifikation um. Behalte exakte Zahlen, Einheiten, Versionen, APIs, Rahmenbedingungen und Dateipfade. Strukturiere die Ausgabe mit kurzen Abschnitten: Ziel, Eingaben, Ausgaben, Constraints, Offene Fragen. Keine fehlenden Werte erfinden. Gib nur das strukturierte Ergebnis zurueck.";

const OLLAMA_PROMPT_ACTION_ITEMS_EN: &str = "Convert this transcript into actionable tasks. Use bullets with format: [Action] [Owner?] [Due?] [Notes]. Preserve technical wording and constraints. If owner or due date is missing, mark it as unknown. Return only the action list.";

const OLLAMA_PROMPT_ACTION_ITEMS_DE: &str = "Wandle dieses Transkript in konkrete Aufgaben um. Nutze Stichpunkte im Format: [Aktion] [Owner?] [Faellig?] [Hinweise]. Behalte technische Begriffe und Rahmenbedingungen bei. Wenn Owner oder Datum fehlen, mit unknown markieren. Gib nur die Aufgabenliste zurueck.";

const OLLAMA_PROMPT_LLM_PROMPT_EN: &str = "You are an expert prompt engineer. Convert the following spoken dictation into a precise, high-quality prompt for a large language model.\n\nA well-structured prompt must include:\n1. A clear role or persona (e.g. \"You are a...\")\n2. An unambiguous task description\n3. Relevant context, background, or constraints\n4. Desired output format (if applicable)\n\nRules:\n- Always write the resulting prompt in English, regardless of the input language.\n- Do not explain, comment, or add preamble.\n- Do not address the speaker or reference this conversation.\n- Return only the final ready-to-use prompt, nothing else.";

const OLLAMA_PROMPT_LLM_PROMPT_DE: &str = "Du bist ein erfahrener Prompt-Engineer. Wandle die folgende gesprochene Eingabe in einen praezisen, einsatzbereiten Prompt fuer ein grosses Sprachmodell um.\n\nEin guter Prompt enthaelt:\n1. Eine klare Rolle oder Persona (z.B. \"You are a...\")\n2. Eine eindeutige Aufgabenbeschreibung\n3. Relevanten Kontext, Hintergrund oder Einschraenkungen\n4. Das gewuenschte Ausgabeformat (falls zutreffend)\n\nRegeln:\n- Schreibe den fertigen Prompt immer auf Englisch, unabhaengig von der Eingabesprache.\n- Keine Erklaerungen, Kommentare oder Vorbemerkungen.\n- Den Sprecher nicht adressieren und nicht auf dieses Gespraech verweisen.\n- Gib nur den fertigen Prompt zurueck, nichts weiteres.";

pub trait AIProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn validate_api_key(&self, api_key: &str) -> Result<(), AIError>;
    fn estimate_cost_usd(&self, model: &str, input_tokens: usize, output_tokens: usize) -> f64;
    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        api_key: &str,
    ) -> Result<RefinementResult, AIError>;
}

pub struct ProviderFactory;

impl ProviderFactory {
    pub fn create(provider: &str) -> Result<Box<dyn AIProvider>, AIError> {
        match normalize_provider(provider) {
            "claude" => Ok(Box::new(ClaudeProvider)),
            "openai" => Ok(Box::new(OpenAIProvider)),
            "gemini" => Ok(Box::new(GeminiProvider)),
            "ollama" => Ok(Box::new(OllamaProvider::new())),
            other => Err(AIError::UnknownProvider(other.to_string())),
        }
    }

    pub fn create_ollama(endpoint: String) -> Box<dyn AIProvider> {
        Box::new(OllamaProvider::with_endpoint(endpoint))
    }

    pub fn create_lm_studio(endpoint: String, api_key: String) -> Box<dyn AIProvider> {
        Box::new(OpenAICompatProvider::lm_studio(endpoint, api_key))
    }

    pub fn create_oobabooga(endpoint: String, api_key: String) -> Box<dyn AIProvider> {
        Box::new(OpenAICompatProvider::oobabooga(endpoint, api_key))
    }
}

fn normalize_ollama_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "http://127.0.0.1:11434".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn is_local_ollama_endpoint(endpoint: &str) -> bool {
    let normalized = normalize_ollama_endpoint(endpoint);
    let parsed = match Url::parse(&normalized) {
        Ok(url) => url,
        Err(_) => return false,
    };
    if parsed.scheme() != "http" {
        return false;
    }
    let host = parsed
        .host_str()
        .map(|h| h.to_ascii_lowercase())
        .unwrap_or_default();
    if host != "localhost" && host != "127.0.0.1" {
        return false;
    }
    if parsed.port_or_known_default().unwrap_or(0) != 11434 {
        return false;
    }
    if parsed.path() != "/" && !parsed.path().is_empty() {
        return false;
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return false;
    }
    true
}

/// Check if an endpoint targets a known SSRF-sensitive address
/// (cloud metadata services, link-local ranges).
pub fn is_ssrf_target(endpoint: &str) -> bool {
    let normalized = normalize_ollama_endpoint(endpoint);
    let parsed = match Url::parse(&normalized) {
        Ok(url) => url,
        Err(_) => return true, // Fail-closed: unparseable URLs treated as SSRF targets
    };
    let host = parsed
        .host_str()
        .map(|h| h.to_ascii_lowercase())
        .unwrap_or_default();
    // Block cloud metadata endpoint (AWS/GCP/Azure)
    if host == "169.254.169.254" || host == "metadata.google.internal" {
        return true;
    }
    // Block IPv4 link-local range (169.254.x.x) entirely
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        if ip.octets()[0] == 169 && ip.octets()[1] == 254 {
            return true;
        }
    }
    // Block IPv6 link-local (fe80::/10) and IPv4-mapped link-local (::ffff:169.254.x.x).
    // host_str() already strips brackets from IPv6 literals like [fe80::1].
    if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
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
    false
}

/// Return all endpoint candidates: primary (with localhost/127.0.0.1 variants)
/// followed by each configured fallback endpoint and its own variants.
/// SSRF targets are silently filtered out.
pub fn ollama_all_endpoint_candidates(primary: &str, fallbacks: &[String]) -> Vec<String> {
    let mut all = ollama_endpoint_candidates(primary);
    for fb in fallbacks {
        if !is_ssrf_target(fb) {
            for candidate in ollama_endpoint_candidates(fb) {
                if !all.contains(&candidate) {
                    all.push(candidate);
                }
            }
        }
    }
    all
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

#[derive(Debug, Clone)]
pub struct LocalModelResolution {
    pub model: String,
    pub repaired: bool,
}

fn normalize_model_tag(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn resolve_installed_model_tag(installed: &[String], candidate: &str) -> Option<String> {
    let target = normalize_model_tag(candidate);
    if target.is_empty() {
        return None;
    }
    installed
        .iter()
        .find(|name| normalize_model_tag(name) == target)
        .cloned()
}

pub fn resolve_effective_local_model(
    configured_model: &str,
    preferred_model: &str,
    endpoint: &str,
) -> Result<LocalModelResolution, AIError> {
    let installed = list_ollama_models(endpoint);
    if installed.is_empty() {
        if ping_ollama(endpoint).is_err() {
            return Err(AIError::OllamaNotRunning);
        }
        return Err(AIError::NetworkError(
            "No local Ollama model configured. Download or import a model first.".to_string(),
        ));
    }

    let configured = configured_model.trim();
    if let Some(found) = resolve_installed_model_tag(&installed, configured) {
        let repaired = !configured.eq_ignore_ascii_case(&found);
        return Ok(LocalModelResolution {
            model: found,
            repaired,
        });
    }

    let preferred = preferred_model.trim();
    if let Some(found) = resolve_installed_model_tag(&installed, preferred) {
        return Ok(LocalModelResolution {
            model: found,
            repaired: true,
        });
    }

    Ok(LocalModelResolution {
        model: installed[0].clone(),
        repaired: true,
    })
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

    let agent = ureq::builder()
        .timeout_connect(Duration::from_millis(300))
        .timeout_read(Duration::from_millis(300))
        .build();

    let mut success = false;
    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/tags", candidate);
        if agent.get(&url).call().is_ok() {
            success = true;
            break;
        }
    }

    QUICK_PING_CACHE_OK.store(success, Ordering::Relaxed);
    QUICK_PING_CACHE_TS.store(now, Ordering::Relaxed);

    if success {
        Ok(())
    } else {
        Err(AIError::OllamaNotRunning)
    }
}

/// Quick reachability check for LM Studio (or any OpenAI-compatible local server).
/// Returns Ok(()) if the server responds and has at least one model loaded.
/// Timeout is 500 ms — suitable for pre-flight checks before refinement.
pub fn ping_lm_studio_quick(endpoint: &str) -> Result<(), AIError> {
    let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
    let agent = ureq::builder()
        .timeout_connect(Duration::from_millis(500))
        .timeout_read(Duration::from_millis(500))
        .build();
    match agent.get(&url).call() {
        Ok(resp) => {
            if let Ok(body) = resp.into_json::<serde_json::Value>() {
                let has_models = body["data"].as_array().map_or(false, |arr| !arr.is_empty());
                if has_models {
                    Ok(())
                } else {
                    Err(AIError::NetworkError(
                        "LM Studio is running but no models are loaded".into(),
                    ))
                }
            } else {
                Err(AIError::NetworkError(
                    "LM Studio returned invalid JSON".into(),
                ))
            }
        }
        Err(_) => Err(AIError::NetworkError(format!(
            "LM Studio not reachable at {}",
            endpoint
        ))),
    }
}

pub fn default_models_for_provider(provider: &str) -> Vec<String> {
    match normalize_provider(provider) {
        "claude" => vec![
            "claude-3-5-sonnet-20241022".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
            "claude-3-opus-20240229".to_string(),
        ],
        "openai" => vec![
            "gpt-4o-mini".to_string(),
            "gpt-4o".to_string(),
            "gpt-4.1-mini".to_string(),
        ],
        "gemini" => vec![
            "gemini-2.0-flash".to_string(),
            "gemini-1.5-pro".to_string(),
            "gemini-1.5-flash".to_string(),
        ],
        "ollama" => vec![], // Models discovered dynamically via /api/tags
        _ => vec![],
    }
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
            return Some(default_prompt_for_language(language).to_string());
        }
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

fn normalize_provider(provider: &str) -> &str {
    // Delegates to the canonical normalize_provider_id in models.rs.
    // The only difference: unrecognized providers yield "unknown" here (not "ollama")
    // so callers like ProviderFactory::create can surface a proper error.
    let canonical = super::models::normalize_provider_id(provider);
    let trimmed = provider.trim();
    if canonical == "ollama" && !trimmed.eq_ignore_ascii_case("ollama") {
        "unknown"
    } else {
        canonical
    }
}

fn rough_token_estimate(text: &str) -> usize {
    let words = text.split_whitespace().count();
    (words as f64 * 1.35).ceil().max(1.0) as usize
}

fn validate_key_basic(api_key: &str, provider: &str) -> Result<(), AIError> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err(AIError::MissingApiKey(provider.to_string()));
    }
    if trimmed.len() < 12 {
        return Err(AIError::InvalidApiKey(format!(
            "{} key is too short",
            provider
        )));
    }
    Ok(())
}

fn passthrough_refinement(
    provider: &dyn AIProvider,
    text: &str,
    model: &str,
    options: &RefinementOptions,
) -> RefinementResult {
    let input_tokens = rough_token_estimate(text);
    let output_tokens = input_tokens.min(options.max_tokens as usize);
    let cost = provider.estimate_cost_usd(model, input_tokens, output_tokens);
    RefinementResult {
        text: text.to_string(),
        usage: TokenUsage {
            input_tokens,
            output_tokens,
            total_cost_usd: cost,
        },
        provider: provider.id().to_string(),
        model: model.to_string(),
        execution_time_ms: 0,
    }
}

fn extract_ollama_error_message(response: ureq::Response) -> String {
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

fn parse_ollama_usage(json: &serde_json::Value) -> (usize, usize) {
    (
        json["prompt_eval_count"].as_u64().unwrap_or(0) as usize,
        json["eval_count"].as_u64().unwrap_or(0) as usize,
    )
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

fn adaptive_num_predict(
    input_text: &str,
    system_prompt: &str,
    configured_max: u32,
    low_latency_mode: bool,
) -> u32 {
    let configured = configured_max.clamp(128, 8192);
    let input_tokens = rough_token_estimate(input_text);
    let prompt_tokens = rough_token_estimate(system_prompt);
    let total = input_tokens + prompt_tokens;
    let heuristic = if low_latency_mode {
        ((total * 2) + 24).clamp(64, 512) as u32
    } else {
        ((total * 3) + 48).clamp(96, 1536) as u32
    };
    configured.min(heuristic.max(64))
}

fn adaptive_num_ctx(input_text: &str, system_prompt: &str, low_latency_mode: bool) -> usize {
    let tokens = rough_token_estimate(input_text) + rough_token_estimate(system_prompt);
    let max_ctx = if low_latency_mode { 2048 } else { 4096 };
    let target = (tokens * 2).clamp(1024, max_ctx);
    if target <= 1024 {
        1024
    } else if target <= 2048 {
        2048
    } else if !low_latency_mode && target <= 3072 {
        3072
    } else {
        max_ctx
    }
}

fn build_ollama_options_payload(
    options: &RefinementOptions,
    input_text: &str,
    system_prompt: &str,
) -> serde_json::Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "temperature".to_string(),
        serde_json::json!(options.temperature),
    );
    let num_predict = adaptive_num_predict(
        input_text,
        system_prompt,
        options.max_tokens,
        options.low_latency_mode,
    );
    payload.insert("num_predict".to_string(), serde_json::json!(num_predict));
    let num_ctx = parse_env_usize("TRISPR_OLLAMA_NUM_CTX")
        .map(|n| n.clamp(1024, 8192))
        .unwrap_or_else(|| adaptive_num_ctx(input_text, system_prompt, options.low_latency_mode));
    payload.insert("num_ctx".to_string(), serde_json::json!(num_ctx));

    let num_thread =
        parse_env_usize("TRISPR_OLLAMA_NUM_THREAD").unwrap_or_else(default_ollama_num_thread);
    payload.insert("num_thread".to_string(), serde_json::json!(num_thread));

    // Optional advanced override. Example: TRISPR_OLLAMA_NUM_GPU=999
    // to request aggressive GPU offload if supported by local runtime.
    if let Some(num_gpu) = parse_env_usize("TRISPR_OLLAMA_NUM_GPU") {
        payload.insert("num_gpu".to_string(), serde_json::json!(num_gpu));
    }

    serde_json::Value::Object(payload)
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
fn strip_thinking_tags(text: &str) -> String {
    if !text.contains("<think>") {
        return text.to_string();
    }
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?s)<think>.*?</think>\s*").unwrap());
    RE.replace_all(text, "").trim().to_string()
}

fn sanitize_ollama_refinement_output(
    original: &str,
    refined: &str,
    options: &RefinementOptions,
) -> String {
    let normalized = refined
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', " "); // Tabs → Space (Modell soll keine Tabs einfügen)
    let collapsed = collapse_excessive_blank_lines(normalized.trim());
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

struct ClaudeProvider;
struct OpenAIProvider;
struct GeminiProvider;

#[derive(Clone)]
struct OllamaProvider {
    endpoint: String,
}

impl OllamaProvider {
    fn new() -> Self {
        Self {
            endpoint: "http://127.0.0.1:11434".to_string(),
        }
    }

    fn with_endpoint(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl AIProvider for ClaudeProvider {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn validate_api_key(&self, api_key: &str) -> Result<(), AIError> {
        validate_key_basic(api_key, self.id())?;
        Ok(())
    }

    fn estimate_cost_usd(&self, _model: &str, input_tokens: usize, output_tokens: usize) -> f64 {
        // Approximation for planning UI only.
        (input_tokens as f64 * 0.000003) + (output_tokens as f64 * 0.000015)
    }

    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        api_key: &str,
    ) -> Result<RefinementResult, AIError> {
        self.validate_api_key(api_key)?;
        Ok(passthrough_refinement(self, text, model, options))
    }
}

impl AIProvider for OpenAIProvider {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn validate_api_key(&self, api_key: &str) -> Result<(), AIError> {
        validate_key_basic(api_key, self.id())?;
        Ok(())
    }

    fn estimate_cost_usd(&self, _model: &str, input_tokens: usize, output_tokens: usize) -> f64 {
        (input_tokens as f64 * 0.000005) + (output_tokens as f64 * 0.000015)
    }

    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        api_key: &str,
    ) -> Result<RefinementResult, AIError> {
        self.validate_api_key(api_key)?;
        Ok(passthrough_refinement(self, text, model, options))
    }
}

impl AIProvider for GeminiProvider {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn validate_api_key(&self, api_key: &str) -> Result<(), AIError> {
        validate_key_basic(api_key, self.id())?;
        Ok(())
    }

    fn estimate_cost_usd(&self, _model: &str, input_tokens: usize, output_tokens: usize) -> f64 {
        (input_tokens as f64 * 0.0000015) + (output_tokens as f64 * 0.0000035)
    }

    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        api_key: &str,
    ) -> Result<RefinementResult, AIError> {
        self.validate_api_key(api_key)?;
        Ok(passthrough_refinement(self, text, model, options))
    }
}

impl AIProvider for OllamaProvider {
    fn id(&self) -> &'static str {
        "ollama"
    }

    fn validate_api_key(&self, _api_key: &str) -> Result<(), AIError> {
        // Ollama doesn't use API keys
        Ok(())
    }

    fn estimate_cost_usd(&self, _model: &str, _input_tokens: usize, _output_tokens: usize) -> f64 {
        // Local Ollama has no cost
        0.0
    }

    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        _api_key: &str,
    ) -> Result<RefinementResult, AIError> {
        let start = Instant::now();
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
        let ollama_options = build_ollama_options_payload(options, text, system_prompt);
        let keep_alive = std::env::var("TRISPR_OLLAMA_KEEP_ALIVE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "60m".to_string());

        let chat_body = serde_json::json!({
            "model": model,
            "stream": false,
            "think": false,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": text }
            ],
            "options": ollama_options.clone(),
            "keep_alive": keep_alive.clone()
        });

        let generate_body = serde_json::json!({
            "model": model,
            "prompt": format!("{}\n\n{}", system_prompt, text),
            "stream": false,
            "think": false,
            "options": ollama_options,
            "keep_alive": keep_alive
        });

        // Per-request agent with timeout scaled to input + prompt complexity.
        // Short inputs: 60 s; long/multilingual prompts: up to 120 s.
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
                    last_transport_error =
                        Some(if msg.contains("timed out") || msg.contains("timeout") {
                            AIError::Timeout
                        } else {
                            AIError::OllamaNotRunning
                        });
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
                let (input_tokens, output_tokens) = parse_ollama_usage(&json);
                let elapsed_ms = start.elapsed().as_millis() as u64;
                return Ok(RefinementResult {
                    text: refined_text,
                    usage: TokenUsage {
                        input_tokens,
                        output_tokens,
                        total_cost_usd: 0.0,
                    },
                    provider: self.id().to_string(),
                    model: model.to_string(),
                    execution_time_ms: elapsed_ms,
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
                        return Err(AIError::NetworkError(
                            "Ollama route not found: neither /api/chat nor /api/generate is available."
                                .to_string(),
                        ));
                    }
                    return Err(map_ollama_http_error(code, &detail));
                }
                Err(ureq::Error::Transport(t)) => {
                    let msg = t.to_string().to_lowercase();
                    last_transport_error =
                        Some(if msg.contains("timed out") || msg.contains("timeout") {
                            AIError::Timeout
                        } else {
                            AIError::OllamaNotRunning
                        });
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
            let (input_tokens, output_tokens) = parse_ollama_usage(&json);
            let elapsed_ms = start.elapsed().as_millis() as u64;

            return Ok(RefinementResult {
                text: refined_text,
                usage: TokenUsage {
                    input_tokens,
                    output_tokens,
                    total_cost_usd: 0.0,
                },
                provider: self.id().to_string(),
                model: model.to_string(),
                execution_time_ms: elapsed_ms,
            });
        }

        Err(last_transport_error.unwrap_or(AIError::OllamaNotRunning))
    }
}

// ============================================================================
/// Truncate input text for local inference to prevent extremely long refinement
/// times on CPU-bound models. Cloud providers have much higher token limits and
/// are not truncated.
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

// OpenAI-compatible provider (LM Studio, Oobabooga, any /v1/chat/completions)
// ============================================================================

pub struct OpenAICompatProvider {
    pub endpoint: String,
    pub api_key: String,
    pub provider_id: &'static str,
    pub label: &'static str,
}

impl OpenAICompatProvider {
    fn lm_studio(endpoint: String, api_key: String) -> Self {
        Self {
            endpoint,
            api_key,
            provider_id: "lm_studio",
            label: "LM Studio",
        }
    }

    fn oobabooga(endpoint: String, api_key: String) -> Self {
        Self {
            endpoint,
            api_key,
            provider_id: "oobabooga",
            label: "Oobabooga",
        }
    }
}

impl AIProvider for OpenAICompatProvider {
    fn id(&self) -> &'static str {
        self.provider_id
    }

    fn validate_api_key(&self, _api_key: &str) -> Result<(), AIError> {
        // Local OpenAI-compat backends (LM Studio, Oobabooga) don't require API keys.
        Ok(())
    }

    fn estimate_cost_usd(&self, _model: &str, _input_tokens: usize, _output_tokens: usize) -> f64 {
        0.0
    }

    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        _api_key: &str,
    ) -> Result<RefinementResult, AIError> {
        let start = Instant::now();
        let model = model.trim();
        if model.is_empty() {
            return Err(AIError::NetworkError(format!(
                "No model selected for {}. Set an active model in AI Refinement settings.",
                self.label
            )));
        }

        let system_prompt = options
            .custom_prompt
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                let lang = options.language.as_deref().unwrap_or("en");
                default_prompt_for_language(lang)
            });

        let system_prompt = with_language_lock(
            system_prompt,
            options.language.as_deref().unwrap_or(""),
            options.enforce_language_guard,
        );

        // Limit input size for local inference to prevent multi-minute refinement times
        let effective_text = truncate_for_local_inference(text, 2000);

        let body = serde_json::json!({
            "model": model,
            "stream": false,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": effective_text }
            ],
            "temperature": options.temperature,
            "max_tokens": options.max_tokens,
            "chat_template_kwargs": {"enable_thinking": false}
        });

        let agent = refinement_agent(&effective_text, &system_prompt);
        let endpoint = normalize_ollama_endpoint(&self.endpoint);
        let url = format!("{}/v1/chat/completions", endpoint);

        let mut request = agent.post(&url).set("Content-Type", "application/json");

        if !self.api_key.is_empty() {
            request = request.set("Authorization", &format!("Bearer {}", self.api_key));
        }

        let response = match request.send_json(body) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                if code == 404 {
                    return Err(AIError::NetworkError(format!(
                        "{} endpoint not reachable at {}. Is {} running?",
                        self.label, url, self.label
                    )));
                }
                return Err(AIError::NetworkError(format!(
                    "{} returned HTTP {}: {}",
                    self.label, code, body
                )));
            }
            Err(ureq::Error::Transport(t)) => {
                let msg = t.to_string().to_lowercase();
                return Err(if msg.contains("timed out") || msg.contains("timeout") {
                    AIError::Timeout
                } else {
                    AIError::NetworkError(format!(
                        "{} not reachable at {}. Is it running?",
                        self.label, url
                    ))
                });
            }
        };

        let json: serde_json::Value = response.into_json().map_err(|e| {
            AIError::NetworkError(format!("Failed to parse {} response: {}", self.label, e))
        })?;

        let refined_text = json
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                AIError::NetworkError(format!(
                    "Unexpected {} response format: missing choices[0].message.content",
                    self.label
                ))
            })?;

        // Strip <think>...</think> blocks from reasoning models before sanitization
        let refined_text = strip_thinking_tags(&refined_text);
        let refined_text = sanitize_ollama_refinement_output(text, &refined_text, options);

        let input_tokens = json
            .pointer("/usage/prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let output_tokens = json
            .pointer("/usage/completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        Ok(RefinementResult {
            text: refined_text,
            usage: TokenUsage {
                input_tokens,
                output_tokens,
                total_cost_usd: 0.0,
            },
            provider: self.id().to_string(),
            model: model.to_string(),
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}

/// List models available on an OpenAI-compatible endpoint via GET /v1/models.
pub fn list_openai_compat_models(endpoint: &str, api_key: &str) -> Vec<String> {
    let normalized = normalize_ollama_endpoint(endpoint);
    let url = format!("{}/v1/models", normalized);
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(5))
        .build();
    let mut request = agent.get(&url).set("Content-Type", "application/json");
    if !api_key.is_empty() {
        request = request.set("Authorization", &format!("Bearer {}", api_key));
    }
    let Ok(resp) = request.call() else {
        return Vec::new();
    };
    let Ok(json) = resp.into_json::<serde_json::Value>() else {
        return Vec::new();
    };
    json.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    item.get("id")
                        .and_then(|id| id.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
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

/// Pull an Ollama model and stream progress via Tauri events.
/// Spawned in a background thread.
///
/// Events emitted:
/// - "ollama:pull-progress": OllamaPullProgress (every ~250ms during download)
/// - "ollama:pull-complete": OllamaPullComplete (on success)
/// - "ollama:pull-error": OllamaPullError (on failure)
pub fn pull_ollama_model_inner(app: AppHandle, model: String, endpoint: String) {
    use std::io::BufRead;

    let body = serde_json::json!({ "model": model, "stream": true });

    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(3600)) // 1 hour for large models
        .build();

    let mut last_connect_error: Option<String> = None;

    for candidate in ollama_endpoint_candidates(&endpoint) {
        let url = format!("{}/api/pull", candidate);
        let response = match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(body.clone())
        {
            Ok(r) => r,
            Err(ureq::Error::Transport(t)) => {
                last_connect_error = Some(t.to_string());
                continue;
            }
            Err(e) => {
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

        let reader = response.into_reader();
        let buffered = std::io::BufReader::new(reader);
        let mut last_emit = Instant::now();

        for line_result in buffered.lines() {
            let line = match line_result {
                Ok(l) if !l.trim().is_empty() => l,
                _ => continue,
            };

            let json: serde_json::Value = match serde_json::from_str(&line) {
                Ok(j) => j,
                Err(_) => continue,
            };

            let status = json["status"].as_str().unwrap_or("").to_string();

            // Check for success
            if status == "success" {
                let _ = app.emit(
                    "ollama:pull-complete",
                    OllamaPullComplete {
                        model: model.clone(),
                    },
                );
                return;
            }

            // Emit progress every 250ms
            if last_emit.elapsed() >= Duration::from_millis(250) {
                let _ = app.emit(
                    "ollama:pull-progress",
                    OllamaPullProgress {
                        model: model.clone(),
                        status: status.clone(),
                        digest: json["digest"].as_str().map(|s| s.to_string()),
                        total: json["total"].as_u64(),
                        completed: json["completed"].as_u64(),
                    },
                );
                last_emit = Instant::now();
            }

            // Check for error in response
            if let Some(err) = json["error"].as_str() {
                let _ = app.emit(
                    "ollama:pull-error",
                    OllamaPullError {
                        model: model.clone(),
                        error: err.to_string(),
                    },
                );
                return;
            }
        }

        // Stream ended without "success" status.
        let _ = app.emit(
            "ollama:pull-error",
            OllamaPullError {
                model: model.clone(),
                error: "Stream ended unexpectedly without success status".to_string(),
            },
        );
        return;
    }

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
        RefinementOptions {
            temperature: 0.3,
            max_tokens: 512,
            low_latency_mode: false,
            language: Some("en".to_string()),
            custom_prompt: None,
            enforce_language_guard,
        }
    }

    #[test]
    fn factory_creates_known_providers() {
        assert!(ProviderFactory::create("claude").is_ok());
        assert!(ProviderFactory::create("openai").is_ok());
        assert!(ProviderFactory::create("gemini").is_ok());
        assert!(ProviderFactory::create("ollama").is_ok());
    }

    #[test]
    fn factory_rejects_unknown_provider() {
        let err = ProviderFactory::create("other").err();
        assert!(matches!(err, Some(AIError::UnknownProvider(_))));
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
    fn strict_local_endpoint_accepts_localhost_and_loopback() {
        assert!(is_local_ollama_endpoint("http://localhost:11434"));
        assert!(is_local_ollama_endpoint("http://127.0.0.1:11434"));
        assert!(is_local_ollama_endpoint("http://localhost:11434/"));
    }

    #[test]
    fn strict_local_endpoint_rejects_remote_or_wrong_port() {
        assert!(!is_local_ollama_endpoint("http://192.168.1.20:11434"));
        assert!(!is_local_ollama_endpoint("http://localhost:8080"));
        assert!(!is_local_ollama_endpoint("https://localhost:11434"));
        assert!(!is_local_ollama_endpoint("http://localhost:11434/api"));
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
        let prompt =
            prompt_for_profile("custom", "en", Some("Custom prompt stays unchanged."), true);
        assert_eq!(prompt.as_deref(), Some("Custom prompt stays unchanged."));
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

    // --- OllamaProvider: validate_api_key is always Ok (no key needed) ---
    #[test]
    fn ollama_provider_validates_any_key() {
        let p = OllamaProvider::new();
        assert!(p.validate_api_key("").is_ok());
        assert!(p.validate_api_key("some-random-key").is_ok());
    }

    // --- OllamaProvider: cost is always zero ---
    #[test]
    fn ollama_provider_has_zero_cost() {
        let p = OllamaProvider::new();
        let cost = p.estimate_cost_usd("llama3.2:3b", 10_000, 10_000);
        assert_eq!(cost, 0.0, "Ollama should never charge");
    }

    // --- default_models_for_provider returns empty for ollama (dynamic) ---
    #[test]
    fn ollama_default_models_are_empty() {
        let models = default_models_for_provider("ollama");
        assert!(models.is_empty(), "Ollama models are discovered at runtime");
    }

    // --- cloud providers have non-empty default model lists ---
    #[test]
    fn cloud_providers_have_default_models() {
        assert!(!default_models_for_provider("claude").is_empty());
        assert!(!default_models_for_provider("openai").is_empty());
        assert!(!default_models_for_provider("gemini").is_empty());
    }

    // --- ProviderFactory::create_ollama passes custom endpoint ---
    #[test]
    fn create_ollama_with_custom_endpoint() {
        let provider = ProviderFactory::create_ollama("http://my-server:11434".to_string());
        assert_eq!(provider.id(), "ollama");
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
        let provider = OllamaProvider::with_endpoint("http://127.0.0.1:19999".to_string());
        let options = test_options(false);
        let result = provider.refine_transcript("hello world", "qwen3:14b", &options, "");
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
    fn language_guard_does_not_trip_on_mixed_language_input() {
        let original =
            "wir deployen den service and we monitor the logs im dashboard und in production";
        let refined = "we deploy the service and monitor the logs in the dashboard and production";
        let sanitized = sanitize_ollama_refinement_output(original, refined, &test_options(true));
        assert_eq!(sanitized, refined);
    }
}
