//! Safe model management for a local Ollama: "can the running runtime even run this model?",
//! "does a freshly pulled model actually work?", and updating or replacing models without
//! ever deleting the old one before the new one has proven itself.
//!
//! Layout: pure decision functions first (unit-tested without I/O), then the HTTP helpers,
//! then the flows (`*_impl`, tested against an in-process mock Ollama), then the thin Tauri
//! commands.

use super::commands::{delete_ollama_model_impl, unload_ollama_model_impl};
use super::model_registry::{
    fetch_model_requires, parse_model_ref, ManifestLookupError, ModelRegistry,
};
use super::provider::{
    extract_ollama_error_message, is_ssrf_target, ollama_endpoint_candidates,
    ollama_runner_defining_options, strip_thinking_tags, strip_transcript_wrapper_tags,
    validate_ollama_model_name, wrap_transcript_in_xml, OLLAMA_PROMPT_DE,
};
use super::update_and_persist_settings;
use crate::state::{normalize_ai_fallback_fields, AppState, Settings};
use serde::Serialize;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

/// Whole functional check, including a cold model load.
const VERIFY_TIME_LIMIT: Duration = Duration::from_secs(90);
const VERIFY_NUM_CTX: usize = 2048;
const VERIFY_TEMPERATURE: f64 = 0.15;
const VERIFY_NUM_PREDICT: u32 = 2048;
/// Share of the input words that must survive in the answer.
const MIN_WORD_RETENTION_PERCENT: usize = 90;
const SAMPLE_MAX_CHARS: usize = 200;
/// Backups created by `backup_ollama_model` carry this prefix; `finalize_ollama_model_update`
/// refuses to delete anything else.
const BACKUP_PREFIX: &str = "trispr-backup-";
const MAX_REQUIREMENT_LOOKUPS: usize = 24;

/// Small German raw transcripts (lower case, no punctuation) like the app feeds the refiner.
const VERIFY_SENTENCES: [&str; 3] = [
    "das wetter ist heute sehr schön und wir gehen später in den park",
    "ich habe gestern mit meinem kollegen über das neue projekt gesprochen",
    "bitte schick mir die unterlagen für den termin am donnerstag",
];

/// Openers of chatty answers ("Here is the corrected text: ..."). Matched on word boundaries
/// after stripping quotes/markdown.
const META_PREFIXES: [&str; 20] = [
    "hier ist",
    "hier sind",
    "hier der",
    "hier die",
    "hier das",
    "here is",
    "here's",
    "here are",
    "sure",
    "certainly",
    "of course",
    "natürlich",
    "gerne",
    "selbstverständlich",
    "okay",
    "der korrigierte",
    "korrigierter text",
    "korrigierte version",
    "the corrected",
    "corrected text",
];

// ───────────────────────────── pure decision functions ─────────────────────────────

/// Lower-cased words without surrounding punctuation.
pub(crate) fn normalized_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|raw| {
            raw.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// Opener of a chatty answer, if any (e.g. "hier ist").
pub(crate) fn meta_prefix(answer: &str) -> Option<&'static str> {
    let lowered = answer
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    META_PREFIXES.iter().copied().find(|phrase| {
        lowered.strip_prefix(phrase).is_some_and(|rest| {
            rest.chars()
                .next()
                .is_none_or(|next| !next.is_alphanumeric())
        })
    })
}

/// Judges one answer to one verification sentence. `Ok` carries the cleaned answer; `Err` the
/// reason it failed. No I/O.
pub(crate) fn evaluate_verify_output(input: &str, raw_answer: &str) -> Result<String, String> {
    // Judge what the app would paste: no thinking block, no echoed <transcript> wrapper.
    let cleaned = strip_transcript_wrapper_tags(&strip_thinking_tags(raw_answer))
        .trim()
        .to_string();
    if cleaned.contains("<think>") || cleaned.contains("</think>") {
        return Err("the answer contains an unterminated thinking block".to_string());
    }
    if cleaned.is_empty() {
        return Err(if raw_answer.trim().is_empty() {
            "the answer is empty".to_string()
        } else {
            "the answer contains only a thinking block".to_string()
        });
    }
    if let Some(prefix) = meta_prefix(&cleaned) {
        return Err(format!(
            "the answer starts with a commentary phrase (\"{}\")",
            prefix
        ));
    }

    let input_words = normalized_words(input);
    let answer_words = normalized_words(&cleaned);
    let answer_set: HashSet<&str> = answer_words.iter().map(String::as_str).collect();
    let kept = input_words
        .iter()
        .filter(|word| answer_set.contains(word.as_str()))
        .count();
    if kept * 100 < input_words.len() * MIN_WORD_RETENTION_PERCENT {
        return Err(format!(
            "only {} of {} input words were kept ({}%, need {}%)",
            kept,
            input_words.len(),
            kept * 100 / input_words.len().max(1),
            MIN_WORD_RETENTION_PERCENT
        ));
    }
    // Commentary after the text keeps every input word, so also bound the length.
    if answer_words.len() * 2 > input_words.len() * 3 + 4 {
        return Err(format!(
            "the answer has {} words for a {}-word input (extra text)",
            answer_words.len(),
            input_words.len()
        ));
    }
    Ok(cleaned)
}

/// `Some(runtime >= requires)` for two readable stable `x.y.z` versions, otherwise `None`.
/// A source build reports `0.0.0`; that says nothing, so it counts as unreadable.
pub(crate) fn runtime_compatibility(runtime: Option<&str>, requires: Option<&str>) -> Option<bool> {
    let runtime = crate::ollama_runtime::parse_stable_semver(runtime?)?;
    if runtime == (0, 0, 0) {
        return None;
    }
    let requires = crate::ollama_runtime::parse_stable_semver(requires?)?;
    Some(runtime >= requires)
}

fn requirement_detail(
    runtime: Option<&str>,
    requires: &str,
    compatible: Option<bool>,
) -> Option<String> {
    match (compatible, runtime) {
        (Some(true), _) => None,
        (Some(false), Some(runtime)) => Some(format!(
            "Needs Ollama {} or newer; running {}.",
            requires, runtime
        )),
        (Some(false), None) => Some(format!("Needs Ollama {} or newer.", requires)),
        (None, None) => Some("Ollama runtime version unknown (is it running?).".to_string()),
        (None, Some(runtime)) => Some(format!(
            "Could not compare versions (running {}, needs {}).",
            runtime, requires
        )),
    }
}

/// The refusal message for a pull the running runtime cannot handle; `None` when the pull may
/// go ahead (compatible, or not provable).
pub(crate) fn pull_block_reason(
    model: &str,
    requires: Option<&str>,
    runtime: Option<&str>,
) -> Option<String> {
    if runtime_compatibility(runtime, requires) != Some(false) {
        return None;
    }
    Some(format!(
        "Model '{}' needs Ollama >= {} (running {}). Update the runtime first.",
        model,
        requires.unwrap_or("?"),
        runtime.unwrap_or("?")
    ))
}

/// Lower-cased, with the implicit `:latest` made explicit.
pub(crate) fn normalize_model_name(name: &str) -> String {
    let lowered = name.trim().to_lowercase();
    let tag_start = lowered.rfind('/').map_or(0, |idx| idx + 1);
    if lowered[tag_start..].contains(':') {
        lowered
    } else {
        format!("{}:latest", lowered)
    }
}

pub(crate) fn model_names_equal(a: &str, b: &str) -> bool {
    normalize_model_name(a) == normalize_model_name(b)
}

fn contains_model(installed: &[String], name: &str) -> bool {
    installed
        .iter()
        .any(|candidate| model_names_equal(candidate, name))
}

/// Name for the `/api/copy` backup of `model`: lower case, valid for `validate_ollama_model_name`.
pub(crate) fn backup_name_for(model: &str, unix_ms: u64) -> String {
    let slug: String = model
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(100)
        .collect();
    format!("{}{}:{}", BACKUP_PREFIX, slug, unix_ms)
}

fn is_backup_name(name: &str) -> bool {
    name.trim().to_lowercase().starts_with(BACKUP_PREFIX)
}

/// True when `backup` is a backup of exactly `model` (name derived by `backup_name_for`).
/// Stops a UI mix-up from restoring or deleting another model's backup.
fn is_backup_of(model: &str, backup: &str) -> bool {
    let probe = backup_name_for(model, 0);
    // "<prefix><slug>:0" -> "<prefix><slug>:"
    let expected_prefix = &probe[..probe.len() - 1];
    backup.trim().to_lowercase().starts_with(expected_prefix)
}

/// Settings fields that name the active Ollama model.
fn settings_reference_model(settings: &Settings, model: &str) -> bool {
    [
        settings.ai_fallback.model.as_str(),
        settings.providers.ollama.preferred_model.as_str(),
        settings.postproc_llm_model.as_str(),
    ]
    .iter()
    .any(|slot| !slot.trim().is_empty() && model_names_equal(slot, model))
}

/// Points `ai_fallback.model`, `providers.ollama.preferred_model` and `postproc_llm_model` at
/// `new`, but only those that currently name `old`. Returns whether anything changed.
pub(crate) fn switch_model_in_settings(settings: &mut Settings, old: &str, new: &str) -> bool {
    let mut changed = false;
    for slot in [
        &mut settings.ai_fallback.model,
        &mut settings.providers.ollama.preferred_model,
        &mut settings.postproc_llm_model,
    ] {
        if !slot.trim().is_empty() && model_names_equal(slot, old) {
            *slot = new.to_string();
            changed = true;
        }
    }
    changed
}

fn median(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn truncate_chars(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

fn format_limit(limit: Duration) -> String {
    if limit.as_millis() >= 1000 {
        format!("{} s", limit.as_secs())
    } else {
        format!("{} ms", limit.as_millis())
    }
}

/// Request body of the check: the app's refinement request form (`/api/chat`, no streaming,
/// no thinking, model kept loaded) with the German system prompt.
pub(crate) fn verify_request_body(model: &str, sentence: &str) -> serde_json::Value {
    let mut options = ollama_runner_defining_options(VERIFY_NUM_CTX);
    options.insert(
        "temperature".to_string(),
        serde_json::json!(VERIFY_TEMPERATURE),
    );
    options.insert(
        "num_predict".to_string(),
        serde_json::json!(VERIFY_NUM_PREDICT),
    );
    serde_json::json!({
        "model": model,
        "stream": false,
        "think": false,
        "messages": [
            { "role": "system", "content": OLLAMA_PROMPT_DE },
            { "role": "user", "content": wrap_transcript_in_xml(sentence) }
        ],
        "options": serde_json::Value::Object(options),
        "keep_alive": -1
    })
}

// ───────────────────────────────── HTTP helpers ─────────────────────────────────

#[derive(Debug)]
enum CallError {
    Status { code: u16, detail: String },
    TimedOut,
    Transport(String),
}

fn is_timeout_message(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("timed out") || lowered.contains("timeout") || lowered.contains("10060")
}

/// One request against the configured Ollama (localhost/127.0.0.1 variants are tried on
/// connection errors). `timeout` bounds the whole request. Returns the response body.
fn ollama_request(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
    timeout: Duration,
) -> Result<String, CallError> {
    if is_ssrf_target(endpoint) {
        return Err(CallError::Transport(
            "the Ollama endpoint is not allowed".to_string(),
        ));
    }
    let timeout = timeout.max(Duration::from_millis(1));
    let started = Instant::now();
    // Only the whole-request budget running out is "timed out"; a connect timeout that fires
    // earlier is a connection problem and may be retried on the next candidate.
    let budget_spent = || started.elapsed() + Duration::from_millis(300) >= timeout;
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(5).min(timeout))
        .timeout(timeout)
        .build();
    let mut last_error: Option<String> = None;
    for candidate in ollama_endpoint_candidates(endpoint) {
        let request = agent.request(method, &format!("{}{}", candidate, path));
        let result = match body {
            Some(body) => request
                .set("Content-Type", "application/json")
                .send_json(body),
            None => request.call(),
        };
        match result {
            Ok(response) => {
                return response.into_string().map_err(|error| {
                    let message = error.to_string();
                    if is_timeout_message(&message) && budget_spent() {
                        CallError::TimedOut
                    } else {
                        CallError::Transport(message)
                    }
                })
            }
            Err(ureq::Error::Status(code, response)) => {
                return Err(CallError::Status {
                    code,
                    detail: extract_ollama_error_message(response),
                })
            }
            Err(ureq::Error::Transport(transport)) => {
                let message = transport.to_string();
                // A retry on the next candidate would only repeat a long wait.
                if is_timeout_message(&message) && budget_spent() {
                    return Err(CallError::TimedOut);
                }
                last_error = Some(message);
            }
        }
    }
    Err(CallError::Transport(last_error.unwrap_or_else(|| {
        "unable to reach Ollama endpoint".to_string()
    })))
}

fn describe_call_error(error: &CallError) -> String {
    match error {
        CallError::Status { code, detail } if detail.is_empty() => {
            format!("Ollama returned HTTP {}", code)
        }
        CallError::Status { code, detail } => format!("Ollama returned HTTP {}: {}", code, detail),
        CallError::TimedOut => "the request timed out".to_string(),
        CallError::Transport(message) => format!("could not reach Ollama: {}", message),
    }
}

fn list_installed(endpoint: &str) -> Result<Vec<String>, String> {
    let body = ollama_request(endpoint, "GET", "/api/tags", None, Duration::from_secs(10))
        .map_err(|error| {
            format!(
                "Could not list installed models: {}",
                describe_call_error(&error)
            )
        })?;
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("Could not read the installed model list: {}", error))?;
    Ok(json["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

fn copy_model(endpoint: &str, source: &str, destination: &str) -> Result<(), String> {
    let body = serde_json::json!({ "source": source, "destination": destination });
    ollama_request(
        endpoint,
        "POST",
        "/api/copy",
        Some(&body),
        Duration::from_secs(60),
    )
    .map(|_| ())
    .map_err(|error| describe_call_error(&error))
}

pub(crate) fn parse_runtime_version(body: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    let version = json["version"].as_str()?.trim().trim_start_matches('v');
    (!version.is_empty()).then(|| version.to_string())
}

fn fetch_runtime_version(endpoint: &str) -> Option<String> {
    ollama_request(
        endpoint,
        "GET",
        "/api/version",
        None,
        Duration::from_secs(3),
    )
    .ok()
    .and_then(|body| parse_runtime_version(&body))
}

// ─────────────────────────── requirements (runtime vs model) ───────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelRequirement {
    pub name: String,
    pub requires: Option<String>,
    pub runtime_version: Option<String>,
    pub compatible: Option<bool>,
    pub detail: Option<String>,
}

fn requirement_for(name: &str, runtime: Option<&str>) -> ModelRequirement {
    let entry = |requires: Option<String>, compatible: Option<bool>, detail: Option<String>| {
        ModelRequirement {
            name: name.to_string(),
            requires,
            runtime_version: runtime.map(str::to_string),
            compatible,
            detail,
        }
    };
    let model = match parse_model_ref(name) {
        Ok(model) => model,
        Err(error) => return entry(None, None, Some(error)),
    };
    if model.registry == ModelRegistry::HuggingFace {
        return entry(
            None,
            None,
            Some("Hugging Face models declare no minimum Ollama version.".to_string()),
        );
    }
    match fetch_model_requires(&model) {
        Ok(Some(requires)) => {
            let compatible = runtime_compatibility(runtime, Some(&requires));
            let detail = requirement_detail(runtime, &requires, compatible);
            entry(Some(requires), compatible, detail)
        }
        Ok(None) => entry(
            None,
            None,
            Some("No minimum Ollama version declared.".to_string()),
        ),
        Err(ManifestLookupError::NotFound) => entry(
            None,
            None,
            Some("Not found in the Ollama registry.".to_string()),
        ),
        Err(ManifestLookupError::Failed(reason)) => entry(
            None,
            None,
            Some(format!("Registry lookup failed: {}", reason)),
        ),
    }
}

/// Looks every model up in parallel (two small registry requests each).
pub(crate) fn check_requirements_for_runtime(
    runtime: Option<String>,
    names: Vec<String>,
) -> Vec<ModelRequirement> {
    let names: Vec<String> = names.into_iter().take(MAX_REQUIREMENT_LOOKUPS).collect();
    let runtime = runtime.as_deref();
    std::thread::scope(|scope| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| scope.spawn(move || requirement_for(name, runtime)))
            .collect();
        handles
            .into_iter()
            .zip(&names)
            .map(|(handle, name)| {
                handle.join().unwrap_or_else(|_| ModelRequirement {
                    name: name.clone(),
                    requires: None,
                    runtime_version: runtime.map(str::to_string),
                    compatible: None,
                    detail: Some("Registry lookup failed unexpectedly.".to_string()),
                })
            })
            .collect()
    })
}

/// Pull precheck: refuses a model the running runtime provably cannot run. Unknown (runtime
/// unreachable, registry unreachable, unreadable versions) never blocks.
pub(crate) fn ensure_runtime_supports_model(endpoint: &str, model: &str) -> Result<(), String> {
    let Some(runtime) = fetch_runtime_version(endpoint) else {
        return Ok(());
    };
    let requirement = requirement_for(model, Some(&runtime));
    match pull_block_reason(model, requirement.requires.as_deref(), Some(&runtime)) {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

// ───────────────────────────────── verification ─────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VerifyResult {
    pub ok: bool,
    /// Median wall time of the requests (the first one includes the cold load).
    pub latency_ms: u64,
    /// `load_duration` Ollama reported for the first request.
    pub load_ms: u64,
    /// The first answer when passed, the offending answer when failed.
    pub sample: Option<String>,
    pub detail: Option<String>,
    /// The check could not be completed (time limit, transport fault, HTTP 5xx, unreadable
    /// answer): no verdict on the model. Internal; not part of the command's payload.
    #[serde(skip)]
    pub(crate) inconclusive: bool,
}

/// A refusal of the server (HTTP 4xx: model missing, runtime too old, bad request) is an
/// answer about the model. No answer at all, or a server fault (5xx), is not.
fn call_error_is_inconclusive(error: &CallError) -> bool {
    match error {
        CallError::Status { code, .. } => *code >= 500,
        CallError::TimedOut | CallError::Transport(_) => true,
    }
}

fn failed_verification(
    durations: &[u64],
    load_ms: u64,
    sample: Option<String>,
    detail: String,
) -> VerifyResult {
    VerifyResult {
        ok: false,
        latency_ms: median(durations),
        load_ms,
        sample,
        detail: Some(detail),
        inconclusive: false,
    }
}

/// The check could not be completed: no verdict on the model either way.
fn inconclusive_verification(durations: &[u64], load_ms: u64, detail: String) -> VerifyResult {
    VerifyResult {
        inconclusive: true,
        ..failed_verification(durations, load_ms, None, detail)
    }
}

/// Runs the three sentences through the model; stops at the first failure.
pub(crate) fn run_model_verification(
    endpoint: &str,
    model: &str,
    time_limit: Duration,
) -> VerifyResult {
    let started = Instant::now();
    let mut durations: Vec<u64> = Vec::new();
    let mut load_ms = 0u64;
    let mut first_sample: Option<String> = None;

    for (index, sentence) in VERIFY_SENTENCES.iter().enumerate() {
        let number = index + 1;
        let remaining = time_limit.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return inconclusive_verification(
                &durations,
                load_ms,
                format!("Time limit of {} exceeded.", format_limit(time_limit)),
            );
        }
        let body = verify_request_body(model, sentence);
        let request_started = Instant::now();
        let response = ollama_request(endpoint, "POST", "/api/chat", Some(&body), remaining);
        let elapsed_ms = request_started.elapsed().as_millis() as u64;
        let raw = match response {
            Ok(raw) => raw,
            Err(CallError::TimedOut) => {
                return inconclusive_verification(
                    &durations,
                    load_ms,
                    format!(
                        "Time limit of {} exceeded (sentence {}).",
                        format_limit(time_limit),
                        number
                    ),
                )
            }
            Err(error) => {
                let detail = format!("Sentence {}: {}.", number, describe_call_error(&error));
                return if call_error_is_inconclusive(&error) {
                    inconclusive_verification(&durations, load_ms, detail)
                } else {
                    failed_verification(&durations, load_ms, None, detail)
                };
            }
        };
        durations.push(elapsed_ms);
        let json: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(json) => json,
            Err(error) => {
                return inconclusive_verification(
                    &durations,
                    load_ms,
                    format!("Sentence {}: unreadable response ({}).", number, error),
                )
            }
        };
        if index == 0 {
            load_ms = json["load_duration"]
                .as_u64()
                .map_or(0, |ns| ns / 1_000_000);
        }
        let answer = json["message"]["content"]
            .as_str()
            .or_else(|| json["response"].as_str())
            .unwrap_or("");
        match evaluate_verify_output(sentence, answer) {
            Ok(cleaned) => {
                if index == 0 {
                    first_sample = Some(truncate_chars(&cleaned, SAMPLE_MAX_CHARS));
                }
            }
            Err(reason) => {
                return failed_verification(
                    &durations,
                    load_ms,
                    Some(truncate_chars(answer, SAMPLE_MAX_CHARS)).filter(|s| !s.is_empty()),
                    format!("Sentence {}: {}.", number, reason),
                )
            }
        }
    }

    VerifyResult {
        ok: true,
        latency_ms: median(&durations),
        load_ms,
        sample: first_sample,
        detail: None,
        inconclusive: false,
    }
}

// ─────────────────────────────── backup / finalize / replace ───────────────────────────────

pub(crate) fn backup_model_impl(
    endpoint: &str,
    model: &str,
    unix_ms: u64,
) -> Result<String, String> {
    let installed = list_installed(endpoint)?;
    if !contains_model(&installed, model) {
        return Err(format!("Model '{}' is not installed.", model));
    }
    let backup = backup_name_for(model, unix_ms);
    if contains_model(&installed, &backup) {
        return Err(format!("Backup '{}' already exists.", backup));
    }
    copy_model(endpoint, model, &backup)
        .map_err(|error| format!("Could not back up '{}': {}", model, error))?;
    Ok(backup)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FinalizeResult {
    pub verified: bool,
    pub restored: bool,
    /// The check could not be completed (time limit, transport fault, HTTP 5xx, unreadable
    /// answer): there is no verdict on the new build. Nothing was changed, the new tag AND the
    /// backup are both still there; the caller may run the check again or roll back. Only a
    /// build that really fails the criteria is restored automatically.
    pub inconclusive: bool,
    pub detail: Option<String>,
}

/// Checks everything that has to hold before a backup may be restored or deleted: it is a
/// Trispr Flow backup, of exactly this model, and it exists. Touches nothing.
fn check_backup_pair(endpoint: &str, model: &str, backup: &str) -> Result<(), String> {
    if !is_backup_name(backup) {
        return Err(format!("'{}' is not a Trispr Flow backup name.", backup));
    }
    if !is_backup_of(model, backup) {
        return Err(format!(
            "'{}' is not a backup of '{}'; nothing was changed.",
            backup, model
        ));
    }
    if model_names_equal(model, backup) {
        return Err("The model and its backup must differ.".to_string());
    }
    // Without a backup there is no way back: refuse before touching anything.
    let installed = list_installed(endpoint)?;
    if !contains_model(&installed, backup) {
        return Err(format!(
            "Backup '{}' not found; nothing was changed.",
            backup
        ));
    }
    Ok(())
}

/// How putting a backup back went.
enum Restore {
    Done,
    /// Restored, but the backup could not be deleted afterwards.
    DoneBackupKept(String),
    /// The new build could not be removed, so nothing was restored; the backup is untouched.
    NewBuildStays(String),
    /// The new build is gone but the copy back failed; the backup is the only copy left.
    CopyFailed(String),
}

/// Removes the new build and copies the backup back, in this order, deleting the backup last:
/// at every step the old version still exists somewhere.
fn restore_backup(endpoint: &str, model: &str, backup: &str) -> Restore {
    // delete_ollama_model_impl unloads the model first.
    match delete_ollama_model_impl(endpoint.to_string(), model.to_string()) {
        Ok(()) => {}
        Err(error) if error.contains("not found in Ollama") => {}
        Err(error) => return Restore::NewBuildStays(error),
    }
    if let Err(error) = copy_model(endpoint, backup, model) {
        return Restore::CopyFailed(error);
    }
    match delete_ollama_model_impl(endpoint.to_string(), backup.to_string()) {
        Ok(()) => Restore::Done,
        Err(error) => Restore::DoneBackupKept(error),
    }
}

/// After a pull over an existing tag: verify the new build. Only a passing build lets the
/// backup go; a build that fails the criteria is removed and the backup copied back. A check
/// that cannot be completed (`inconclusive`) changes nothing at all.
pub(crate) fn finalize_model_update_impl(
    endpoint: &str,
    model: &str,
    backup: &str,
    time_limit: Duration,
) -> Result<FinalizeResult, String> {
    check_backup_pair(endpoint, model, backup)?;

    let verification = run_model_verification(endpoint, model, time_limit);
    if verification.ok {
        return Ok(
            match delete_ollama_model_impl(endpoint.to_string(), backup.to_string()) {
                Ok(()) => FinalizeResult {
                    verified: true,
                    restored: false,
                    inconclusive: false,
                    detail: None,
                },
                Err(error) => FinalizeResult {
                    verified: true,
                    restored: false,
                    inconclusive: false,
                    detail: Some(format!(
                        "Verified, but the backup '{}' could not be deleted: {}",
                        backup, error
                    )),
                },
            },
        );
    }

    let inconclusive = verification.inconclusive;
    let reason = verification
        .detail
        .unwrap_or_else(|| "Verification failed.".to_string());
    if inconclusive {
        return Ok(FinalizeResult {
            verified: false,
            restored: false,
            inconclusive: true,
            detail: Some(format!(
                "{} The check did not reach a verdict, so nothing was changed: '{}' and the backup '{}' are both kept. Run the check again or roll back.",
                reason, model, backup
            )),
        });
    }
    let stuck = |what: &str, error: &str| FinalizeResult {
        verified: false,
        restored: false,
        inconclusive: false,
        detail: Some(format!(
            "{} {} ({}); the previous version is still available as '{}'.",
            reason, what, error, backup
        )),
    };
    Ok(match restore_backup(endpoint, model, backup) {
        Restore::NewBuildStays(error) => stuck("The new build could not be removed", &error),
        Restore::CopyFailed(error) => stuck("The previous version could not be restored", &error),
        Restore::Done => FinalizeResult {
            verified: false,
            restored: true,
            inconclusive: false,
            detail: Some(format!("{} The previous version was restored.", reason)),
        },
        Restore::DoneBackupKept(error) => FinalizeResult {
            verified: false,
            restored: true,
            inconclusive: false,
            detail: Some(format!(
                "{} The previous version was restored; the backup '{}' could not be deleted: {}",
                reason, backup, error
            )),
        },
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RollbackResult {
    pub restored: bool,
    pub detail: Option<String>,
}

/// Puts the backup back over the new tag (for an update whose check was inconclusive, or that
/// the user does not want): same order and same guarantees as the restore in `finalize`.
pub(crate) fn rollback_model_update_impl(
    endpoint: &str,
    model: &str,
    backup: &str,
) -> Result<RollbackResult, String> {
    check_backup_pair(endpoint, model, backup)?;
    Ok(match restore_backup(endpoint, model, backup) {
        Restore::Done => RollbackResult {
            restored: true,
            detail: None,
        },
        Restore::DoneBackupKept(error) => RollbackResult {
            restored: true,
            detail: Some(format!(
                "The previous version was restored; the backup '{}' could not be deleted: {}",
                backup, error
            )),
        },
        Restore::NewBuildStays(error) => RollbackResult {
            restored: false,
            detail: Some(format!(
                "The new build could not be removed ({}); the previous version is still available as '{}'.",
                error, backup
            )),
        },
        Restore::CopyFailed(error) => RollbackResult {
            restored: false,
            detail: Some(format!(
                "The previous version could not be restored ({}); it is still available as '{}'.",
                error, backup
            )),
        },
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReplaceResult {
    pub verified: bool,
    pub switched_active: bool,
    pub deleted_old: bool,
    pub latency_ms: u64,
    pub detail: Option<String>,
}

/// Replaces `old` by the already installed `new`. `switch_settings(old, new)` repoints the
/// settings that named `old` and says whether any did; it runs only after `new` passed.
pub(crate) fn replace_model_impl(
    endpoint: &str,
    old: &str,
    new: &str,
    switch_settings: &mut dyn FnMut(&str, &str) -> Result<bool, String>,
    time_limit: Duration,
) -> Result<ReplaceResult, String> {
    if old.trim().is_empty() || new.trim().is_empty() {
        return Err("Both model names are required.".to_string());
    }
    if model_names_equal(old, new) {
        return Err("The old and the new model are the same model.".to_string());
    }
    let installed = list_installed(endpoint)?;
    if !contains_model(&installed, new) {
        return Err(format!("Model '{}' is not installed.", new));
    }

    let verification = run_model_verification(endpoint, new, time_limit);
    if !verification.ok {
        // Settings and the old model stay as they are. Only release what the check loaded.
        let _ = unload_ollama_model_impl(endpoint, new);
        return Ok(ReplaceResult {
            verified: false,
            switched_active: false,
            deleted_old: false,
            latency_ms: verification.latency_ms,
            detail: verification.detail,
        });
    }

    let switched_active = match switch_settings(old, new) {
        Ok(switched) => switched,
        Err(error) => {
            return Ok(ReplaceResult {
                verified: true,
                switched_active: false,
                deleted_old: false,
                latency_ms: verification.latency_ms,
                detail: Some(format!(
                    "Verified, but the settings could not be updated, so '{}' was kept: {}",
                    old, error
                )),
            })
        }
    };
    // delete_ollama_model_impl unloads the old model first.
    let (deleted_old, detail) =
        match delete_ollama_model_impl(endpoint.to_string(), old.to_string()) {
            Ok(()) => (true, None),
            Err(error) => (
                false,
                Some(format!(
                    "Verified, but '{}' could not be deleted: {}",
                    old, error
                )),
            ),
        };
    Ok(ReplaceResult {
        verified: true,
        switched_active,
        deleted_old,
        latency_ms: verification.latency_ms,
        detail,
    })
}

// ───────────────────────────────── Tauri commands ─────────────────────────────────

fn local_endpoint(state: &AppState) -> Result<String, String> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(settings.providers.ollama.endpoint.clone())
}

fn pull_in_progress(state: &AppState, model: &str) -> bool {
    state
        .ollama_pulls
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .any(|pulling| model_names_equal(pulling, model))
}

/// Repoints the persisted settings from `old` to `new` (only the fields that named `old`).
fn switch_model_in_app(app: &AppHandle, old: &str, new: &str) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let referenced = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        settings_reference_model(&settings, old)
    };
    if !referenced {
        return Ok(false);
    }
    let mut switched = false;
    update_and_persist_settings(app, state.inner(), |settings| {
        switched = switch_model_in_settings(settings, old, new);
        if switched {
            normalize_ai_fallback_fields(settings);
        }
        Ok(())
    })?;
    Ok(switched)
}

/// Minimum Ollama version per model (from the registry config blob) next to the version of
/// the running runtime.
#[tauri::command]
pub(crate) async fn check_ollama_model_requirements(
    state: State<'_, AppState>,
    models: Vec<String>,
) -> Result<Vec<ModelRequirement>, String> {
    let endpoint = local_endpoint(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = fetch_runtime_version(&endpoint);
        check_requirements_for_runtime(runtime, models)
    })
    .await
    .map_err(|e| format!("Model requirement check failed: {}", e))
}

/// Functional check of an installed model with the app's own request form.
#[tauri::command]
pub(crate) async fn verify_ollama_model(
    state: State<'_, AppState>,
    model: String,
) -> Result<VerifyResult, String> {
    validate_ollama_model_name(&model)?;
    let endpoint = local_endpoint(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        run_model_verification(&endpoint, &model, VERIFY_TIME_LIMIT)
    })
    .await
    .map_err(|e| format!("Model verification failed: {}", e))
}

/// `/api/copy` of an installed model to a backup name (same blobs, no extra disk).
#[tauri::command]
pub(crate) async fn backup_ollama_model(
    state: State<'_, AppState>,
    model: String,
) -> Result<String, String> {
    validate_ollama_model_name(&model)?;
    let endpoint = local_endpoint(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        backup_model_impl(&endpoint, &model, crate::util::now_ms())
    })
    .await
    .map_err(|e| format!("Model backup failed: {}", e))?
}

#[tauri::command]
pub(crate) async fn finalize_ollama_model_update(
    state: State<'_, AppState>,
    model: String,
    backup: String,
) -> Result<FinalizeResult, String> {
    validate_ollama_model_name(&model)?;
    validate_ollama_model_name(&backup)?;
    let endpoint = local_endpoint(&state)?;
    if pull_in_progress(&state, &model) {
        return Err(format!("A pull of '{}' is still running.", model));
    }
    tauri::async_runtime::spawn_blocking(move || {
        finalize_model_update_impl(&endpoint, &model, &backup, VERIFY_TIME_LIMIT)
    })
    .await
    .map_err(|e| format!("Model update finalization failed: {}", e))?
}

/// Takes an update back: removes the new build and restores the backup. For an update whose
/// `finalize_ollama_model_update` was `inconclusive`, or that the user does not want.
#[tauri::command]
pub(crate) async fn rollback_ollama_model_update(
    state: State<'_, AppState>,
    model: String,
    backup: String,
) -> Result<RollbackResult, String> {
    validate_ollama_model_name(&model)?;
    validate_ollama_model_name(&backup)?;
    let endpoint = local_endpoint(&state)?;
    if pull_in_progress(&state, &model) {
        return Err(format!("A pull of '{}' is still running.", model));
    }
    tauri::async_runtime::spawn_blocking(move || {
        rollback_model_update_impl(&endpoint, &model, &backup)
    })
    .await
    .map_err(|e| format!("Model rollback failed: {}", e))?
}

#[tauri::command]
pub(crate) async fn replace_ollama_model(
    app: AppHandle,
    state: State<'_, AppState>,
    old_model: String,
    new_model: String,
) -> Result<ReplaceResult, String> {
    validate_ollama_model_name(&old_model)?;
    validate_ollama_model_name(&new_model)?;
    let endpoint = local_endpoint(&state)?;
    if pull_in_progress(&state, &new_model) {
        return Err(format!("A pull of '{}' is still running.", new_model));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut switch = |old: &str, new: &str| switch_model_in_app(&app, old, new);
        replace_model_impl(
            &endpoint,
            &old_model,
            &new_model,
            &mut switch,
            VERIFY_TIME_LIMIT,
        )
    })
    .await
    .map_err(|e| format!("Model replacement failed: {}", e))?
}

// ─────────────────────────────────────── tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_only_belongs_to_the_model_it_was_made_from() {
        let backup = backup_name_for("gemma4:e4b", 42);
        assert!(is_backup_of("gemma4:e4b", &backup));
        assert!(
            is_backup_of("GEMMA4:E4B", &backup),
            "case-insensitive like Ollama names"
        );
        assert!(!is_backup_of("llama3:8b", &backup));
        // a longer model name must not claim the backup of its prefix
        assert!(!is_backup_of("gemma4:e4b-it-q4_K_M", &backup));
    }

    #[test]
    fn finalize_refuses_another_models_backup_before_touching_anything() {
        // Port 1 is closed: reaching the network would fail differently than this guard does.
        let error = finalize_model_update_impl(
            "http://127.0.0.1:1",
            "gemma4:e4b",
            &backup_name_for("llama3:8b", 42),
            Duration::from_secs(1),
        )
        .expect_err("a backup of another model must be refused");
        assert!(error.contains("is not a backup of"), "got: {}", error);
    }
    use crate::ai_fallback::test_support::{MockReply, MockRequest, MockServer};
    use std::sync::{Arc, Mutex};

    const S1: &str = VERIFY_SENTENCES[0];
    const GOOD_S1: &str = "Das Wetter ist heute sehr schön und wir gehen später in den Park.";

    // ── pure criteria of verify_ollama_model ─────────────────────────────────────

    #[test]
    fn criteria_reject_an_empty_answer() {
        assert!(evaluate_verify_output(S1, "")
            .unwrap_err()
            .contains("empty"));
        assert!(evaluate_verify_output(S1, " \n\t ")
            .unwrap_err()
            .contains("empty"));
    }

    #[test]
    fn criteria_strip_thinking_tags_before_judging() {
        let with_think = format!(
            "<think>\nDie Zeichensetzung fehlt.\n</think>\n\n{}",
            GOOD_S1
        );
        assert_eq!(evaluate_verify_output(S1, &with_think).unwrap(), GOOD_S1);

        // Qwen-style empty think block in front of the answer
        let empty_think = format!("<think>\n\n</think>\n\n{}", GOOD_S1);
        assert!(evaluate_verify_output(S1, &empty_think).is_ok());

        // the echoed <transcript> wrapper (lfm3, bonsai) is removed like the app removes it
        let echoed = format!(
            "<transcript>
{}
</transcript>",
            GOOD_S1
        );
        assert_eq!(evaluate_verify_output(S1, &echoed).unwrap(), GOOD_S1);
        let think_and_echo = format!(
            "<think>x</think><transcript>
{}
</transcript>",
            GOOD_S1
        );
        assert_eq!(
            evaluate_verify_output(S1, &think_and_echo).unwrap(),
            GOOD_S1
        );
        // a wrapper around commentary is still commentary
        let echoed_chatty = format!(
            "<transcript>
Hier ist der Text: {}
</transcript>",
            GOOD_S1
        );
        assert!(evaluate_verify_output(S1, &echoed_chatty)
            .unwrap_err()
            .contains("commentary"));

        // nothing but a thinking block is not an answer
        let only_think = "<think>ich überlege</think>";
        assert!(evaluate_verify_output(S1, only_think)
            .unwrap_err()
            .contains("only a thinking block"));

        // an unterminated block must not count as (partially matching) text
        let unterminated = format!("<think>{}", GOOD_S1);
        assert!(evaluate_verify_output(S1, &unterminated)
            .unwrap_err()
            .contains("unterminated"));
    }

    #[test]
    fn criteria_reject_a_meta_commentary_prefix() {
        for chatty in [
            format!("Hier ist der korrigierte Text: {}", GOOD_S1),
            format!("Sure! Here is the corrected text: {}", GOOD_S1),
            format!("**Natürlich** {}", GOOD_S1),
            format!("\"Gerne: {}\"", GOOD_S1),
        ] {
            let error = evaluate_verify_output(S1, &chatty).unwrap_err();
            assert!(error.contains("commentary"), "{}: {}", chatty, error);
        }
        // word boundary: a word that merely starts like a phrase is not commentary
        assert_eq!(meta_prefix("Surely not"), None);
        assert_eq!(meta_prefix("Sure thing"), Some("sure"));
        assert_eq!(meta_prefix("Das Wetter ist schön"), None);
    }

    #[test]
    fn criteria_reject_too_little_word_retention() {
        // 13 input words, need 90% -> 12 must survive
        let one_missing = "Das Wetter ist heute sehr schön und wir gehen später in den Garten.";
        assert!(evaluate_verify_output(S1, one_missing).is_ok());
        let two_missing = "Das Wetter ist heute sehr schön und wir gehen dann in den Garten.";
        let error = evaluate_verify_output(S1, two_missing).unwrap_err();
        assert!(error.contains("only 11 of 13"), "got: {}", error);
        // a translation keeps (almost) nothing
        let english = "The weather is very nice today and we will go to the park later.";
        assert!(evaluate_verify_output(S1, english).is_err());
    }

    #[test]
    fn criteria_accept_a_clean_correction_regardless_of_case_and_punctuation() {
        assert_eq!(evaluate_verify_output(S1, GOOD_S1).unwrap(), GOOD_S1);
        let shouted = "DAS WETTER, ist heute sehr schön - und wir gehen später in den Park!";
        assert!(evaluate_verify_output(S1, shouted).is_ok());
        assert!(
            evaluate_verify_output(S1, S1).is_ok(),
            "an unchanged echo still works"
        );
    }

    #[test]
    fn criteria_reject_text_added_after_the_answer() {
        let rambling = format!(
            "{} Ich hoffe, das hilft dir weiter. Lass mich wissen, ob ich noch etwas anpassen soll.",
            GOOD_S1
        );
        assert!(evaluate_verify_output(S1, &rambling)
            .unwrap_err()
            .contains("extra text"));
    }

    #[test]
    fn request_body_has_the_app_request_form() {
        let body = verify_request_body("lfm3:1.7b", S1);
        assert_eq!(body["model"], "lfm3:1.7b");
        assert_eq!(body["stream"], false);
        assert_eq!(body["think"], false);
        assert_eq!(body["keep_alive"], -1);
        assert_eq!(body["options"]["num_ctx"], 2048);
        assert_eq!(body["options"]["temperature"], 0.15);
        assert_eq!(body["options"]["num_predict"], 2048);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], OLLAMA_PROMPT_DE);
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(
            body["messages"][1]["content"],
            format!("<transcript>\n{}\n</transcript>", S1)
        );
    }

    #[test]
    fn verification_sentences_are_small_unpunctuated_german() {
        for sentence in VERIFY_SENTENCES {
            assert!(
                sentence.chars().all(|c| c.is_alphabetic() || c == ' '),
                "{}",
                sentence
            );
            assert_eq!(sentence, sentence.to_lowercase());
            let words = normalized_words(sentence);
            assert!((8..=16).contains(&words.len()), "{}", sentence);
            let unique: HashSet<&String> = words.iter().collect();
            assert_eq!(
                unique.len(),
                words.len(),
                "repeated word would skew retention"
            );
        }
    }

    // ── semver compatibility ─────────────────────────────────────────────────────

    #[test]
    fn compatibility_compares_stable_versions() {
        assert_eq!(
            runtime_compatibility(Some("0.40.7"), Some("0.30.9")),
            Some(true)
        );
        assert_eq!(
            runtime_compatibility(Some("0.30.9"), Some("0.30.9")),
            Some(true)
        );
        assert_eq!(
            runtime_compatibility(Some("0.30.4"), Some("0.30.9")),
            Some(false)
        );
        assert_eq!(
            runtime_compatibility(Some("0.40.7"), Some("0.40.13")),
            Some(false)
        );
        // numeric, not lexicographic: 0.9.0 < 0.40.0 and 0.40.13 > 0.40.7
        assert_eq!(
            runtime_compatibility(Some("0.40.13"), Some("0.40.7")),
            Some(true)
        );
        assert_eq!(
            runtime_compatibility(Some("0.9.0"), Some("0.40.0")),
            Some(false)
        );
        assert_eq!(
            runtime_compatibility(Some("v0.40.7"), Some("0.40.6")),
            Some(true)
        );
        assert_eq!(
            runtime_compatibility(Some(" 1.0.0 "), Some("0.99.99")),
            Some(true)
        );
    }

    #[test]
    fn compatibility_is_unknown_for_unreadable_versions() {
        for (runtime, requires) in [
            (Some("0.40.7-rc1"), Some("0.30.9")),
            (Some("0.40.7"), Some("0.41.0-rc2")),
            (Some("dev"), Some("0.30.9")),
            (Some(""), Some("0.30.9")),
            (Some("0.40"), Some("0.30.9")),
            (Some("0.40.7.1"), Some("0.30.9")),
            (Some("0.40.7"), Some("latest")),
            (Some("0.0.0"), Some("0.30.9")), // source builds report 0.0.0
            (None, Some("0.30.9")),
            (Some("0.40.7"), None),
            (None, None),
        ] {
            assert_eq!(
                runtime_compatibility(runtime, requires),
                None,
                "{:?} vs {:?}",
                runtime,
                requires
            );
        }
    }

    #[test]
    fn pull_is_blocked_only_when_incompatibility_is_proven() {
        assert_eq!(
            pull_block_reason("gemma4:e4b", Some("0.30.9"), Some("0.30.4")).as_deref(),
            Some("Model 'gemma4:e4b' needs Ollama >= 0.30.9 (running 0.30.4). Update the runtime first.")
        );
        assert_eq!(
            pull_block_reason("gemma4:e4b", Some("0.30.9"), Some("0.40.7")),
            None
        );
        assert_eq!(pull_block_reason("gemma4:e4b", Some("0.30.9"), None), None);
        assert_eq!(pull_block_reason("gemma4:e4b", None, Some("0.30.4")), None);
        assert_eq!(
            pull_block_reason("gemma4:e4b", Some("weird"), Some("0.30.4")),
            None
        );
    }

    #[test]
    fn requirement_detail_explains_each_outcome() {
        assert_eq!(
            requirement_detail(Some("0.40.7"), "0.30.9", Some(true)),
            None
        );
        assert_eq!(
            requirement_detail(Some("0.30.4"), "0.30.9", Some(false)).as_deref(),
            Some("Needs Ollama 0.30.9 or newer; running 0.30.4.")
        );
        assert!(requirement_detail(None, "0.30.9", None)
            .unwrap()
            .contains("version unknown"));
        assert!(requirement_detail(Some("0.40.7-rc1"), "0.30.9", None)
            .unwrap()
            .contains("Could not compare"));
    }

    #[test]
    fn runtime_version_is_read_from_api_version() {
        assert_eq!(
            parse_runtime_version(r#"{"version":"0.40.7"}"#).as_deref(),
            Some("0.40.7")
        );
        assert_eq!(
            parse_runtime_version(r#"{"version":"v0.30.4"}"#).as_deref(),
            Some("0.30.4")
        );
        assert_eq!(parse_runtime_version(r#"{"version":""}"#), None);
        assert_eq!(parse_runtime_version("not json"), None);
    }

    // ── names, backups, settings ─────────────────────────────────────────────────

    #[test]
    fn model_names_compare_like_ollama_does() {
        assert!(model_names_equal("gemma4", "gemma4:latest"));
        assert!(model_names_equal(" GEMMA4:E4B ", "gemma4:e4b"));
        assert!(!model_names_equal("gemma4:e4b", "gemma4:e2b"));
        assert!(model_names_equal(
            "hf.co/unsloth/x:Q4_K_M",
            "hf.co/unsloth/x:q4_k_m"
        ));
        // a colon in the host/namespace part is not a tag separator
        assert_eq!(
            normalize_model_name("registry/model"),
            "registry/model:latest"
        );
    }

    #[test]
    fn backup_names_are_valid_unique_and_recognizable() {
        for model in [
            "gemma4:e4b-it-q4_K_M",
            "qwen3.5:4b",
            "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M",
        ] {
            let backup = backup_name_for(model, 1_730_000_000_123);
            assert!(validate_ollama_model_name(&backup).is_ok(), "{}", backup);
            assert!(is_backup_name(&backup));
            assert_eq!(backup, backup.to_lowercase());
            assert!(backup.ends_with(":1730000000123"));
            assert!(!model_names_equal(&backup, model));
        }
        assert_ne!(backup_name_for("a:b", 1), backup_name_for("a:b", 2));
        assert!(backup_name_for(&"x".repeat(500), 1).len() < 200);
        assert!(!is_backup_name("gemma4:e4b"));
    }

    fn settings_with(active: &str, preferred: &str, postproc: &str) -> Settings {
        let mut settings = Settings::default();
        settings.ai_fallback.model = active.to_string();
        settings.providers.ollama.preferred_model = preferred.to_string();
        settings.postproc_llm_model = postproc.to_string();
        settings
    }

    #[test]
    fn settings_switch_only_the_fields_that_named_the_old_model() {
        let mut settings = settings_with("old:1", "other:2", "OLD:1");
        assert!(switch_model_in_settings(&mut settings, "old:1", "new:1"));
        assert_eq!(settings.ai_fallback.model, "new:1");
        assert_eq!(settings.providers.ollama.preferred_model, "other:2");
        assert_eq!(settings.postproc_llm_model, "new:1");
    }

    #[test]
    fn settings_stay_untouched_when_they_did_not_use_the_old_model() {
        let mut settings = settings_with("other:1", "other:1", "");
        assert!(!settings_reference_model(&settings, "old:1"));
        assert!(!switch_model_in_settings(&mut settings, "old:1", "new:1"));
        assert_eq!(settings.ai_fallback.model, "other:1");
        assert_eq!(settings.providers.ollama.preferred_model, "other:1");
        assert_eq!(settings.postproc_llm_model, "");
        // empty fields never match an (also empty) model name
        assert!(!switch_model_in_settings(&mut settings, "", "new:1"));
        assert_eq!(settings.postproc_llm_model, "");
    }

    #[test]
    fn median_and_truncation_helpers() {
        assert_eq!(median(&[]), 0);
        assert_eq!(median(&[30, 10, 20]), 20);
        assert_eq!(median(&[7]), 7);
        assert_eq!(truncate_chars("  kurz  ", 10), "kurz");
        assert_eq!(truncate_chars("äöüäöü", 3), "äöü…");
    }

    // ── mock Ollama ──────────────────────────────────────────────────────────────

    type Log = Arc<Mutex<Vec<String>>>;

    struct MockOllama {
        server: MockServer,
        log: Log,
    }

    type ChatReply = Box<dyn Fn(&str, &str) -> MockReply + Send + Sync>;

    struct MockConfig {
        installed: Vec<&'static str>,
        /// What the chat endpoint answers for a model (given the user content).
        chat: ChatReply,
        copy_status: u16,
        /// Models whose `/api/delete` answers 404 (already gone).
        delete_404: Vec<&'static str>,
    }

    impl MockConfig {
        fn new(installed: Vec<&'static str>) -> Self {
            Self {
                installed,
                chat: Box::new(|_, user| good_chat_reply(user)),
                copy_status: 200,
                delete_404: Vec::new(),
            }
        }
        fn chat(mut self, f: impl Fn(&str, &str) -> MockReply + Send + Sync + 'static) -> Self {
            self.chat = Box::new(f);
            self
        }
    }

    /// Echoes the transcript, capitalized and with a full stop.
    fn good_chat_reply(user_content: &str) -> MockReply {
        let text = user_content
            .trim_start_matches("<transcript>\n")
            .trim_end_matches("\n</transcript>");
        let mut chars = text.chars();
        let capitalized = chars
            .next()
            .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_default();
        MockReply::ok(serde_json::json!({
            "message": { "role": "assistant", "content": format!("{}.", capitalized) },
            "load_duration": 4_200_000_000u64,
            "done": true
        }))
    }

    fn start_mock(log: Log, config: MockConfig) -> MockOllama {
        let log_for_handler = log.clone();
        let server = MockServer::start(move |request: &MockRequest| {
            let body = request.json();
            let model = body["model"].as_str().unwrap_or("").to_string();
            let mut log = log_for_handler.lock().unwrap();
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/api/tags") => {
                    log.push("tags".into());
                    let models: Vec<_> = config
                        .installed
                        .iter()
                        .map(|name| serde_json::json!({ "name": name }))
                        .collect();
                    MockReply::ok(serde_json::json!({ "models": models }))
                }
                ("GET", "/api/version") => {
                    log.push("version".into());
                    MockReply::ok(serde_json::json!({ "version": "0.40.7" }))
                }
                ("POST", "/api/chat") => {
                    log.push(format!("chat:{}", model));
                    let user = body["messages"][1]["content"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    drop(log);
                    (config.chat)(&model, &user)
                }
                ("POST", "/api/copy") => {
                    log.push(format!(
                        "copy:{}->{}",
                        body["source"].as_str().unwrap_or(""),
                        body["destination"].as_str().unwrap_or("")
                    ));
                    if config.copy_status == 200 {
                        MockReply::empty_ok()
                    } else {
                        MockReply::error(config.copy_status, "copy failed")
                    }
                }
                ("DELETE", "/api/delete") => {
                    log.push(format!("delete:{}", model));
                    if config.delete_404.iter().any(|gone| *gone == model) {
                        MockReply::error(404, "model not found")
                    } else {
                        MockReply::empty_ok()
                    }
                }
                ("POST", "/api/generate") => {
                    assert_eq!(body["keep_alive"], "0m", "generate is only used to unload");
                    log.push(format!("unload:{}", model));
                    MockReply::ok(serde_json::json!({ "done": true }))
                }
                _ => MockReply::error(404, "unknown route"),
            }
        });
        MockOllama { server, log }
    }

    fn mock(config: MockConfig) -> MockOllama {
        start_mock(Arc::new(Mutex::new(Vec::new())), config)
    }

    impl MockOllama {
        fn endpoint(&self) -> &str {
            &self.server.base_url
        }
        fn trace(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    fn position(trace: &[String], entry: &str) -> usize {
        trace
            .iter()
            .position(|e| e == entry)
            .unwrap_or_else(|| panic!("{} missing in {:?}", entry, trace))
    }

    const LIMIT: Duration = Duration::from_secs(20);

    // ── verify against the mock ──────────────────────────────────────────────────

    #[test]
    fn verification_passes_for_a_model_that_corrects_the_text() {
        let ollama = mock(MockConfig::new(vec!["lfm3:1.7b"]));
        let result = run_model_verification(ollama.endpoint(), "lfm3:1.7b", LIMIT);

        assert!(result.ok, "{:?}", result);
        assert_eq!(result.detail, None);
        assert_eq!(
            result.sample.as_deref(),
            Some("Das wetter ist heute sehr schön und wir gehen später in den park.")
        );
        assert_eq!(result.load_ms, 4200);
        assert_eq!(ollama.trace(), vec!["chat:lfm3:1.7b"; 3]);
        // the wire form is the app's request form
        let first = &ollama.server.requests()[0];
        assert_eq!(first.path, "/api/chat");
        assert_eq!(
            first.json(),
            verify_request_body("lfm3:1.7b", VERIFY_SENTENCES[0])
        );
    }

    #[test]
    fn verification_fails_fast_on_a_chatty_model() {
        let ollama = mock(MockConfig::new(vec!["m:1"]).chat(|_, user| {
            MockReply::ok(serde_json::json!({
                "message": { "content": format!("Hier ist der korrigierte Text: {}", user) }
            }))
        }));
        let result = run_model_verification(ollama.endpoint(), "m:1", LIMIT);

        assert!(!result.ok);
        let detail = result.detail.expect("reason");
        assert!(
            detail.starts_with("Sentence 1:") && detail.contains("commentary"),
            "{}",
            detail
        );
        assert!(result
            .sample
            .expect("offending answer")
            .starts_with("Hier ist"));
        assert_eq!(
            ollama.trace(),
            vec!["chat:m:1"],
            "stops at the first failing sentence"
        );
    }

    #[test]
    fn verification_reports_http_errors_with_ollamas_message() {
        let ollama =
            mock(MockConfig::new(vec![]).chat(|_, _| {
                MockReply::error(412, "this model requires a newer version of Ollama")
            }));
        let result = run_model_verification(ollama.endpoint(), "new:1", LIMIT);
        assert!(!result.ok);
        let detail = result.detail.expect("reason");
        assert!(
            detail.contains("HTTP 412") && detail.contains("requires a newer version"),
            "{}",
            detail
        );
        assert_eq!(result.latency_ms, 0, "no request completed");
    }

    #[test]
    fn verification_fails_on_an_empty_answer_from_the_server() {
        let ollama = mock(
            MockConfig::new(vec!["m:1"])
                .chat(|_, _| MockReply::ok(serde_json::json!({ "message": { "content": "" } }))),
        );
        let result = run_model_verification(ollama.endpoint(), "m:1", LIMIT);
        assert!(!result.ok);
        assert!(result.detail.unwrap().contains("empty"));
    }

    #[test]
    fn verification_enforces_the_time_limit() {
        let ollama = mock(
            MockConfig::new(vec!["slow:1"])
                .chat(|_, user| good_chat_reply(user).delayed(Duration::from_millis(2500))),
        );
        let started = Instant::now();
        let result =
            run_model_verification(ollama.endpoint(), "slow:1", Duration::from_millis(400));

        assert!(!result.ok);
        let detail = result.detail.expect("reason");
        assert!(
            detail.contains("Time limit of 400 ms exceeded"),
            "{}",
            detail
        );
        assert!(
            started.elapsed() < Duration::from_millis(2000),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn verification_reports_a_server_that_drops_the_connection() {
        // accepts and immediately closes: a connection fault, not a timeout
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                drop(stream);
            }
        });
        let started = Instant::now();
        let result = run_model_verification(&format!("http://127.0.0.1:{}", port), "m:1", LIMIT);

        assert!(!result.ok);
        let detail = result.detail.expect("reason");
        assert!(detail.contains("could not reach Ollama"), "{}", detail);
        assert!(!detail.contains("Time limit"), "{}", detail);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    // ── backup ───────────────────────────────────────────────────────────────────

    #[test]
    fn backup_copies_the_installed_model_under_a_backup_name() {
        let ollama = mock(MockConfig::new(vec!["gemma4:e4b"]));
        let backup = backup_model_impl(ollama.endpoint(), "gemma4:e4b", 42).expect("backup");
        assert_eq!(backup, "trispr-backup-gemma4-e4b:42");
        assert_eq!(
            ollama.trace(),
            vec!["tags", "copy:gemma4:e4b->trispr-backup-gemma4-e4b:42"]
        );
    }

    #[test]
    fn backup_of_a_missing_model_copies_nothing() {
        let ollama = mock(MockConfig::new(vec!["other:1"]));
        let error = backup_model_impl(ollama.endpoint(), "gemma4:e4b", 42).unwrap_err();
        assert!(error.contains("not installed"), "{}", error);
        assert_eq!(ollama.trace(), vec!["tags"]);
    }

    // ── finalize: order guarantee ────────────────────────────────────────────────

    const MODEL: &str = "gemma4:e4b";
    const BACKUP: &str = "trispr-backup-gemma4-e4b:42";

    #[test]
    fn finalize_deletes_the_backup_only_after_the_new_build_passed() {
        let ollama = mock(MockConfig::new(vec![MODEL, BACKUP]));
        let result = finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap();

        assert_eq!(
            result,
            FinalizeResult {
                verified: true,
                restored: false,
                inconclusive: false,
                detail: None
            }
        );
        let trace = ollama.trace();
        let last_chat = trace.iter().rposition(|e| e.starts_with("chat:")).unwrap();
        assert!(
            position(&trace, &format!("delete:{}", BACKUP)) > last_chat,
            "{:?}",
            trace
        );
        assert!(
            !trace.iter().any(|e| e == &format!("delete:{}", MODEL)),
            "{:?}",
            trace
        );
        assert!(!trace.iter().any(|e| e.starts_with("copy:")), "{:?}", trace);
        assert_eq!(trace.iter().filter(|e| e.starts_with("chat:")).count(), 3);
    }

    #[test]
    fn finalize_restores_the_backup_when_the_new_build_fails() {
        let ollama = mock(MockConfig::new(vec![MODEL, BACKUP]).chat(|_, _| {
            MockReply::ok(serde_json::json!({ "message": { "content": "Zzz zzz" } }))
        }));
        let result = finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap();

        assert!(!result.verified);
        assert!(result.restored);
        assert!(result.detail.unwrap().contains("restored"));
        let trace = ollama.trace();
        // Order: new tag unloaded+deleted, THEN backup copied back, THEN backup removed.
        let delete_new = position(&trace, &format!("delete:{}", MODEL));
        let copy_back = position(&trace, &format!("copy:{}->{}", BACKUP, MODEL));
        let delete_backup = position(&trace, &format!("delete:{}", BACKUP));
        assert!(
            position(&trace, &format!("unload:{}", MODEL)) < delete_new,
            "{:?}",
            trace
        );
        assert!(
            delete_new < copy_back && copy_back < delete_backup,
            "{:?}",
            trace
        );
        // the backup is never deleted before the restore copy happened
        assert_eq!(
            trace
                .iter()
                .filter(|e| *e == &format!("delete:{}", BACKUP))
                .count(),
            1
        );
    }

    #[test]
    fn finalize_keeps_the_backup_when_the_restore_copy_fails() {
        let mut config = MockConfig::new(vec![MODEL, BACKUP])
            .chat(|_, _| MockReply::ok(serde_json::json!({ "message": { "content": "" } })));
        config.copy_status = 500;
        let ollama = mock(config);
        let result = finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap();

        assert!(!result.verified);
        assert!(!result.restored);
        let detail = result.detail.unwrap();
        assert!(
            detail.contains(BACKUP) && detail.contains("still available"),
            "{}",
            detail
        );
        let trace = ollama.trace();
        assert!(
            !trace.iter().any(|e| e == &format!("delete:{}", BACKUP)),
            "the backup is the only copy of the old model: {:?}",
            trace
        );
    }

    #[test]
    fn finalize_refuses_a_backup_that_does_not_exist_before_touching_anything() {
        let ollama = mock(MockConfig::new(vec![MODEL]));
        let error =
            finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap_err();
        assert!(
            error.contains("not found") && error.contains("nothing was changed"),
            "{}",
            error
        );
        assert_eq!(
            ollama.trace(),
            vec!["tags"],
            "no verify, no delete, no copy"
        );
    }

    #[test]
    fn finalize_refuses_names_that_are_not_trispr_backups() {
        let ollama = mock(MockConfig::new(vec![MODEL, "precious:1"]));
        let error =
            finalize_model_update_impl(ollama.endpoint(), MODEL, "precious:1", LIMIT).unwrap_err();
        assert!(error.contains("not a Trispr Flow backup name"), "{}", error);
        assert!(
            ollama.trace().is_empty(),
            "nothing may be contacted: {:?}",
            ollama.trace()
        );
        assert!(finalize_model_update_impl(ollama.endpoint(), BACKUP, BACKUP, LIMIT).is_err());
    }

    // ── replace: order guarantee + settings ──────────────────────────────────────

    const OLD: &str = "gemma4:e4b-it-q4_K_M";
    const NEW: &str = "gemma4:e4b";

    #[test]
    fn replace_switches_settings_and_deletes_the_old_model_only_after_the_check() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let ollama = start_mock(log.clone(), MockConfig::new(vec![OLD, NEW]));
        let settings_log = log.clone();
        let mut switch = move |old: &str, new: &str| {
            settings_log
                .lock()
                .unwrap()
                .push(format!("settings:{}->{}", old, new));
            Ok(true)
        };

        let result = replace_model_impl(ollama.endpoint(), OLD, NEW, &mut switch, LIMIT).unwrap();

        assert_eq!(
            (
                result.verified,
                result.switched_active,
                result.deleted_old,
                result.detail.clone()
            ),
            (true, true, true, None)
        );
        let trace = ollama.trace();
        let last_chat = trace.iter().rposition(|e| e.starts_with("chat:")).unwrap();
        let settings = position(&trace, &format!("settings:{}->{}", OLD, NEW));
        let unload_old = position(&trace, &format!("unload:{}", OLD));
        let delete_old = position(&trace, &format!("delete:{}", OLD));
        assert!(
            last_chat < settings && settings < unload_old && unload_old < delete_old,
            "{:?}",
            trace
        );
        assert!(trace
            .iter()
            .all(|e| !e.starts_with("chat:") || e == &format!("chat:{}", NEW)));
        assert!(
            !trace.iter().any(|e| e == &format!("delete:{}", NEW)),
            "{:?}",
            trace
        );
    }

    #[test]
    fn replace_changes_nothing_when_the_new_model_fails_the_check() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let ollama = start_mock(
            log.clone(),
            MockConfig::new(vec![OLD, NEW]).chat(|_, user| {
                MockReply::ok(serde_json::json!({
                    "message": { "content": format!("Sure! {}", user) }
                }))
            }),
        );
        let settings_log = log.clone();
        let mut switch = move |_: &str, _: &str| {
            settings_log.lock().unwrap().push("settings".to_string());
            Ok(true)
        };

        let result = replace_model_impl(ollama.endpoint(), OLD, NEW, &mut switch, LIMIT).unwrap();

        assert!(!result.verified && !result.switched_active && !result.deleted_old);
        assert!(result.detail.expect("reason").contains("commentary"));
        let trace = ollama.trace();
        assert!(!trace.iter().any(|e| e == "settings"), "{:?}", trace);
        assert!(
            !trace
                .iter()
                .any(|e| e.starts_with("delete:") || e.starts_with("copy:")),
            "{:?}",
            trace
        );
        // the only side effect is releasing the model the check itself loaded
        assert_eq!(
            trace.last().unwrap(),
            &format!("unload:{}", NEW),
            "{:?}",
            trace
        );
        assert!(
            !trace.iter().any(|e| e == &format!("unload:{}", OLD)),
            "{:?}",
            trace
        );
    }

    #[test]
    fn replace_keeps_the_old_model_when_the_settings_cannot_be_saved() {
        let ollama = mock(MockConfig::new(vec![OLD, NEW]));
        let mut switch = |_: &str, _: &str| Err("disk full".to_string());

        let result = replace_model_impl(ollama.endpoint(), OLD, NEW, &mut switch, LIMIT).unwrap();

        assert!(result.verified && !result.switched_active && !result.deleted_old);
        assert!(result.detail.unwrap().contains("disk full"));
        assert!(
            !ollama.trace().iter().any(|e| e.starts_with("delete:")),
            "{:?}",
            ollama.trace()
        );
    }

    #[test]
    fn replace_still_deletes_the_old_model_when_no_setting_used_it() {
        let ollama = mock(MockConfig::new(vec![OLD, NEW]));
        let mut switch = |_: &str, _: &str| Ok(false);

        let result = replace_model_impl(ollama.endpoint(), OLD, NEW, &mut switch, LIMIT).unwrap();

        assert!(result.verified && !result.switched_active && result.deleted_old);
        assert!(ollama.trace().contains(&format!("delete:{}", OLD)));
    }

    #[test]
    fn replace_refuses_identical_names_and_uninstalled_targets() {
        let ollama = mock(MockConfig::new(vec![OLD]));
        let mut switch = |_: &str, _: &str| Ok(true);

        let same = replace_model_impl(
            ollama.endpoint(),
            "Gemma4:E4B",
            "gemma4:e4b",
            &mut switch,
            LIMIT,
        )
        .unwrap_err();
        assert!(same.contains("same model"), "{}", same);
        let implicit_latest = replace_model_impl(
            ollama.endpoint(),
            "gemma4",
            "gemma4:latest",
            &mut switch,
            LIMIT,
        );
        assert!(implicit_latest.is_err());
        assert!(
            ollama.trace().is_empty(),
            "name checks come before any request"
        );

        let missing =
            replace_model_impl(ollama.endpoint(), OLD, NEW, &mut switch, LIMIT).unwrap_err();
        assert!(missing.contains("not installed"), "{}", missing);
        assert_eq!(ollama.trace(), vec!["tags"], "no verify, no delete");
    }

    // ── finalize: "could not finish" is not "failed" ─────────────────────────────

    fn assert_nothing_destroyed(trace: &[String]) {
        assert!(
            !trace
                .iter()
                .any(|e| e.starts_with("delete:") || e.starts_with("copy:")),
            "an inconclusive check must not delete or copy anything: {:?}",
            trace
        );
    }

    fn assert_inconclusive(result: &FinalizeResult) {
        assert!(!result.verified, "{:?}", result);
        assert!(!result.restored, "{:?}", result);
        assert!(result.inconclusive, "{:?}", result);
        let detail = result.detail.as_deref().unwrap_or("");
        assert!(
            detail.contains(MODEL) && detail.contains(BACKUP) && detail.contains("both kept"),
            "the detail names both tags: {}",
            detail
        );
    }

    #[test]
    fn finalize_is_inconclusive_when_the_time_limit_runs_out() {
        let ollama = mock(
            MockConfig::new(vec![MODEL, BACKUP])
                .chat(|_, user| good_chat_reply(user).delayed(Duration::from_millis(2500))),
        );
        let started = Instant::now();
        let result = finalize_model_update_impl(
            ollama.endpoint(),
            MODEL,
            BACKUP,
            Duration::from_millis(400),
        )
        .unwrap();

        assert_inconclusive(&result);
        assert!(
            result
                .detail
                .as_deref()
                .unwrap()
                .contains("Time limit of 400 ms exceeded"),
            "{:?}",
            result
        );
        assert!(
            started.elapsed() < Duration::from_millis(2000),
            "{:?}",
            started.elapsed()
        );
        assert_nothing_destroyed(&ollama.trace());
    }

    #[test]
    fn finalize_is_inconclusive_when_the_connection_drops() {
        let ollama = mock(MockConfig::new(vec![MODEL, BACKUP]).chat(|_, _| MockReply::hang_up()));
        let result = finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap();

        assert_inconclusive(&result);
        assert!(
            result
                .detail
                .as_deref()
                .unwrap()
                .contains("could not reach Ollama"),
            "{:?}",
            result
        );
        assert_nothing_destroyed(&ollama.trace());
    }

    #[test]
    fn finalize_is_inconclusive_when_ollama_stops_after_the_backup_check() {
        // the server answers the tag list, then goes away before the first chat
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tags =
            serde_json::json!({ "models": [{ "name": MODEL }, { "name": BACKUP }] }).to_string();
        let server = std::thread::spawn(move || {
            // first connection: /api/tags; then the listener is dropped
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    tags.len(),
                    tags
                );
            }
        });
        let endpoint = format!("http://127.0.0.1:{}", port);
        let result = finalize_model_update_impl(&endpoint, MODEL, BACKUP, LIMIT).unwrap();
        server.join().unwrap();

        assert_inconclusive(&result);
    }

    #[test]
    fn finalize_is_inconclusive_on_server_errors() {
        for status in [500u16, 502, 503] {
            let ollama = mock(
                MockConfig::new(vec![MODEL, BACKUP])
                    .chat(move |_, _| MockReply::error(status, "runner crashed")),
            );
            let result =
                finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap();

            assert_inconclusive(&result);
            assert!(
                result
                    .detail
                    .as_deref()
                    .unwrap()
                    .contains(&format!("HTTP {}", status)),
                "{:?}",
                result
            );
            assert_nothing_destroyed(&ollama.trace());
        }
    }

    #[test]
    fn finalize_is_inconclusive_when_the_answer_is_unreadable() {
        let ollama = mock(
            MockConfig::new(vec![MODEL, BACKUP])
                .chat(|_, _| MockReply::text(200, "<html>captive portal</html>")),
        );
        let result = finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap();
        assert_inconclusive(&result);
        assert_nothing_destroyed(&ollama.trace());
    }

    #[test]
    fn a_refusal_of_the_server_is_still_a_failed_check_and_restores() {
        // 4xx = the server answered about the model (missing, runtime too old, ...)
        for (status, message) in [
            (404u16, "model not found"),
            (412, "requires a newer version of Ollama"),
        ] {
            let ollama = mock(
                MockConfig::new(vec![MODEL, BACKUP])
                    .chat(move |_, _| MockReply::error(status, message)),
            );
            let result =
                finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap();

            assert!(
                !result.verified && result.restored && !result.inconclusive,
                "{:?}",
                result
            );
            let trace = ollama.trace();
            assert!(
                position(&trace, &format!("delete:{}", MODEL))
                    < position(&trace, &format!("copy:{}->{}", BACKUP, MODEL)),
                "{:?}",
                trace
            );
        }
    }

    #[test]
    fn failed_criteria_still_restore_and_are_not_inconclusive() {
        let ollama = mock(MockConfig::new(vec![MODEL, BACKUP]).chat(|_, user| {
            MockReply::ok(serde_json::json!({
                "message": { "content": format!("Sure! {}", user) }
            }))
        }));
        let result = finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap();
        assert!(
            !result.verified && result.restored && !result.inconclusive,
            "{:?}",
            result
        );
    }

    #[test]
    fn the_verification_result_says_why_it_has_no_verdict_but_not_in_its_payload() {
        let slow = mock(
            MockConfig::new(vec![MODEL])
                .chat(|_, user| good_chat_reply(user).delayed(Duration::from_millis(2500))),
        );
        let timed_out = run_model_verification(slow.endpoint(), MODEL, Duration::from_millis(400));
        assert!(!timed_out.ok && timed_out.inconclusive, "{:?}", timed_out);

        let broken = mock(MockConfig::new(vec![MODEL]).chat(|_, _| MockReply::error(500, "boom")));
        assert!(run_model_verification(broken.endpoint(), MODEL, LIMIT).inconclusive);

        let refused =
            mock(MockConfig::new(vec![MODEL]).chat(|_, _| MockReply::error(412, "too old")));
        let refused = run_model_verification(refused.endpoint(), MODEL, LIMIT);
        assert!(!refused.ok && !refused.inconclusive, "{:?}", refused);

        let chatty = mock(MockConfig::new(vec![MODEL]).chat(|_, user| {
            MockReply::ok(
                serde_json::json!({ "message": { "content": format!("Sure! {}", user) } }),
            )
        }));
        assert!(!run_model_verification(chatty.endpoint(), MODEL, LIMIT).inconclusive);

        // the payload of `verify_ollama_model` keeps the shape the UI was built against
        let payload = serde_json::to_value(&timed_out).unwrap();
        let mut keys: Vec<&str> = payload
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["detail", "latency_ms", "load_ms", "ok", "sample"]
        );
    }

    #[test]
    fn the_finalize_payload_carries_inconclusive_and_the_rollback_payload_its_two_fields() {
        let finalize = serde_json::to_value(FinalizeResult {
            verified: false,
            restored: false,
            inconclusive: true,
            detail: Some("why".into()),
        })
        .unwrap();
        assert_eq!(
            finalize,
            serde_json::json!({"verified": false, "restored": false, "inconclusive": true, "detail": "why"})
        );
        let rollback = serde_json::to_value(RollbackResult {
            restored: true,
            detail: None,
        })
        .unwrap();
        assert_eq!(
            rollback,
            serde_json::json!({"restored": true, "detail": null})
        );
    }

    #[test]
    fn replace_changes_nothing_when_the_check_cannot_finish() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let ollama = start_mock(
            log.clone(),
            MockConfig::new(vec![OLD, NEW]).chat(|_, _| MockReply::error(503, "busy")),
        );
        let settings_log = log.clone();
        let mut switch = move |_: &str, _: &str| {
            settings_log.lock().unwrap().push("settings".to_string());
            Ok(true)
        };

        let result = replace_model_impl(ollama.endpoint(), OLD, NEW, &mut switch, LIMIT).unwrap();

        assert!(
            !result.verified && !result.switched_active && !result.deleted_old,
            "{:?}",
            result
        );
        let trace = ollama.trace();
        assert!(
            !trace
                .iter()
                .any(|e| e == "settings" || e.starts_with("delete:")),
            "{:?}",
            trace
        );
    }

    // ── rollback ─────────────────────────────────────────────────────────────────

    #[test]
    fn rollback_removes_the_new_build_then_copies_the_backup_back_then_deletes_the_backup() {
        let ollama = mock(MockConfig::new(vec![MODEL, BACKUP]));
        let result = rollback_model_update_impl(ollama.endpoint(), MODEL, BACKUP).unwrap();

        assert_eq!(
            result,
            RollbackResult {
                restored: true,
                detail: None
            }
        );
        let trace = ollama.trace();
        let unload_new = position(&trace, &format!("unload:{}", MODEL));
        let delete_new = position(&trace, &format!("delete:{}", MODEL));
        let copy_back = position(&trace, &format!("copy:{}->{}", BACKUP, MODEL));
        let delete_backup = position(&trace, &format!("delete:{}", BACKUP));
        assert!(
            unload_new < delete_new && delete_new < copy_back && copy_back < delete_backup,
            "{:?}",
            trace
        );
        assert!(
            !trace.iter().any(|e| e.starts_with("chat:")),
            "a rollback runs no check: {:?}",
            trace
        );
    }

    #[test]
    fn rollback_refuses_foreign_and_missing_backups_before_touching_anything() {
        let ollama = mock(MockConfig::new(vec![
            MODEL,
            "precious:1",
            "trispr-backup-other-model:1",
        ]));

        // not a Trispr Flow backup name
        let error = rollback_model_update_impl(ollama.endpoint(), MODEL, "precious:1").unwrap_err();
        assert!(error.contains("not a Trispr Flow backup name"), "{}", error);
        // another model's backup
        let error =
            rollback_model_update_impl(ollama.endpoint(), MODEL, "trispr-backup-other-model:1")
                .unwrap_err();
        assert!(error.contains("not a backup of"), "{}", error);
        assert!(
            ollama.trace().is_empty(),
            "nothing may be contacted: {:?}",
            ollama.trace()
        );

        // the right name, but the backup is gone: the new tag must not be deleted
        let error = rollback_model_update_impl(ollama.endpoint(), MODEL, BACKUP).unwrap_err();
        assert!(
            error.contains("not found") && error.contains("nothing was changed"),
            "{}",
            error
        );
        assert_nothing_destroyed(&ollama.trace());
        assert_eq!(ollama.trace(), vec!["tags"]);
    }

    #[test]
    fn rollback_keeps_the_backup_when_the_copy_back_fails() {
        let mut config = MockConfig::new(vec![MODEL, BACKUP]);
        config.copy_status = 500;
        let ollama = mock(config);

        let result = rollback_model_update_impl(ollama.endpoint(), MODEL, BACKUP).unwrap();

        assert!(!result.restored, "{:?}", result);
        let detail = result.detail.unwrap();
        assert!(
            detail.contains(BACKUP) && detail.contains("still available"),
            "{}",
            detail
        );
        assert!(
            !ollama
                .trace()
                .iter()
                .any(|e| e == &format!("delete:{}", BACKUP)),
            "the backup is the only copy of the old version: {:?}",
            ollama.trace()
        );
    }

    #[test]
    fn rollback_copes_with_a_new_tag_that_is_already_gone() {
        let mut config = MockConfig::new(vec![BACKUP]);
        config.delete_404 = vec![MODEL];
        let ollama = mock(config);

        let result = rollback_model_update_impl(ollama.endpoint(), MODEL, BACKUP).unwrap();

        assert!(result.restored, "{:?}", result);
        assert!(ollama
            .trace()
            .contains(&format!("copy:{}->{}", BACKUP, MODEL)));
    }

    #[test]
    fn an_inconclusive_update_can_be_checked_again_or_rolled_back() {
        // First the check cannot finish (nothing changes), then the user rolls back.
        let ollama =
            mock(MockConfig::new(vec![MODEL, BACKUP]).chat(|_, _| MockReply::error(503, "busy")));
        let first = finalize_model_update_impl(ollama.endpoint(), MODEL, BACKUP, LIMIT).unwrap();
        assert_inconclusive(&first);
        assert_nothing_destroyed(&ollama.trace());

        let rolled_back = rollback_model_update_impl(ollama.endpoint(), MODEL, BACKUP).unwrap();
        assert!(rolled_back.restored);
        let trace = ollama.trace();
        assert!(
            position(&trace, &format!("copy:{}->{}", BACKUP, MODEL))
                > position(&trace, &format!("chat:{}", MODEL))
        );
    }

    /// Deterministic hostile text: multi-byte characters, tag fragments, separators, control
    /// characters, in random order. Every parser of outside data must survive all of it.
    fn hostile_strings(count: usize) -> Vec<String> {
        const PIECES: &[&str] = &[
            "a",
            "Z",
            "0",
            ":",
            "/",
            "\\",
            "-",
            "_",
            ".",
            "<",
            ">",
            "</",
            "<transcript>",
            "</transcript>",
            "<think>",
            "</think>",
            "transcript",
            "ä",
            "ß",
            "日",
            "🦙",
            "\u{0}",
            "\u{feff}",
            " ",
            "\n",
            "\t",
            "sha256-",
            "sha256:",
            "hf.co/",
            "trispr-backup-",
            "O=",
            "\"",
            ",",
            " 0.40.13",
            "v",
            "ollama version is ",
            "client version is ",
            "hier ist ",
            "Sure",
            "=",
            "|",
        ];
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..count)
            .map(|_| {
                let length = 1 + (next() % 14) as usize;
                (0..length)
                    .map(|_| PIECES[(next() % PIECES.len() as u64) as usize])
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn parsers_of_model_output_and_names_survive_hostile_text() {
        for text in hostile_strings(20_000) {
            let _ = evaluate_verify_output(VERIFY_SENTENCES[0], &text);
            let _ = evaluate_verify_output(&text, &text);
            let _ = meta_prefix(&text);
            let _ = normalized_words(&text);
            let _ = normalize_model_name(&text);
            let _ = model_names_equal(&text, "gemma4:e4b");
            let backup = backup_name_for(&text, u64::MAX);
            assert!(backup.is_ascii(), "backup names are ASCII: {:?}", backup);
            assert!(
                is_backup_of(&text, &backup),
                "{:?} must own its backup name",
                text
            );
            let _ = is_backup_of("gemma4:e4b", &text);
            let _ = parse_runtime_version(&text);
            let _ = runtime_compatibility(Some(&text), Some(&text));
            let _ = truncate_chars(&text, 7);
            let _ = super::super::provider::strip_transcript_wrapper_tags(&text);
            let _ = super::super::provider::strip_thinking_tags(&text);
        }
    }

    // ── live registry ────────────────────────────────────────────────────────────

    /// Hits the real Ollama registry and Hugging Face (small manifest/config requests only).
    /// Run: cargo test --manifest-path src-tauri/Cargo.toml --lib -- live_ --ignored --nocapture
    #[test]
    #[ignore]
    fn live_check_ollama_model_requirements_reads_the_registry_config_blob() {
        let names = vec![
            "gemma4:e4b".to_string(),
            "lfm3:1.7b".to_string(),
            "bonsai-2:4b".to_string(),
            "hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M".to_string(),
            "no-such-model-xyz:1b".to_string(),
        ];
        let results = check_requirements_for_runtime(Some("0.40.7".to_string()), names);
        for r in &results {
            println!(
                "{} requires={:?} runtime={:?} compatible={:?} detail={:?}",
                r.name, r.requires, r.runtime_version, r.compatible, r.detail
            );
        }
        assert_eq!(results.len(), 5);
        assert_eq!(results[0].requires.as_deref(), Some("0.30.9"));
        assert_eq!(results[0].compatible, Some(true));
        assert_eq!(results[1].requires.as_deref(), Some("0.40.13"));
        assert_eq!(results[1].compatible, Some(false));
        assert_eq!(
            results[1].detail.as_deref(),
            Some("Needs Ollama 0.40.13 or newer; running 0.40.7.")
        );
        assert_eq!(results[2].requires.as_deref(), Some("0.40.6"));
        assert_eq!(results[2].compatible, Some(true));
        assert_eq!(results[3].requires, None);
        assert_eq!(results[3].compatible, None);
        assert_eq!(results[4].requires, None);
        assert!(results[4]
            .detail
            .as_deref()
            .unwrap_or("")
            .contains("Not found"));

        // an older runtime must be refused before a pull, a newer one not
        let old = check_requirements_for_runtime(
            Some("0.30.4".to_string()),
            vec!["gemma4:e4b".to_string()],
        );
        assert_eq!(old[0].compatible, Some(false));
        assert!(
            pull_block_reason("gemma4:e4b", old[0].requires.as_deref(), Some("0.30.4"))
                .unwrap()
                .contains("needs Ollama >= 0.30.9 (running 0.30.4)")
        );
        // without a known runtime version nothing is judged
        let unknown = check_requirements_for_runtime(None, vec!["gemma4:e4b".to_string()]);
        assert_eq!(unknown[0].requires.as_deref(), Some("0.30.9"));
        assert_eq!(unknown[0].compatible, None);
    }

    // ── live Ollama (real server, real model store) ──────────────────────────────
    //
    // The flows above are proven against a mock; these run them against a real Ollama.
    // They need `TRISPR_LIVE_OLLAMA=http://127.0.0.1:<port>` and skip cleanly without it.
    //
    // SAFETY: the server may be pointed at the user's real model store. These tests only ever
    // create, overwrite or delete tags named `trispr-live-*` (plus the
    // `trispr-backup-trispr-live-*` backups the flows derive from them, because
    // `finalize_ollama_model_update` insists on the `trispr-backup-` prefix). `gemma4:e4b` is
    // only read, copied from (aliases share the blobs) and chatted with; every scenario ends by
    // checking its digest is unchanged and that it still answers.
    //
    // Run (one at a time, they share one server and one loaded runner):
    //   TRISPR_LIVE_OLLAMA=http://127.0.0.1:11437 cargo test --manifest-path src-tauri/Cargo.toml \
    //     --lib -- live_ --ignored --nocapture --test-threads=1

    const LIVE_BASE: &str = "gemma4:e4b";
    const LIVE_OLD_BUILD: &str = "gemma4:e4b-it-q4_K_M";
    const LIVE_PREFIX: &str = "trispr-live-";
    const LIVE_BACKUP_PREFIX: &str = "trispr-backup-trispr-live-";
    const LIVE_TARGET: &str = "trispr-live-target:latest";
    const LIVE_OLD: &str = "trispr-live-old:latest";
    const LIVE_BROKEN: &str = "trispr-live-broken:latest";
    const LIVE_LIMIT: Duration = Duration::from_secs(90);
    const LIVE_CALL_TIMEOUT: Duration = Duration::from_secs(120);

    fn live_owned(name: &str) -> bool {
        let lowered = name.trim().to_lowercase();
        lowered.starts_with(LIVE_PREFIX) || lowered.starts_with(LIVE_BACKUP_PREFIX)
    }

    /// Extra aliases of the user's models that a human creates before a risky run
    /// (`/api/copy`, shared blobs): the sweep leaves them alone so they outlive a failed test.
    fn live_is_safety_net(name: &str) -> bool {
        name.trim()
            .to_lowercase()
            .starts_with("trispr-live-safety-")
    }

    fn live_assert_owned(name: &str) {
        assert!(
            live_owned(name),
            "live tests may only create, change or delete trispr-live-* tags, not '{}'",
            name
        );
    }

    fn live_try_json(
        endpoint: &str,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let raw = ollama_request(endpoint, method, path, body, LIVE_CALL_TIMEOUT)
            .map_err(|error| format!("{} {}: {}", method, path, describe_call_error(&error)))?;
        Ok(serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null))
    }

    fn live_json(
        endpoint: &str,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> serde_json::Value {
        live_try_json(endpoint, method, path, body).unwrap_or_else(|error| panic!("{}", error))
    }

    /// `(name, digest)` of every installed model.
    fn live_try_tags(endpoint: &str) -> Result<Vec<(String, String)>, String> {
        let json = live_try_json(endpoint, "GET", "/api/tags", None)?;
        Ok(json["models"]
            .as_array()
            .map(|models| {
                models
                    .iter()
                    .map(|model| {
                        (
                            model["name"].as_str().unwrap_or("").to_string(),
                            model["digest"].as_str().unwrap_or("").to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    fn live_digest(endpoint: &str, name: &str) -> Option<String> {
        live_try_tags(endpoint)
            .expect("list tags")
            .into_iter()
            .find(|(candidate, _)| model_names_equal(candidate, name))
            .map(|(_, digest)| digest)
    }

    fn live_owned_tags(endpoint: &str) -> Vec<String> {
        live_try_tags(endpoint)
            .expect("list tags")
            .into_iter()
            .map(|(name, _)| name)
            .filter(|name| live_owned(name) && !live_is_safety_net(name))
            .collect()
    }

    fn live_ps(endpoint: &str) -> Vec<String> {
        live_json(endpoint, "GET", "/api/ps", None)["models"]
            .as_array()
            .map(|models| {
                models
                    .iter()
                    .filter_map(|model| model["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Polls `/api/ps` until `done(names)`; returns how long it took, or the last list on timeout.
    fn live_wait_ps(
        endpoint: &str,
        within: Duration,
        done: impl Fn(&[String]) -> bool,
    ) -> Result<Duration, Vec<String>> {
        let started = Instant::now();
        loop {
            let names = live_ps(endpoint);
            if done(&names) {
                return Ok(started.elapsed());
            }
            if started.elapsed() > within {
                return Err(names);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn live_copy(endpoint: &str, source: &str, destination: &str) {
        live_assert_owned(destination);
        copy_model(endpoint, source, destination)
            .unwrap_or_else(|error| panic!("copy {} -> {}: {}", source, destination, error));
    }

    /// `/api/create ... FROM gemma4:e4b` plus extra request fields; an existing tag is overwritten.
    fn live_create(endpoint: &str, name: &str, extra: serde_json::Value) {
        live_assert_owned(name);
        let mut body = serde_json::json!({ "model": name, "from": LIVE_BASE, "stream": false });
        for (key, value) in extra.as_object().expect("extra fields") {
            body[key.as_str()] = value.clone();
        }
        live_json(endpoint, "POST", "/api/create", Some(&body));
    }

    /// A build that really is broken for the app's request form. `SYSTEM "..."` is not: the
    /// request carries its own system message, which wins over the Modelfile's. A stop sequence
    /// is applied to every request, so the answer is cut off at the first "e".
    fn live_create_broken(endpoint: &str, name: &str) {
        live_create(
            endpoint,
            name,
            serde_json::json!({ "parameters": { "stop": ["e"] } }),
        );
    }

    /// Loads `model` with the runner options the app uses and keeps it loaded.
    fn live_load(endpoint: &str, model: &str) {
        let body = serde_json::json!({
            "model": model,
            "prompt": "",
            "stream": false,
            "keep_alive": -1,
            "options": serde_json::Value::Object(ollama_runner_defining_options(VERIFY_NUM_CTX))
        });
        live_json(endpoint, "POST", "/api/generate", Some(&body));
    }

    fn live_unload_all(endpoint: &str, names: &[&str]) {
        for name in names {
            let _ = unload_ollama_model_impl(endpoint, name);
        }
        live_wait_ps(endpoint, Duration::from_secs(15), |loaded| {
            loaded.is_empty()
        })
        .unwrap_or_else(|loaded| panic!("runner did not unload: {:?}", loaded));
    }

    /// Independent of the flows under test: one app-form chat judged by the pure criteria.
    fn live_assert_answers(endpoint: &str, model: &str) {
        let sentence = VERIFY_SENTENCES[1];
        let body = verify_request_body(model, sentence);
        let json = live_json(endpoint, "POST", "/api/chat", Some(&body));
        let answer = json["message"]["content"].as_str().unwrap_or("");
        evaluate_verify_output(sentence, answer).unwrap_or_else(|why| {
            panic!(
                "'{}' does not answer properly: {} (answer: {:?})",
                model, why, answer
            )
        });
    }

    struct Live {
        endpoint: String,
        base_digest: String,
    }

    impl Live {
        /// `None` (test skips) without `TRISPR_LIVE_OLLAMA`.
        fn start() -> Option<Live> {
            let endpoint = std::env::var("TRISPR_LIVE_OLLAMA")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let Some(endpoint) = endpoint else {
                eprintln!("TRISPR_LIVE_OLLAMA is not set - skipping live test");
                return None;
            };
            let base_digest = live_digest(&endpoint, LIVE_BASE)
                .unwrap_or_else(|| panic!("'{}' is not installed on {}", LIVE_BASE, endpoint));
            let live = Live {
                endpoint,
                base_digest,
            };
            live.sweep(); // leftovers of an aborted earlier run
            Some(live)
        }

        /// Deletes every trispr-live-* / trispr-backup-trispr-live-* tag. Never panics (runs in Drop).
        fn sweep(&self) {
            let Ok(tags) = live_try_tags(&self.endpoint) else {
                return;
            };
            for (name, _) in tags
                .into_iter()
                .filter(|(name, _)| live_owned(name) && !live_is_safety_net(name))
            {
                eprintln!("live: removing '{}'", name);
                let _ = delete_ollama_model_impl(self.endpoint.clone(), name);
            }
        }

        fn assert_base_intact(&self, when: &str) {
            assert_eq!(
                live_digest(&self.endpoint, LIVE_BASE).as_deref(),
                Some(self.base_digest.as_str()),
                "{}: {} changed or vanished",
                when,
                LIVE_BASE
            );
            live_assert_answers(&self.endpoint, LIVE_BASE);
            println!(
                "{}: {} unchanged ({}) and answers",
                when,
                LIVE_BASE,
                &self.base_digest[..12]
            );
        }

        /// End of a scenario: remove the test tags, then prove the real model is untouched.
        fn finish(self, scenario: &str) {
            self.sweep();
            assert_eq!(
                live_owned_tags(&self.endpoint),
                Vec::<String>::new(),
                "{}: test tags left behind",
                scenario
            );
            self.assert_base_intact(scenario);
        }
    }

    impl Drop for Live {
        fn drop(&mut self) {
            self.sweep();
        }
    }

    /// Scenario 1: backup, new build is fine -> verified, backup gone, target kept.
    #[test]
    #[ignore]
    fn live_finalize_keeps_a_working_update_and_drops_the_backup() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        live_copy(ep, LIVE_BASE, LIVE_TARGET);
        let target_digest = live_digest(ep, LIVE_TARGET).expect("target installed");
        println!(
            "target digest {} (base {})",
            target_digest, live.base_digest
        );

        let backup = backup_model_impl(ep, LIVE_TARGET, crate::util::now_ms()).expect("backup");
        println!("backup name {}", backup);
        live_assert_owned(&backup);
        assert!(
            backup.starts_with("trispr-backup-trispr-live-target-latest:"),
            "{}",
            backup
        );
        assert_eq!(
            live_digest(ep, &backup).as_deref(),
            Some(target_digest.as_str()),
            "a backup is the same manifest"
        );
        assert_eq!(
            live_digest(ep, LIVE_TARGET).as_deref(),
            Some(target_digest.as_str()),
            "backing up must not touch the target"
        );

        let started = Instant::now();
        let result =
            finalize_model_update_impl(ep, LIVE_TARGET, &backup, LIVE_LIMIT).expect("finalize");
        println!(
            "finalize took {} ms -> {:?}",
            started.elapsed().as_millis(),
            result
        );
        assert_eq!(
            result,
            FinalizeResult {
                verified: true,
                restored: false,
                inconclusive: false,
                detail: None
            }
        );
        assert_eq!(
            live_digest(ep, &backup),
            None,
            "the backup goes after a passed check"
        );
        assert_eq!(
            live_digest(ep, LIVE_TARGET).as_deref(),
            Some(target_digest.as_str()),
            "the target stays"
        );
        assert_eq!(live_owned_tags(ep), vec![LIVE_TARGET.to_string()]);
        live.finish("scenario 1");
    }

    /// Scenario 2: the target tag is overwritten by a broken build -> check fails, original back.
    #[test]
    #[ignore]
    fn live_finalize_restores_the_original_when_the_new_build_is_broken() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        live_copy(ep, LIVE_BASE, LIVE_TARGET);
        let original = live_digest(ep, LIVE_TARGET).expect("target installed");
        let backup = backup_model_impl(ep, LIVE_TARGET, crate::util::now_ms()).expect("backup");
        println!("original {} backup {}", &original[..12], backup);

        // stands in for `ollama pull` replacing the tag with a bad build
        live_create_broken(ep, LIVE_TARGET);
        let broken = live_digest(ep, LIVE_TARGET).expect("target installed");
        println!("broken build {}", &broken[..12]);
        assert_ne!(broken, original, "the tag really was overwritten");
        let direct = run_model_verification(ep, LIVE_TARGET, LIVE_LIMIT);
        println!("direct check of the broken build: {:?}", direct);
        assert!(
            !direct.ok,
            "the broken build must fail the check on its own"
        );

        let started = Instant::now();
        let result =
            finalize_model_update_impl(ep, LIVE_TARGET, &backup, LIVE_LIMIT).expect("finalize");
        println!(
            "finalize took {} ms -> {:?}",
            started.elapsed().as_millis(),
            result
        );
        assert!(!result.verified, "{:?}", result);
        assert!(result.restored, "{:?}", result);
        assert!(
            result.detail.as_deref().unwrap_or("").contains("restored"),
            "{:?}",
            result
        );
        assert_eq!(
            live_digest(ep, LIVE_TARGET).as_deref(),
            Some(original.as_str()),
            "the target has its original digest again"
        );
        assert_eq!(live_digest(ep, &backup), None, "backup deleted");
        assert_eq!(
            live_owned_tags(ep),
            vec![LIVE_TARGET.to_string()],
            "nothing but the target is left"
        );
        // the restored build is the working one
        let after = run_model_verification(ep, LIVE_TARGET, LIVE_LIMIT);
        assert!(after.ok, "{:?}", after);
        live.finish("scenario 2");
    }

    /// Why scenario 2 does not use `SYSTEM`: the Modelfile system prompt never reaches a request
    /// that brings its own system message (which is how the app and the check always ask).
    #[test]
    #[ignore]
    fn live_a_modelfile_system_prompt_does_not_break_the_app_request_form() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        live_create(
            ep,
            LIVE_BROKEN,
            serde_json::json!({ "system": "Antworte ausschließlich mit dem Wort Banane." }),
        );
        let result = run_model_verification(ep, LIVE_BROKEN, LIVE_LIMIT);
        println!("SYSTEM-only build: {:?}", result);
        assert!(result.ok, "{:?}", result);
        live.finish("system-only build");
    }

    /// Scenario 3a: replace with a model that passes -> old alias gone, new one kept.
    #[test]
    #[ignore]
    fn live_replace_deletes_the_old_alias_after_the_new_model_passed() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        live_copy(ep, LIVE_BASE, LIVE_OLD);
        let mut switched: Vec<(String, String)> = Vec::new();
        let result = {
            let mut switch = |old: &str, new: &str| {
                switched.push((old.to_string(), new.to_string()));
                Ok(true)
            };
            replace_model_impl(ep, LIVE_OLD, LIVE_BASE, &mut switch, LIVE_LIMIT).expect("replace")
        };
        println!("replace -> {:?}", result);
        assert!(result.verified, "{:?}", result);
        assert!(result.switched_active, "{:?}", result);
        assert!(result.deleted_old, "{:?}", result);
        assert_eq!(result.detail, None);
        assert!(result.latency_ms > 0);
        assert_eq!(
            switched,
            vec![(LIVE_OLD.to_string(), LIVE_BASE.to_string())],
            "settings are switched exactly once, old -> new"
        );
        assert_eq!(live_digest(ep, LIVE_OLD), None, "old is gone");
        assert_eq!(
            live_digest(ep, LIVE_BASE).as_deref(),
            Some(live.base_digest.as_str()),
            "new is still there"
        );
        live.finish("scenario 3a");
    }

    /// Scenario 3b: replace with a model that fails the check -> nothing deleted or switched.
    #[test]
    #[ignore]
    fn live_replace_changes_nothing_when_the_new_model_is_broken() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        live_copy(ep, LIVE_BASE, LIVE_OLD);
        live_create_broken(ep, LIVE_BROKEN);
        let old_digest = live_digest(ep, LIVE_OLD).expect("old installed");
        let broken_digest = live_digest(ep, LIVE_BROKEN).expect("broken installed");
        let mut switched: Vec<(String, String)> = Vec::new();
        let result = {
            let mut switch = |old: &str, new: &str| {
                switched.push((old.to_string(), new.to_string()));
                Ok(true)
            };
            replace_model_impl(ep, LIVE_OLD, LIVE_BROKEN, &mut switch, LIVE_LIMIT).expect("replace")
        };
        println!("replace -> {:?}", result);
        assert!(!result.verified, "{:?}", result);
        assert!(
            !result.switched_active && !result.deleted_old,
            "{:?}",
            result
        );
        assert!(
            result
                .detail
                .as_deref()
                .unwrap_or("")
                .starts_with("Sentence 1:"),
            "{:?}",
            result
        );
        assert!(
            switched.is_empty(),
            "settings must stay untouched: {:?}",
            switched
        );
        assert_eq!(
            live_digest(ep, LIVE_OLD).as_deref(),
            Some(old_digest.as_str()),
            "old is still installed and unchanged"
        );
        assert_eq!(
            live_digest(ep, LIVE_BROKEN).as_deref(),
            Some(broken_digest.as_str()),
            "the failed new model is left alone"
        );
        live.finish("scenario 3b");
    }

    /// Scenario 4: a loaded alias must be gone from `/api/ps` after delete / replace.
    #[test]
    #[ignore]
    fn live_loaded_aliases_are_unloaded_before_they_are_deleted() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        let alias_a = "trispr-live-loaded-a:latest";
        let alias_b = "trispr-live-loaded-b:latest";

        // phase 1: plain delete of a loaded alias
        live_unload_all(ep, &[LIVE_BASE]);
        live_copy(ep, LIVE_BASE, alias_a);
        live_load(ep, alias_a);
        let loaded = live_ps(ep);
        println!("phase 1, ps with the alias loaded: {:?}", loaded);
        assert!(
            loaded.iter().any(|name| model_names_equal(name, alias_a)),
            "{:?}",
            loaded
        );
        delete_ollama_model_impl(ep.to_string(), alias_a.to_string()).expect("delete alias");
        let gone = live_wait_ps(ep, Duration::from_secs(10), |names| {
            !names.iter().any(|name| model_names_equal(name, alias_a))
        })
        .unwrap_or_else(|names| panic!("deleted alias is still loaded: {:?}", names));
        println!(
            "phase 1, alias left /api/ps {} ms after the delete returned; ps now {:?}",
            gone.as_millis(),
            live_ps(ep)
        );
        assert_eq!(live_digest(ep, alias_a), None);
        live.assert_base_intact("phase 1");

        // phase 2: the replace flow with the loaded alias as the old model
        live_unload_all(ep, &[LIVE_BASE]);
        live_copy(ep, LIVE_BASE, alias_b);
        live_load(ep, alias_b);
        let loaded = live_ps(ep);
        println!("phase 2, ps with the alias loaded: {:?}", loaded);
        assert!(
            loaded.iter().any(|name| model_names_equal(name, alias_b)),
            "{:?}",
            loaded
        );
        let mut no_settings = |_: &str, _: &str| Ok(false);
        let result = replace_model_impl(ep, alias_b, LIVE_BASE, &mut no_settings, LIVE_LIMIT)
            .expect("replace");
        println!("phase 2, replace -> {:?}", result);
        assert!(result.verified && result.deleted_old, "{:?}", result);
        let gone = live_wait_ps(ep, Duration::from_secs(10), |names| {
            !names.iter().any(|name| model_names_equal(name, alias_b))
        })
        .unwrap_or_else(|names| panic!("replaced alias is still loaded: {:?}", names));
        println!(
            "phase 2, alias left /api/ps {} ms after replace returned; ps now {:?}",
            gone.as_millis(),
            live_ps(ep)
        );
        assert_eq!(live_digest(ep, alias_b), None);
        live.assert_base_intact("phase 2");
        println!("ps after the base check: {:?}", live_ps(ep));
        live.finish("scenario 4");
    }

    /// Control for scenario 4 (informational): what a raw delete of a loaded alias leaves behind.
    /// It can strand a runner until the server restarts, so run it last and restart the server.
    #[test]
    #[ignore]
    fn live_control_raw_delete_of_a_loaded_alias_without_unloading() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        let alias = "trispr-live-ghost:latest";
        live_unload_all(ep, &[LIVE_BASE]);
        live_copy(ep, LIVE_BASE, alias);
        live_load(ep, alias);
        println!("ps before: {:?}", live_ps(ep));
        let body = serde_json::json!({ "model": alias });
        live_json(ep, "DELETE", "/api/delete", Some(&body));
        std::thread::sleep(Duration::from_secs(2));
        println!("ps after a raw delete (no unload): {:?}", live_ps(ep));
        println!(
            "unload by the deleted name afterwards: {:?}",
            unload_ollama_model_impl(ep, alias)
        );
        std::thread::sleep(Duration::from_secs(2));
        println!("ps after that unload attempt: {:?}", live_ps(ep));
        live.finish("control");
    }

    /// Names as real models carry them (mixed case, upper-case tags, namespace, no tag): the
    /// backup name must be accepted by Ollama and by the command's name validation, and both
    /// the keep path and the restore path must work and give back the original digest.
    #[test]
    #[ignore]
    fn live_flows_work_with_real_world_shaped_model_names() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        for target in [
            "trispr-live-Mixed.Case_Name:Q4_K_M",
            "trispr-live-ns/Sub-Model:v1.0",
            "trispr-live-bare",
            "trispr-live-hf.co/unsloth/Gemma-4-E4B-it-GGUF:Q4_K_M",
        ] {
            validate_ollama_model_name(target).expect("model name is valid");
            live_copy(ep, LIVE_BASE, target);
            let original = live_digest(ep, target).expect("target installed");

            // keep path
            let backup = backup_model_impl(ep, target, crate::util::now_ms()).expect("backup");
            validate_ollama_model_name(&backup).expect("backup name is valid");
            live_assert_owned(&backup);
            println!("{} -> backup {}", target, backup);
            let kept =
                finalize_model_update_impl(ep, target, &backup, LIVE_LIMIT).expect("finalize");
            assert!(kept.verified && !kept.restored, "{}: {:?}", target, kept);
            assert_eq!(live_digest(ep, &backup), None, "{}: backup deleted", target);
            assert_eq!(live_digest(ep, target).as_deref(), Some(original.as_str()));

            // restore path
            let backup = backup_model_impl(ep, target, crate::util::now_ms()).expect("backup");
            live_create_broken(ep, target);
            assert_ne!(live_digest(ep, target).as_deref(), Some(original.as_str()));
            let restored =
                finalize_model_update_impl(ep, target, &backup, LIVE_LIMIT).expect("finalize");
            println!("{} restore -> {:?}", target, restored);
            assert!(
                !restored.verified && restored.restored,
                "{}: {:?}",
                target,
                restored
            );
            assert_eq!(
                live_digest(ep, target).as_deref(),
                Some(original.as_str()),
                "{}: original digest is back",
                target
            );
            assert_eq!(live_digest(ep, &backup), None, "{}: backup deleted", target);
        }
        live.finish("names");
    }

    /// The pulled tag vanished entirely (nothing to verify): the previous version comes back.
    #[test]
    #[ignore]
    fn live_finalize_restores_when_the_updated_tag_is_missing() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        live_copy(ep, LIVE_BASE, LIVE_TARGET);
        let original = live_digest(ep, LIVE_TARGET).expect("target installed");
        let backup = backup_model_impl(ep, LIVE_TARGET, crate::util::now_ms()).expect("backup");
        live_assert_owned(LIVE_TARGET);
        delete_ollama_model_impl(ep.to_string(), LIVE_TARGET.to_string()).expect("delete target");
        assert_eq!(live_digest(ep, LIVE_TARGET), None);

        let result =
            finalize_model_update_impl(ep, LIVE_TARGET, &backup, LIVE_LIMIT).expect("finalize");
        println!("finalize -> {:?}", result);
        assert!(!result.verified && result.restored, "{:?}", result);
        assert_eq!(
            live_digest(ep, LIVE_TARGET).as_deref(),
            Some(original.as_str())
        );
        assert_eq!(live_digest(ep, &backup), None);
        live.finish("missing tag");
    }

    /// A stale UI may name an old model that is already gone: the new one is still checked and
    /// the settings are still switched, the missing delete is reported, nothing else happens.
    #[test]
    #[ignore]
    fn live_replace_with_an_old_model_that_is_already_gone() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        let ghost = "trispr-live-gone:latest";
        assert_eq!(live_digest(ep, ghost), None);
        let mut switched: Vec<(String, String)> = Vec::new();
        let result = {
            let mut switch = |old: &str, new: &str| {
                switched.push((old.to_string(), new.to_string()));
                Ok(true)
            };
            replace_model_impl(ep, ghost, LIVE_BASE, &mut switch, LIVE_LIMIT).expect("replace")
        };
        println!("replace -> {:?}", result);
        assert!(
            result.verified && result.switched_active && !result.deleted_old,
            "{:?}",
            result
        );
        assert!(
            result.detail.as_deref().unwrap_or("").contains("not found"),
            "{:?}",
            result
        );
        assert_eq!(switched.len(), 1);
        live.finish("gone old model");
    }

    /// Update and replace between two builds with DIFFERENT weights (old build -> gemma4:e4b), the
    /// way a real update looks: separate blobs, separate runners. The user's old build is only
    /// aliased (`/api/copy`), never touched itself, so its blobs cannot go away.
    #[test]
    #[ignore]
    fn live_update_and_replace_between_builds_with_different_weights() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        let Some(old_build) = live_digest(ep, LIVE_OLD_BUILD) else {
            eprintln!("{} is not installed - skipping", LIVE_OLD_BUILD);
            return;
        };
        let target = "trispr-live-upd:latest";

        // A: the tag held the old build; "the pull" put the new build under it; it works.
        live_copy(ep, LIVE_OLD_BUILD, target);
        let backup = backup_model_impl(ep, target, crate::util::now_ms()).expect("backup");
        live_copy(ep, LIVE_BASE, target); // stands in for `ollama pull` finishing
        assert_eq!(
            live_digest(ep, target).as_deref(),
            Some(live.base_digest.as_str())
        );
        let kept = finalize_model_update_impl(ep, target, &backup, LIVE_LIMIT).expect("finalize");
        println!("A (new build is good) -> {:?}", kept);
        assert!(kept.verified && !kept.restored, "{:?}", kept);
        assert_eq!(live_digest(ep, &backup), None, "A: backup deleted");
        assert_eq!(
            live_digest(ep, target).as_deref(),
            Some(live.base_digest.as_str())
        );
        assert_eq!(
            live_digest(ep, LIVE_OLD_BUILD).as_deref(),
            Some(old_build.as_str())
        );

        // B: the new build under the tag is bad; the old weights must come back.
        live_copy(ep, LIVE_OLD_BUILD, target);
        let backup = backup_model_impl(ep, target, crate::util::now_ms()).expect("backup");
        live_create_broken(ep, target);
        let restored =
            finalize_model_update_impl(ep, target, &backup, LIVE_LIMIT).expect("finalize");
        println!("B (new build is broken) -> {:?}", restored);
        assert!(!restored.verified && restored.restored, "{:?}", restored);
        assert_eq!(
            live_digest(ep, target).as_deref(),
            Some(old_build.as_str()),
            "B: the old build's digest is back"
        );
        assert_eq!(live_digest(ep, &backup), None, "B: backup deleted");
        let again = run_model_verification(ep, target, LIVE_LIMIT);
        assert!(again.ok, "B: the restored old build works: {:?}", again);

        // C: replace, old build loaded under its own runner, new build elsewhere.
        let old_alias = "trispr-live-old-b:latest";
        live_copy(ep, LIVE_OLD_BUILD, old_alias);
        live_load(ep, old_alias);
        println!("C ps before: {:?}", live_ps(ep));
        let mut no_settings = |_: &str, _: &str| Ok(true);
        let replaced = replace_model_impl(ep, old_alias, LIVE_BASE, &mut no_settings, LIVE_LIMIT)
            .expect("replace");
        println!("C replace -> {:?}", replaced);
        assert!(replaced.verified && replaced.deleted_old, "{:?}", replaced);
        assert_eq!(live_digest(ep, old_alias), None);
        std::thread::sleep(Duration::from_millis(500));
        let loaded = live_ps(ep);
        println!("C ps after: {:?}", loaded);
        assert!(
            !loaded.iter().any(|name| model_names_equal(name, old_alias)),
            "{:?}",
            loaded
        );
        assert_eq!(
            live_digest(ep, LIVE_OLD_BUILD).as_deref(),
            Some(old_build.as_str())
        );
        live.finish("different weights");
    }

    /// The time limit against a real cold load: it is honoured and reported as such, the server
    /// stays usable afterwards, and `finalize` treats it as "no verdict" (`inconclusive`): the new
    /// tag and the backup both stay, and a rollback brings the original back.
    #[test]
    #[ignore]
    fn live_a_cold_load_beyond_the_time_limit_is_inconclusive_and_can_be_rolled_back() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        live_copy(ep, LIVE_BASE, LIVE_TARGET);
        let original = live_digest(ep, LIVE_TARGET).expect("target installed");
        let backup = backup_model_impl(ep, LIVE_TARGET, crate::util::now_ms()).expect("backup");

        live_unload_all(ep, &[LIVE_BASE, LIVE_TARGET]);
        let started = Instant::now();
        let result = run_model_verification(ep, LIVE_TARGET, Duration::from_millis(600));
        println!(
            "600 ms limit, cold: {:?} after {:?}",
            result,
            started.elapsed()
        );
        assert!(!result.ok);
        assert!(
            result.inconclusive,
            "a time limit is no verdict: {:?}",
            result
        );
        assert!(
            result
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("Time limit of 600 ms exceeded"),
            "{:?}",
            result
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );

        // the aborted request does not wedge the server
        let fine = run_model_verification(ep, LIVE_TARGET, LIVE_LIMIT);
        assert!(fine.ok, "{:?}", fine);

        live_unload_all(ep, &[LIVE_BASE, LIVE_TARGET]);
        let finalized =
            finalize_model_update_impl(ep, LIVE_TARGET, &backup, Duration::from_millis(600))
                .expect("finalize");
        println!("finalize with a 600 ms limit, cold: {:?}", finalized);
        assert!(
            !finalized.verified && !finalized.restored && finalized.inconclusive,
            "{:?}",
            finalized
        );
        assert_eq!(
            live_digest(ep, LIVE_TARGET).as_deref(),
            Some(original.as_str()),
            "the new tag is untouched"
        );
        assert_eq!(
            live_digest(ep, &backup).as_deref(),
            Some(original.as_str()),
            "the backup is untouched"
        );

        let rolled_back = rollback_model_update_impl(ep, LIVE_TARGET, &backup).expect("rollback");
        println!("rollback -> {:?}", rolled_back);
        assert!(rolled_back.restored, "{:?}", rolled_back);
        assert_eq!(
            live_digest(ep, LIVE_TARGET).as_deref(),
            Some(original.as_str())
        );
        assert_eq!(live_digest(ep, &backup), None, "the backup is deleted last");
        live.finish("time limit");
    }

    /// The case the contract exists for: an update whose check cannot finish in time is kept as it
    /// is (new tag AND backup), even when the new build is in fact broken. Running the check again
    /// with enough time then fails it on its merits and restores the original; so does a manual
    /// rollback. At no point is a tag lost.
    #[test]
    #[ignore]
    fn live_inconclusive_check_keeps_both_tags_until_the_user_decides() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        live_copy(ep, LIVE_BASE, LIVE_TARGET);
        let original = live_digest(ep, LIVE_TARGET).expect("target installed");

        // phase 1: broken new build, check cannot finish -> both stay, then "Re-run check"
        let backup = backup_model_impl(ep, LIVE_TARGET, crate::util::now_ms()).expect("backup");
        live_assert_owned(&backup);
        live_create_broken(ep, LIVE_TARGET);
        let broken = live_digest(ep, LIVE_TARGET).expect("target installed");
        assert_ne!(broken, original, "the tag really was overwritten");
        live_unload_all(ep, &[LIVE_BASE, LIVE_TARGET]);

        let first =
            finalize_model_update_impl(ep, LIVE_TARGET, &backup, Duration::from_millis(300))
                .expect("finalize");
        println!("phase 1, 300 ms limit: {:?}", first);
        assert!(
            !first.verified && !first.restored && first.inconclusive,
            "{:?}",
            first
        );
        assert_eq!(
            live_digest(ep, LIVE_TARGET).as_deref(),
            Some(broken.as_str()),
            "new build kept"
        );
        assert_eq!(
            live_digest(ep, &backup).as_deref(),
            Some(original.as_str()),
            "backup kept"
        );

        let rerun =
            finalize_model_update_impl(ep, LIVE_TARGET, &backup, LIVE_LIMIT).expect("re-run");
        println!("phase 1, re-run with {:?}: {:?}", LIVE_LIMIT, rerun);
        assert!(
            !rerun.verified && rerun.restored && !rerun.inconclusive,
            "{:?}",
            rerun
        );
        assert_eq!(
            live_digest(ep, LIVE_TARGET).as_deref(),
            Some(original.as_str())
        );
        assert_eq!(live_digest(ep, &backup), None);

        // phase 2: inconclusive again, this time the user rolls back by hand
        let backup = backup_model_impl(ep, LIVE_TARGET, crate::util::now_ms()).expect("backup");
        live_create_broken(ep, LIVE_TARGET);
        live_unload_all(ep, &[LIVE_BASE, LIVE_TARGET]);
        let second =
            finalize_model_update_impl(ep, LIVE_TARGET, &backup, Duration::from_millis(300))
                .expect("finalize");
        assert!(second.inconclusive, "{:?}", second);
        let rolled_back = rollback_model_update_impl(ep, LIVE_TARGET, &backup).expect("rollback");
        println!("phase 2 rollback: {:?}", rolled_back);
        assert!(rolled_back.restored, "{:?}", rolled_back);
        assert_eq!(
            live_digest(ep, LIVE_TARGET).as_deref(),
            Some(original.as_str())
        );
        assert_eq!(live_digest(ep, &backup), None);
        let after = run_model_verification(ep, LIVE_TARGET, LIVE_LIMIT);
        assert!(after.ok, "the restored build works: {:?}", after);

        // a backup of another model is refused by the real server flow too
        assert!(
            rollback_model_update_impl(ep, LIVE_TARGET, "trispr-backup-trispr-live-other:1")
                .is_err()
        );
        live.finish("inconclusive");
    }

    /// Scenario 5: duration of the check with a cold runner and a warm one.
    #[test]
    #[ignore]
    fn live_verification_timing_cold_and_warm() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        let alias = "trispr-live-timing:latest";
        live_copy(ep, LIVE_BASE, alias);
        live_unload_all(ep, &[LIVE_BASE, alias]);

        let cold_started = Instant::now();
        let cold = run_model_verification(ep, alias, LIVE_LIMIT);
        let cold_total = cold_started.elapsed();
        let warm_started = Instant::now();
        let warm = run_model_verification(ep, alias, LIVE_LIMIT);
        let warm_total = warm_started.elapsed();
        println!(
            "cold: total {} ms, median request {} ms, load {} ms, ok {}, sample {:?}",
            cold_total.as_millis(),
            cold.latency_ms,
            cold.load_ms,
            cold.ok,
            cold.sample
        );
        println!(
            "warm: total {} ms, median request {} ms, load {} ms, ok {}, sample {:?}",
            warm_total.as_millis(),
            warm.latency_ms,
            warm.load_ms,
            warm.ok,
            warm.sample
        );
        assert!(cold.ok && warm.ok, "{:?} / {:?}", cold, warm);
        assert!(cold_total < LIVE_LIMIT && warm_total < cold_total);
        assert!(
            cold.load_ms > warm.load_ms,
            "the first request pays the load"
        );
        live.finish("scenario 5");
    }

    /// Read-only: the user's old build is a different model blob (its own runner). Only chats.
    #[test]
    #[ignore]
    fn live_verify_the_old_build_read_only() {
        let Some(live) = Live::start() else { return };
        let ep = live.endpoint.as_str();
        if live_digest(ep, LIVE_OLD_BUILD).is_none() {
            eprintln!("{} is not installed - skipping", LIVE_OLD_BUILD);
            return;
        }
        let before = live_digest(ep, LIVE_OLD_BUILD);
        let started = Instant::now();
        let result = run_model_verification(ep, LIVE_OLD_BUILD, LIVE_LIMIT);
        println!(
            "old build: total {} ms -> {:?}; ps {:?}",
            started.elapsed().as_millis(),
            result,
            live_ps(ep)
        );
        assert_eq!(live_digest(ep, LIVE_OLD_BUILD), before, "read-only");
        live.finish("old build");
    }
}
