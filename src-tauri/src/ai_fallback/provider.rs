use super::error::AIError;
use super::models::{RefinementOptions, RefinementResult, TokenUsage};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tracing::warn;
use url::Url;

// Prompt templates optimized for local models (Ollama: qwen3, mistral-small).
// Guidelines: no translation, no explanations, preserve register and proper nouns.
pub const OLLAMA_PROMPT_EN: &str = "You are a transcript editor. Fix punctuation, capitalization, and obvious speech-to-text errors in the text below. Rules: do NOT translate; do NOT add explanations or commentary; preserve all proper nouns and technical terms exactly; preserve the original register (formal/informal). Output ONLY the corrected text with no preamble.";

pub const OLLAMA_PROMPT_DE: &str = "Du bist ein Transkript-Editor. Korrigiere Zeichensetzung, Groß-/Kleinschreibung und offensichtliche Sprache-zu-Text-Fehler im Text unten. Regeln: NICHT übersetzen; KEINE Erklärungen oder Kommentare hinzufügen; alle Eigennamen und Fachbegriffe exakt beibehalten; Anredeform (Du/Sie) aus dem Original beibehalten. Gib NUR den korrigierten Text aus, ohne Einleitung.";

const OLLAMA_PROMPT_SUMMARY_EN: &str = "Summarize this transcript into 3 to 6 concise bullet points. Preserve key facts, numbers, names, and decisions. Do not invent information. If something is uncertain, state it cautiously. Return only the bullet list.";

const OLLAMA_PROMPT_SUMMARY_DE: &str = "Fasse dieses Transkript in 3 bis 6 praegnanten Stichpunkten zusammen. Behalte wichtige Fakten, Zahlen, Namen und Entscheidungen bei. Keine Informationen erfinden. Unsichere Inhalte vorsichtig formulieren. Gib nur die Stichpunktliste zurueck.";

const OLLAMA_PROMPT_TECHNICAL_EN: &str = "Rewrite this transcript in technical specification style. Keep exact numbers, units, versions, APIs, constraints, and file paths. Structure output with short sections: Goal, Inputs, Outputs, Constraints, Open Questions. Do not invent missing values. Return only the structured result.";

const OLLAMA_PROMPT_TECHNICAL_DE: &str = "Formuliere dieses Transkript als technische Spezifikation um. Behalte exakte Zahlen, Einheiten, Versionen, APIs, Rahmenbedingungen und Dateipfade. Strukturiere die Ausgabe mit kurzen Abschnitten: Ziel, Eingaben, Ausgaben, Constraints, Offene Fragen. Keine fehlenden Werte erfinden. Gib nur das strukturierte Ergebnis zurueck.";

const OLLAMA_PROMPT_ACTION_ITEMS_EN: &str = "Convert this transcript into actionable tasks. Use bullets with format: [Action] [Owner?] [Due?] [Notes]. Preserve technical wording and constraints. If owner or due date is missing, mark it as unknown. Return only the action list.";

const OLLAMA_PROMPT_ACTION_ITEMS_DE: &str = "Wandle dieses Transkript in konkrete Aufgaben um. Nutze Stichpunkte im Format: [Aktion] [Owner?] [Faellig?] [Hinweise]. Behalte technische Begriffe und Rahmenbedingungen bei. Wenn Owner oder Datum fehlen, mit unknown markieren. Gib nur die Aufgabenliste zurueck.";

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
}

fn normalize_ollama_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "http://localhost:11434".to_string()
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

/// Return preferred endpoint plus a localhost/127.0.0.1 fallback variant.
pub fn ollama_endpoint_candidates(endpoint: &str) -> Vec<String> {
    let primary = normalize_ollama_endpoint(endpoint);
    let mut candidates = vec![primary.clone()];

    if let Ok(parsed) = Url::parse(&primary) {
        let alt_host = match parsed.host_str().map(|h| h.to_ascii_lowercase()) {
            Some(host) if host == "localhost" => Some("127.0.0.1"),
            Some(host) if host == "127.0.0.1" => Some("localhost"),
            _ => None,
        };

        if let Some(host) = alt_host {
            let mut alt = parsed;
            if alt.set_host(Some(host)).is_ok() {
                let alt_str = alt.to_string().trim_end_matches('/').to_string();
                if !candidates.iter().any(|c| c == &alt_str) {
                    candidates.push(alt_str);
                }
            }
        }
    }

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
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(5))
        .build();

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
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(5))
        .build();

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
pub fn ping_ollama_quick(endpoint: &str) -> Result<(), AIError> {
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
        _ => OLLAMA_PROMPT_EN,
    }
}

pub fn normalize_prompt_profile(profile: &str) -> &'static str {
    match profile.trim().to_lowercase().as_str() {
        "wording" => "wording",
        "summary" => "summary",
        "technical_specs" => "technical_specs",
        "action_items" => "action_items",
        "custom" => "custom",
        _ => "wording",
    }
}

pub fn prompt_for_profile(
    profile: &str,
    language: &str,
    custom_prompt: Option<&str>,
) -> Option<String> {
    match normalize_prompt_profile(profile) {
        "custom" => {
            let normalized = custom_prompt.unwrap_or("").trim();
            if normalized.is_empty() {
                None
            } else {
                Some(normalized.to_string())
            }
        }
        "summary" => Some(
            match language.trim().to_lowercase().as_str() {
                "de" | "german" => OLLAMA_PROMPT_SUMMARY_DE,
                _ => OLLAMA_PROMPT_SUMMARY_EN,
            }
            .to_string(),
        ),
        "technical_specs" => Some(
            match language.trim().to_lowercase().as_str() {
                "de" | "german" => OLLAMA_PROMPT_TECHNICAL_DE,
                _ => OLLAMA_PROMPT_TECHNICAL_EN,
            }
            .to_string(),
        ),
        "action_items" => Some(
            match language.trim().to_lowercase().as_str() {
                "de" | "german" => OLLAMA_PROMPT_ACTION_ITEMS_DE,
                _ => OLLAMA_PROMPT_ACTION_ITEMS_EN,
            }
            .to_string(),
        ),
        _ => Some(default_prompt_for_language(language).to_string()),
    }
}

fn normalize_provider(provider: &str) -> &str {
    match provider.trim().to_lowercase().as_str() {
        "claude" => "claude",
        "openai" => "openai",
        "gemini" => "gemini",
        "ollama" => "ollama",
        _ => "unknown",
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

fn adaptive_num_predict(input_text: &str, configured_max: u32, low_latency_mode: bool) -> u32 {
    let configured = configured_max.clamp(128, 8192);
    let input_tokens = rough_token_estimate(input_text);
    let heuristic = if low_latency_mode {
        ((input_tokens * 2) + 24).clamp(64, 384) as u32
    } else {
        ((input_tokens * 3) + 48).clamp(96, 1024) as u32
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
    payload.insert("temperature".to_string(), serde_json::json!(options.temperature));
    let num_predict = adaptive_num_predict(input_text, options.max_tokens, options.low_latency_mode);
    payload.insert("num_predict".to_string(), serde_json::json!(num_predict));
    let num_ctx = parse_env_usize("TRISPR_OLLAMA_NUM_CTX")
        .map(|n| n.clamp(1024, 8192))
        .unwrap_or_else(|| adaptive_num_ctx(input_text, system_prompt, options.low_latency_mode));
    payload.insert("num_ctx".to_string(), serde_json::json!(num_ctx));

    let num_thread = parse_env_usize("TRISPR_OLLAMA_NUM_THREAD").unwrap_or_else(default_ollama_num_thread);
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

fn suspicious_refinement_shape(original: &str, refined: &str) -> bool {
    let original_trimmed = original.trim();
    let refined_trimmed = refined.trim();
    if refined_trimmed.is_empty() {
        return true;
    }
    if original_trimmed.is_empty() {
        return false;
    }

    let original_chars = original_trimmed.chars().filter(|c| !c.is_whitespace()).count();
    let refined_chars = refined_trimmed.chars().filter(|c| !c.is_whitespace()).count();

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

fn sanitize_ollama_refinement_output(original: &str, refined: &str) -> String {
    let normalized = refined.replace("\r\n", "\n").replace('\r', "\n");
    let collapsed = collapse_excessive_blank_lines(normalized.trim());
    if suspicious_refinement_shape(original, &collapsed) {
        warn!(
            "Ignoring suspicious Ollama refinement output and keeping original transcript (orig_len={}, refined_len={})",
            original.len(),
            collapsed.len()
        );
        return original.to_string();
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
            endpoint: "http://localhost:11434".to_string(),
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

        // "think": false disables extended chain-of-thought mode on reasoning models
        // (e.g. qwen3, deepseek-r1). Without this, thinking models generate internal
        // reasoning tokens indefinitely before producing any output, causing apparent hangs.
        let ollama_options = build_ollama_options_payload(options, text, system_prompt);
        let keep_alive = std::env::var("TRISPR_OLLAMA_KEEP_ALIVE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "20m".to_string());

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

        // 45 s read timeout: with think:false, inference on 8B models takes 2-15 s.
        // Two candidates × 50 s total budget stays comfortably under the 90 s watchdog.
        // 5 s connect timeout: if Ollama is running, it should accept immediately.
        let read_timeout_secs = if options.low_latency_mode { 20 } else { 45 };
        let agent = ureq::builder()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(read_timeout_secs))
            .build();

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
                let refined_text = sanitize_ollama_refinement_output(text, &refined_text);
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
            let refined_text = sanitize_ollama_refinement_output(text, &refined_text);
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

/// Validate an Ollama model name against known safe characters.
/// Allowed: alphanumeric, ':' (for tags), '.' (for versions), '-', '_'
pub fn validate_ollama_model_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Model name cannot be empty".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '.' || c == '-' || c == '_')
    {
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
        assert_eq!(candidates[0], "http://localhost:11434");
        assert!(candidates.iter().any(|c| c == "http://127.0.0.1:11434"));
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

    // --- Prompt selection by language ---
    #[test]
    fn default_prompt_uses_english_for_unknown_language() {
        assert_eq!(default_prompt_for_language("en"), OLLAMA_PROMPT_EN);
        assert_eq!(default_prompt_for_language("fr"), OLLAMA_PROMPT_EN);
        assert_eq!(default_prompt_for_language(""), OLLAMA_PROMPT_EN);
    }

    #[test]
    fn default_prompt_uses_german_for_de() {
        assert_eq!(default_prompt_for_language("de"), OLLAMA_PROMPT_DE);
        assert_eq!(default_prompt_for_language("german"), OLLAMA_PROMPT_DE);
        assert_eq!(default_prompt_for_language("DE"), OLLAMA_PROMPT_DE);
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
        let options = RefinementOptions {
            temperature: 0.3,
            max_tokens: 512,
            low_latency_mode: false,
            language: Some("en".to_string()),
            custom_prompt: None,
        };
        let result = provider.refine_transcript("hello world", "qwen3:14b", &options, "");
        assert!(
            matches!(result, Err(AIError::OllamaNotRunning)),
            "connection refused should map to OllamaNotRunning, got: {:?}",
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
        let sanitized = sanitize_ollama_refinement_output(original, refined);
        assert_eq!(sanitized, original);
    }

    #[test]
    fn refinement_output_collapses_excessive_blank_lines() {
        let original = "First line\nSecond line\nThird line";
        let refined = "First line\n\n\n\nSecond line\n\n\nThird line";
        let sanitized = sanitize_ollama_refinement_output(original, refined);
        assert_eq!(sanitized, "First line\n\nSecond line\n\nThird line");
    }
}
