use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{info, warn};

use crate::history_partition::PartitionedHistory;
use crate::modules::{normalize_confluence_settings, ASSISTANT_CORE_MODULE_ID};
use crate::overlay::update_overlay_refining_indicator;
use crate::state::{
    save_settings_file, AppState, AssistantOrchestratorState, HistoryEntry, Settings,
};
use crate::weather;
use crate::{capability_enabled, guarded_command, require_capability_enabled, RuntimeCapability};

static ASSISTANT_CONFIRM_TOKEN_SEQ: AtomicU64 = AtomicU64::new(1_000);

#[derive(Debug, Clone)]
struct PendingAssistantConfirmation {
    plan: AgentExecutionPlan,
    confirm_token: String,
    expires_at_ms: u64,
}

static ASSISTANT_PENDING_CONFIRMATION: Mutex<Option<PendingAssistantConfirmation>> =
    Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentParseCommandRequest {
    pub command_text: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCommandParseResult {
    pub detected: bool,
    pub intent: String, // "gdd_generate_publish" | "unknown"
    pub confidence: f32,
    pub publish_requested: bool,
    pub wakeword_matched: bool,
    pub temporal_hint: Option<String>,
    pub topic_hint: Option<String>,
    pub reasoning: String,
    pub command_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchTranscriptSessionsRequest {
    pub temporal_hint: Option<String>,
    pub topic_hint: Option<String>,
    pub session_gap_minutes: Option<u32>,
    pub max_candidates: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSessionCandidate {
    pub session_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub entry_count: usize,
    pub source_mix: Vec<String>,
    pub preview: String,
    pub score: f32,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionStep {
    pub id: String,
    pub title: String,
    pub status: String, // "pending" | "running" | "done" | "failed"
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBuildExecutionPlanRequest {
    pub intent: String,
    pub session_id: String,
    pub target_language: String, // "source" | "en" | ...
    pub publish: bool,
    pub command_text: Option<String>,
    pub temporal_hint: Option<String>,
    pub topic_hint: Option<String>,
    pub parse_confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionPlan {
    pub intent: String,
    pub session_id: String,
    pub session_title: String,
    pub target_language: String,
    pub publish: bool,
    #[serde(default)]
    pub analysis_steps: Vec<AgentExecutionStep>,
    #[serde(default)]
    pub execution_steps: Vec<AgentExecutionStep>,
    pub steps: Vec<AgentExecutionStep>,
    #[serde(default)]
    pub recognized_signals: Vec<String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub proposed_actions: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecuteGddPlanRequest {
    pub plan: AgentExecutionPlan,
    pub title: Option<String>,
    pub preset_id: Option<String>,
    pub max_chunk_chars: Option<usize>,
    pub space_key: Option<String>,
    pub parent_page_id: Option<String>,
    pub target_page_id: Option<String>,
    #[serde(default)]
    pub confirmation_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionResult {
    pub status: String, // "completed" | "queued" | "failed" | "cancelled"
    pub message: String,
    pub draft: Option<crate::gdd::GddDraft>,
    pub publish_result: Option<crate::gdd::confluence::ConfluencePublishResult>,
    pub queued_job: Option<crate::gdd::publish_queue::GddPendingPublishJob>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTranscriptionEvent {
    pub text: String,
    pub source: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SessionBucket {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub entries: Vec<HistoryEntry>,
}

fn collect_partitioned_entries(history: &PartitionedHistory) -> Vec<HistoryEntry> {
    let mut out = Vec::new();
    for partition in history.list_partitions() {
        if let Ok(key) = crate::history_partition::PartitionKey::parse(&partition.key) {
            out.extend(history.load_partition(&key));
        }
    }
    out
}

fn collect_all_transcript_entries(state: &AppState) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    {
        let history = state
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.extend(collect_partitioned_entries(&history));
    }
    {
        let history = state
            .history_transcribe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.extend(collect_partitioned_entries(&history));
    }
    entries.sort_by_key(|entry| entry.timestamp_ms);
    entries
}

fn normalize(text: &str) -> String {
    let mut cleaned = String::with_capacity(text.len());
    for ch in text.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() || ch.is_whitespace() {
            cleaned.push(ch);
        } else {
            cleaned.push(' ');
        }
    }
    cleaned
        .split_whitespace()
        .map(|token| match token {
            "trispa" | "trisper" | "trispar" | "trispur" => "trispr",
            _ => token,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_any(text: &str, words: &[String]) -> bool {
    words
        .iter()
        .any(|word| !word.trim().is_empty() && text.contains(&normalize(word)))
}

fn detect_temporal_hint(text: &str) -> Option<String> {
    let normalized = normalize(text);
    let hints = [
        "vorgestern",
        "gestern",
        "heute",
        "vorhin",
        "today",
        "yesterday",
        "day before yesterday",
        "earlier",
        "recently",
    ];
    hints
        .iter()
        .find(|hint| normalized.contains(**hint))
        .map(|hint| (*hint).to_string())
}

fn detect_topic_hint(text: &str) -> Option<String> {
    let lower = normalize(text);
    let separators = [" ueber ", " über ", " about ", " regarding ", " zu "];
    for separator in separators {
        if let Some(pos) = lower.find(separator) {
            let tail = lower[pos + separator.len()..].trim();
            if tail.is_empty() {
                continue;
            }
            let cleaned = tail
                .split(['.', ',', ';', ':', '!', '?'])
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }
    None
}

fn parse_publish_requested(text: &str) -> bool {
    let normalized = normalize(text);
    let publish_words = [
        "publish",
        "post",
        "confluence",
        "veroeffentlichen",
        "veröffentlichen",
        "posten",
        "hochladen",
    ];
    publish_words.iter().any(|word| normalized.contains(word))
}

fn relative_day_hint_score(hint: &str, start_ms: u64, now_ms: u64) -> f32 {
    let start = Utc.timestamp_millis_opt(start_ms as i64).single();
    let now = Utc.timestamp_millis_opt(now_ms as i64).single();
    let (Some(start), Some(now)) = (start, now) else {
        return 0.0;
    };
    let start_date = start.date_naive();
    let now_date = now.date_naive();
    let target_date = if hint.contains("vorgestern") || hint.contains("day before yesterday") {
        now_date - ChronoDuration::days(2)
    } else if hint.contains("gestern") || hint.contains("yesterday") {
        now_date - ChronoDuration::days(1)
    } else if hint.contains("heute") || hint.contains("today") {
        now_date
    } else if hint.contains("vorhin") || hint.contains("earlier") || hint.contains("recently") {
        return recency_score(start_ms, now_ms);
    } else {
        return 0.0;
    };

    if start_date == target_date {
        1.0
    } else {
        0.0
    }
}

fn recency_score(start_ms: u64, now_ms: u64) -> f32 {
    if now_ms <= start_ms {
        return 1.0;
    }
    let delta_hours = (now_ms - start_ms) as f32 / (1000.0 * 60.0 * 60.0);
    (1.0 / (1.0 + (delta_hours / 12.0))).clamp(0.0, 1.0)
}

fn topic_score(topic_hint: Option<&str>, entries: &[HistoryEntry]) -> f32 {
    let Some(topic) = topic_hint else {
        return 0.0;
    };
    let hint = normalize(topic);
    if hint.is_empty() {
        return 0.0;
    }
    let mut matches = 0usize;
    let mut total = 0usize;
    for entry in entries {
        total += 1;
        let text = normalize(&entry.text);
        if text.contains(&hint) {
            matches += 1;
        }
    }
    if total == 0 {
        0.0
    } else {
        (matches as f32 / total as f32).clamp(0.0, 1.0)
    }
}

fn tokenize_for_similarity(text: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "the", "and", "for", "with", "that", "this", "from", "have", "will", "into", "about", "de",
        "der", "die", "das", "und", "mit", "ist", "ein", "eine", "den", "dem", "des", "wir", "ihr",
        "sie", "ich", "du", "zu", "auf", "von", "im", "in", "am", "an", "oder", "aber",
    ];
    text.to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 3)
        .filter(|token| !STOPWORDS.contains(token))
        .map(str::to_string)
        .collect()
}

fn lexical_overlap_score(left: &str, right: &str) -> f32 {
    let left_tokens: HashSet<String> = tokenize_for_similarity(left).into_iter().collect();
    let right_tokens: HashSet<String> = tokenize_for_similarity(right).into_iter().collect();
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0.0;
    }
    let intersection = left_tokens.intersection(&right_tokens).count() as f32;
    let union = left_tokens.union(&right_tokens).count() as f32;
    if union <= 0.0 {
        0.0
    } else {
        (intersection / union).clamp(0.0, 1.0)
    }
}

fn entry_continuation_score(previous: &HistoryEntry, next: &HistoryEntry) -> f32 {
    let lexical = lexical_overlap_score(&previous.text, &next.text);
    let source_switch = if previous.source == next.source {
        0.5
    } else {
        1.0
    };
    let gap_minutes =
        next.timestamp_ms.saturating_sub(previous.timestamp_ms) as f32 / (1000.0 * 60.0);
    let gap_score = (1.0 / (1.0 + (gap_minutes / 20.0))).clamp(0.0, 1.0);
    (lexical * 0.45 + source_switch * 0.25 + gap_score * 0.30).clamp(0.0, 1.0)
}

fn session_continuity_score(entries: &[HistoryEntry]) -> f32 {
    if entries.len() < 2 {
        return 0.0;
    }
    let mut source_switch_acc = 0.0f32;
    let mut lexical_acc = 0.0f32;
    let mut gap_acc = 0.0f32;
    let mut pairs = 0.0f32;

    for window in entries.windows(2) {
        let previous = &window[0];
        let next = &window[1];
        pairs += 1.0;
        source_switch_acc += if previous.source == next.source {
            0.0
        } else {
            1.0
        };
        lexical_acc += lexical_overlap_score(&previous.text, &next.text);
        let gap_minutes =
            next.timestamp_ms.saturating_sub(previous.timestamp_ms) as f32 / (1000.0 * 60.0);
        gap_acc += (1.0 / (1.0 + (gap_minutes / 3.0))).clamp(0.0, 1.0);
    }

    if pairs <= 0.0 {
        return 0.0;
    }
    let source_switch_ratio = source_switch_acc / pairs;
    let lexical_ratio = lexical_acc / pairs;
    let gap_ratio = gap_acc / pairs;
    (source_switch_ratio * 0.35 + lexical_ratio * 0.30 + gap_ratio * 0.35).clamp(0.0, 1.0)
}

fn archive_context_score(session: &SessionBucket) -> f32 {
    let entry_count = session.entries.len() as f32;
    if entry_count <= 0.0 {
        return 0.0;
    }
    let duration_minutes =
        ((session.end_ms.saturating_sub(session.start_ms)) as f32 / (1000.0 * 60.0)).max(1.0);
    let richness = (entry_count / 12.0).clamp(0.0, 1.0);
    let duration_coverage = (duration_minutes / 20.0).clamp(0.0, 1.0);
    let density = ((entry_count / duration_minutes) / 3.0).clamp(0.0, 1.0);
    let unique_sources = session
        .entries
        .iter()
        .map(|entry| entry.source.as_str())
        .collect::<HashSet<_>>()
        .len();
    let source_diversity = if unique_sources >= 3 {
        1.0
    } else if unique_sources == 2 {
        0.8
    } else if unique_sources == 1 {
        0.3
    } else {
        0.0
    };
    (richness * 0.35 + duration_coverage * 0.2 + density * 0.2 + source_diversity * 0.25)
        .clamp(0.0, 1.0)
}

fn session_preview(entries: &[HistoryEntry]) -> String {
    let mut joined = entries
        .iter()
        .take(4)
        .map(|entry| entry.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    joined = joined.trim().to_string();
    if joined.len() > 180 {
        let mut end = 180;
        while !joined.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        joined.truncate(end);
        joined.push_str("...");
    }
    joined
}

pub fn parse_command(
    request: &AgentParseCommandRequest,
    wakewords: &[String],
    intent_keywords: &[String],
) -> AgentCommandParseResult {
    let text = request.command_text.trim().to_string();
    let normalized = normalize(&text);
    let wakeword_matched = contains_any(&normalized, wakewords);
    let gdd_keyword_hits = intent_keywords
        .iter()
        .filter(|keyword| {
            let normalized_keyword = normalize(keyword);
            !normalized_keyword.is_empty() && normalized.contains(&normalized_keyword)
        })
        .count();
    let recap_keywords = [
        "recap",
        "summary",
        "zusammenfassung",
        "fasse zusammen",
        "session recap",
    ];
    let web_search_keywords = [
        "search",
        "search for",
        "look up",
        "google",
        "weather",
        "wetter",
        "news",
        "online",
    ];
    let open_module_keywords = [
        "open gdd",
        "open settings",
        "open modules",
        "open module",
        "open voice output",
        "open tts",
        "open refinement",
        "open assistant",
        "open agent tab",
        "show settings",
    ];
    let open_app_keywords = [
        "open explorer",
        "open files",
        "open browser",
        "open chrome",
        "open edge",
        "open cursor",
        "open terminal",
        "open notepad",
        "open calculator",
        "start explorer",
        "launch cursor",
    ];
    let plan_status_keywords = [
        "plan status",
        "status",
        "status update",
        "planstand",
        "was ist der plan",
    ];
    let confirm_cancel_keywords = [
        "confirm",
        "cancel",
        "bestätigen",
        "bestaetigen",
        "abbrechen",
        "freigeben",
    ];
    let reminder_keywords = [
        "erinnere mich",
        "trag ein",
        "trag auf liste",
        "auf meine liste",
        "auf die agenda",
        "auf meine todo liste",
        "add to list",
        "add to my agenda",
        "put on my todo",
        "remind me",
    ];

    let recap_hit = recap_keywords
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let web_search_hit = web_search_keywords
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let open_module_hit = open_module_keywords
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let open_app_hit = open_app_keywords
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let plan_status_hit = plan_status_keywords
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let confirm_cancel_hit = confirm_cancel_keywords
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let reminder_hit = reminder_keywords
        .iter()
        .any(|keyword| normalized.contains(keyword));

    let (intent_detected, intent) = if wakeword_matched && confirm_cancel_hit {
        (true, "confirm_or_cancel".to_string())
    } else if wakeword_matched && plan_status_hit {
        (true, "plan_status".to_string())
    } else if wakeword_matched && recap_hit {
        (true, "session_recap".to_string())
    } else if wakeword_matched && reminder_hit {
        (true, "reminder_capture".to_string())
    } else if wakeword_matched && open_module_hit {
        (true, "open_module".to_string())
    } else if wakeword_matched && open_app_hit {
        (true, "open_app".to_string())
    } else if wakeword_matched && web_search_hit {
        (true, "web_search".to_string())
    } else if wakeword_matched && gdd_keyword_hits > 0 {
        (true, "gdd_generate_publish".to_string())
    } else {
        (false, "unknown".to_string())
    };
    let temporal_hint = detect_temporal_hint(&normalized);
    let topic_hint = detect_topic_hint(&normalized);
    let publish_requested = parse_publish_requested(&normalized);
    let publish_requested_for_intent =
        intent_detected && intent.as_str() == "gdd_generate_publish" && publish_requested;

    let mut confidence: f32 = 0.0;
    if wakeword_matched {
        confidence += 0.45;
    }
    if gdd_keyword_hits > 0
        || recap_hit
        || plan_status_hit
        || confirm_cancel_hit
        || reminder_hit
        || web_search_hit
        || open_module_hit
        || open_app_hit
    {
        confidence += 0.35;
    }
    if temporal_hint.is_some() {
        confidence += 0.1;
    }
    if topic_hint.is_some() {
        confidence += 0.1;
    }

    AgentCommandParseResult {
        detected: intent_detected,
        intent,
        confidence: confidence.clamp(0.0, 1.0),
        publish_requested: publish_requested_for_intent,
        wakeword_matched,
        temporal_hint,
        topic_hint,
        reasoning: format!(
            "wakeword={}, gdd_keywords={}, recap={}, plan_status={}, confirm_cancel={}, reminder={}, web_search={}, open_module={}, open_app={}, publish_hint={}",
            wakeword_matched,
            gdd_keyword_hits,
            recap_hit,
            plan_status_hit,
            confirm_cancel_hit,
            reminder_hit,
            web_search_hit,
            open_module_hit,
            open_app_hit,
            publish_requested
        ),
        command_text: text,
    }
}

pub fn build_sessions(entries: &[HistoryEntry], session_gap_minutes: u32) -> Vec<SessionBucket> {
    if entries.is_empty() {
        return Vec::new();
    }

    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|entry| entry.timestamp_ms);

    let gap_ms = (session_gap_minutes.max(1) as u64) * 60_000;
    let adaptive_gap_ms = gap_ms.saturating_mul(2);
    let mut sessions: Vec<SessionBucket> = Vec::new();
    let mut current_entries: Vec<HistoryEntry> = Vec::new();
    let mut current_start = sorted[0].timestamp_ms;
    let mut current_end = sorted[0].timestamp_ms;

    for entry in sorted {
        if current_entries.is_empty() {
            current_start = entry.timestamp_ms;
            current_end = entry.timestamp_ms;
            current_entries.push(entry);
            continue;
        }

        let gap_since_last_ms = entry.timestamp_ms.saturating_sub(current_end);
        let should_split = if gap_since_last_ms <= gap_ms {
            false
        } else if gap_since_last_ms <= adaptive_gap_ms {
            if let Some(previous) = current_entries.last() {
                entry_continuation_score(previous, &entry) < 0.58
            } else {
                true
            }
        } else {
            true
        };

        if should_split {
            let id = format!("s_{}_{}", current_start, current_end);
            sessions.push(SessionBucket {
                id,
                start_ms: current_start,
                end_ms: current_end,
                entries: std::mem::take(&mut current_entries),
            });
            current_start = entry.timestamp_ms;
        }
        current_end = entry.timestamp_ms;
        current_entries.push(entry);
    }

    if !current_entries.is_empty() {
        let id = format!("s_{}_{}", current_start, current_end);
        sessions.push(SessionBucket {
            id,
            start_ms: current_start,
            end_ms: current_end,
            entries: current_entries,
        });
    }

    sessions.sort_by(|a, b| b.start_ms.cmp(&a.start_ms));
    sessions
}

pub fn score_sessions(
    sessions: &[SessionBucket],
    request: &SearchTranscriptSessionsRequest,
) -> Vec<TranscriptSessionCandidate> {
    let now_ms = crate::util::now_ms();
    let temporal_hint = request.temporal_hint.as_deref().map(normalize);
    let topic_hint = request.topic_hint.as_deref().map(normalize);
    let max_candidates = request.max_candidates.unwrap_or(3).clamp(1, 5) as usize;

    let mut scored: Vec<TranscriptSessionCandidate> = sessions
        .iter()
        .map(|session| {
            let temporal = temporal_hint
                .as_ref()
                .map(|hint| relative_day_hint_score(hint, session.start_ms, now_ms))
                .unwrap_or(0.0);
            let topic = topic_score(topic_hint.as_deref(), &session.entries);
            let recency = recency_score(session.start_ms, now_ms);
            let continuity = session_continuity_score(&session.entries);
            let archive_context = archive_context_score(session);
            let score = (temporal * 0.34
                + topic * 0.26
                + recency * 0.18
                + continuity * 0.12
                + archive_context * 0.10)
                .clamp(0.0, 1.0);

            let mut source_mix = session
                .entries
                .iter()
                .map(|entry| entry.source.clone())
                .collect::<Vec<_>>();
            source_mix.sort();
            source_mix.dedup();

            TranscriptSessionCandidate {
                session_id: session.id.clone(),
                start_ms: session.start_ms,
                end_ms: session.end_ms,
                entry_count: session.entries.len(),
                source_mix,
                preview: session_preview(&session.entries),
                score,
                reasoning: format!(
                    "temporal={:.2}, topic={:.2}, recency={:.2}, continuity={:.2}, archive={:.2}",
                    temporal, topic, recency, continuity, archive_context
                ),
            }
        })
        .collect();

    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    scored.truncate(max_candidates);
    scored
}

pub fn default_execution_plan(request: &AgentBuildExecutionPlanRequest) -> AgentExecutionPlan {
    let mut recognized_signals = vec![
        format!("intent={}", request.intent),
        format!("session_id={}", request.session_id),
        format!("target_language={}", request.target_language),
        format!("publish_requested={}", request.publish),
    ];
    if let Some(confidence) = request.parse_confidence {
        recognized_signals.push(format!(
            "parse_confidence={:.2}",
            confidence.clamp(0.0, 1.0)
        ));
    }
    if let Some(topic_hint) = request.topic_hint.as_deref().map(str::trim) {
        if !topic_hint.is_empty() {
            recognized_signals.push(format!("topic_hint={topic_hint}"));
        }
    }
    if let Some(temporal_hint) = request.temporal_hint.as_deref().map(str::trim) {
        if !temporal_hint.is_empty() {
            recognized_signals.push(format!("temporal_hint={temporal_hint}"));
        }
    }
    if let Some(command_text) = request.command_text.as_deref().map(str::trim) {
        if !command_text.is_empty() {
            let mut excerpt = command_text.to_string();
            if excerpt.len() > 140 {
                excerpt.truncate(140);
                excerpt.push_str("...");
            }
            recognized_signals.push(format!("command_excerpt=\"{excerpt}\""));
        }
    }

    let assumptions = vec![
        if request
            .topic_hint
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            "No explicit topic hint provided; transcript ranking determines dominant topic."
                .to_string()
        } else {
            "Topic hint is treated as primary retrieval guidance.".to_string()
        },
        if request
            .temporal_hint
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            "No temporal hint provided; recent sessions are preferred.".to_string()
        } else {
            "Temporal hint is used to prioritize matching session day.".to_string()
        },
        if request.publish {
            "Publish was requested; execution requires explicit confirmation before side effects."
                .to_string()
        } else {
            "Publish not requested; execution should remain draft-only unless user confirms publish."
                .to_string()
        },
    ];

    let proposed_actions = vec![
        "Review selected session context and confirm it matches the intended conversation."
            .to_string(),
        format!(
            "Generate a {} GDD draft from the session transcript.",
            request.target_language
        ),
        if request.publish {
            "After review, publish to Confluence or queue fallback if publishing is unavailable."
                .to_string()
        } else {
            "Present draft for review and wait for explicit publish decision.".to_string()
        },
    ];

    let analysis_steps = vec![
        AgentExecutionStep {
            id: "load_session".to_string(),
            title: "Load transcript session".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
        AgentExecutionStep {
            id: "generate_draft".to_string(),
            title: "Generate GDD draft".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
    ];
    let execution_steps = if request.publish {
        vec![AgentExecutionStep {
            id: "publish_or_queue".to_string(),
            title: "Publish to Confluence or queue fallback".to_string(),
            status: "pending".to_string(),
            detail: None,
        }]
    } else {
        Vec::new()
    };
    let mut steps = analysis_steps.clone();
    steps.extend(execution_steps.clone());

    AgentExecutionPlan {
        intent: request.intent.clone(),
        session_id: request.session_id.clone(),
        session_title: format!("Session {}", request.session_id),
        target_language: request.target_language.clone(),
        publish: request.publish,
        analysis_steps,
        execution_steps,
        steps,
        recognized_signals,
        assumptions,
        proposed_actions,
        summary: format!(
            "Intent={} session={} target_language={} publish={}",
            request.intent, request.session_id, request.target_language, request.publish
        ),
    }
}

pub fn should_publish_after_draft(plan: &AgentExecutionPlan) -> bool {
    if !plan.publish {
        return false;
    }
    let has_execution_publish_step = plan
        .execution_steps
        .iter()
        .any(|step| step.id == "publish_or_queue");
    if has_execution_publish_step {
        return true;
    }
    // Compatibility lane: accept older plans that only carry `steps`.
    plan.steps.iter().any(|step| step.id == "publish_or_queue")
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct AssistantCapabilitySnapshot {
    product_mode: String,
    assistant_mode: bool,
    assistant_core_available: bool,
    workflow_agent_available: bool,
    tts_available: bool,
    vision_available: bool,
    degraded: bool,
    hard_blocked: bool,
    missing_capabilities: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct AssistantStateChangedEvent {
    state: AssistantOrchestratorState,
    previous_state: AssistantOrchestratorState,
    reason: String,
    transition_id: u64,
    changed_at_ms: u64,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantPlanReadyEvent {
    state: AssistantOrchestratorState,
    reason: String,
    plan: crate::workflow_agent::AgentExecutionPlan,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantIntentDetectedEvent {
    state: AssistantOrchestratorState,
    reason: String,
    parse: crate::workflow_agent::AgentCommandParseResult,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantAwaitingConfirmationEvent {
    state: AssistantOrchestratorState,
    reason: String,
    plan: crate::workflow_agent::AgentExecutionPlan,
    confirm_token: String,
    confirm_timeout_sec: u16,
    expires_at_ms: u64,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantConfirmationExpiredEvent {
    state: AssistantOrchestratorState,
    reason: String,
    expired_at_ms: u64,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantActionResultEvent {
    state: AssistantOrchestratorState,
    reason: String,
    result: crate::workflow_agent::AgentExecutionResult,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug)]
enum PendingConfirmationError {
    Missing,
    Expired { expired_at_ms: u64 },
    TokenMismatch,
    PlanMismatch,
}

fn next_confirmation_token() -> String {
    let seq = ASSISTANT_CONFIRM_TOKEN_SEQ.fetch_add(1, Ordering::Relaxed);
    let code = (seq % 9_000) + 1_000;
    format!("{code:04}")
}

fn normalize_confirmation_token(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn plans_match_for_confirmation(
    left: &crate::workflow_agent::AgentExecutionPlan,
    right: &crate::workflow_agent::AgentExecutionPlan,
) -> bool {
    left.intent == right.intent
        && left.session_id == right.session_id
        && left.target_language == right.target_language
        && left.publish == right.publish
}

fn register_pending_confirmation(
    plan: &crate::workflow_agent::AgentExecutionPlan,
    confirm_timeout_sec: u16,
) -> (String, u64) {
    let token = next_confirmation_token();
    let expires_at_ms = crate::util::now_ms().saturating_add(confirm_timeout_sec as u64 * 1_000);
    let mut pending = ASSISTANT_PENDING_CONFIRMATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *pending = Some(PendingAssistantConfirmation {
        plan: plan.clone(),
        confirm_token: token.clone(),
        expires_at_ms,
    });
    (token, expires_at_ms)
}

fn clear_pending_confirmation() {
    let mut pending = ASSISTANT_PENDING_CONFIRMATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *pending = None;
}

fn clear_pending_confirmation_for_plan(plan: &crate::workflow_agent::AgentExecutionPlan) {
    let mut pending = ASSISTANT_PENDING_CONFIRMATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if pending
        .as_ref()
        .map(|entry| plans_match_for_confirmation(&entry.plan, plan))
        .unwrap_or(false)
    {
        *pending = None;
    }
}

fn consume_pending_confirmation(
    plan: &crate::workflow_agent::AgentExecutionPlan,
    token: &str,
) -> Result<(), PendingConfirmationError> {
    let now_ms = crate::util::now_ms();
    let mut pending = ASSISTANT_PENDING_CONFIRMATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(current) = pending.as_ref() else {
        return Err(PendingConfirmationError::Missing);
    };
    if now_ms > current.expires_at_ms {
        let expired_at_ms = current.expires_at_ms;
        *pending = None;
        return Err(PendingConfirmationError::Expired { expired_at_ms });
    }
    if !plans_match_for_confirmation(&current.plan, plan) {
        return Err(PendingConfirmationError::PlanMismatch);
    }
    if normalize_confirmation_token(token)
        != normalize_confirmation_token(current.confirm_token.as_str())
    {
        return Err(PendingConfirmationError::TokenMismatch);
    }
    *pending = None;
    Ok(())
}

fn assistant_product_mode(settings: &Settings) -> &'static str {
    if settings
        .product_mode
        .trim()
        .eq_ignore_ascii_case("assistant")
    {
        "assistant"
    } else {
        "transcribe"
    }
}

fn assistant_capability_snapshot(settings: &Settings) -> AssistantCapabilitySnapshot {
    let assistant_mode = assistant_product_mode(settings) == "assistant";
    let assistant_core_available = capability_enabled(settings, RuntimeCapability::WorkflowAgent);
    let tts_available = capability_enabled(settings, RuntimeCapability::VoiceOutputTts);
    let vision_available = capability_enabled(settings, RuntimeCapability::VisionInput);
    let mut missing_capabilities: Vec<String> = Vec::new();
    if !assistant_mode {
        missing_capabilities.push("product_mode_assistant".to_string());
    }
    if !assistant_core_available {
        missing_capabilities.push(ASSISTANT_CORE_MODULE_ID.to_string());
    }
    if !tts_available {
        missing_capabilities.push("output_voice_tts".to_string());
    }
    if !vision_available {
        missing_capabilities.push("input_vision".to_string());
    }
    AssistantCapabilitySnapshot {
        product_mode: assistant_product_mode(settings).to_string(),
        assistant_mode,
        assistant_core_available,
        workflow_agent_available: assistant_core_available,
        tts_available,
        vision_available,
        degraded: assistant_mode
            && assistant_core_available
            && (!tts_available || !vision_available),
        hard_blocked: !assistant_mode || !assistant_core_available,
        missing_capabilities,
    }
}

fn assistant_baseline_state(
    capability: &AssistantCapabilitySnapshot,
) -> (AssistantOrchestratorState, &'static str) {
    if !capability.assistant_mode {
        return (AssistantOrchestratorState::Idle, "product_mode_transcribe");
    }
    if !capability.assistant_core_available {
        return (
            AssistantOrchestratorState::Idle,
            "assistant_core_unavailable",
        );
    }
    if capability.degraded {
        return (
            AssistantOrchestratorState::Listening,
            "assistant_degraded_capability",
        );
    }
    (AssistantOrchestratorState::Listening, "assistant_ready")
}

fn transition_assistant_state_with_settings(
    app: &AppHandle,
    state: &AppState,
    settings: &Settings,
    next_state: AssistantOrchestratorState,
    reason: impl Into<String>,
) -> AssistantStateChangedEvent {
    let reason = reason.into();
    let capability = assistant_capability_snapshot(settings);
    let now_ms = crate::util::now_ms();
    let (previous_state, changed_at_ms, transition_id) = {
        let mut tracker = state
            .assistant_orchestrator
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_state = tracker.state;
        let has_changed = tracker.state != next_state || tracker.last_reason != reason;
        if has_changed {
            tracker.transition_id = tracker.transition_id.saturating_add(1);
            tracker.changed_at_ms = now_ms;
            tracker.state = next_state;
            tracker.last_reason = reason.clone();
        }
        (
            previous_state,
            if has_changed {
                tracker.changed_at_ms
            } else if tracker.changed_at_ms == 0 {
                now_ms
            } else {
                tracker.changed_at_ms
            },
            tracker.transition_id,
        )
    };

    let payload = AssistantStateChangedEvent {
        state: next_state,
        previous_state,
        reason,
        transition_id,
        changed_at_ms,
        capability,
    };
    let _ = app.emit("assistant:state-changed", &payload);
    payload
}

pub(crate) fn emit_assistant_baseline_state(
    app: &AppHandle,
    state: &AppState,
    settings: &Settings,
    trigger: &str,
) -> AssistantStateChangedEvent {
    let capability = assistant_capability_snapshot(settings);
    let (baseline_state, baseline_reason) = assistant_baseline_state(&capability);
    clear_pending_confirmation();
    let reason = if trigger.trim().is_empty() {
        baseline_reason.to_string()
    } else {
        format!("{}:{}", trigger.trim(), baseline_reason)
    };
    transition_assistant_state_with_settings(app, state, settings, baseline_state, reason)
}

pub(crate) fn emit_assistant_runtime_state_from_current_settings(
    app: &AppHandle,
    state: &AppState,
    trigger: &str,
) -> AssistantStateChangedEvent {
    let settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    emit_assistant_baseline_state(app, state, &settings_snapshot, trigger)
}

fn emit_assistant_plan_ready(
    app: &AppHandle,
    settings: &Settings,
    plan: &crate::workflow_agent::AgentExecutionPlan,
    reason: &str,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let payload = AssistantPlanReadyEvent {
        state: AssistantOrchestratorState::AwaitingConfirm,
        reason: reason.to_string(),
        plan: plan.clone(),
        capability,
    };
    let _ = app.emit("assistant:plan-ready", &payload);
}

fn emit_assistant_intent_detected(
    app: &AppHandle,
    settings: &Settings,
    parse: &crate::workflow_agent::AgentCommandParseResult,
    reason: &str,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let payload = AssistantIntentDetectedEvent {
        state: AssistantOrchestratorState::Parsing,
        reason: reason.to_string(),
        parse: parse.clone(),
        capability,
    };
    let _ = app.emit("assistant:intent-detected", &payload);
}

fn emit_assistant_awaiting_confirmation(
    app: &AppHandle,
    settings: &Settings,
    plan: &crate::workflow_agent::AgentExecutionPlan,
    reason: &str,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let confirm_timeout_sec = settings.workflow_agent.confirm_timeout_sec.clamp(10, 300);
    let (confirm_token, expires_at_ms) = register_pending_confirmation(plan, confirm_timeout_sec);
    let payload = AssistantAwaitingConfirmationEvent {
        state: AssistantOrchestratorState::AwaitingConfirm,
        reason: reason.to_string(),
        plan: plan.clone(),
        confirm_token,
        confirm_timeout_sec,
        expires_at_ms,
        capability,
    };
    let _ = app.emit("assistant:awaiting-confirmation", &payload);
}

fn emit_assistant_confirmation_expired(
    app: &AppHandle,
    settings: &Settings,
    reason: &str,
    expired_at_ms: u64,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let payload = AssistantConfirmationExpiredEvent {
        state: AssistantOrchestratorState::Recovering,
        reason: reason.to_string(),
        expired_at_ms,
        capability,
    };
    let _ = app.emit("assistant:confirmation-expired", &payload);
}

fn expire_pending_confirmation_if_needed(
    app: &AppHandle,
    state: &AppState,
    settings: &Settings,
    trigger: &str,
) -> bool {
    let now_ms = crate::util::now_ms();
    let expired_at_ms = {
        let mut pending = ASSISTANT_PENDING_CONFIRMATION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match pending.as_ref() {
            Some(entry) if now_ms > entry.expires_at_ms => {
                let expired_at_ms = entry.expires_at_ms;
                *pending = None;
                Some(expired_at_ms)
            }
            _ => None,
        }
    };
    if let Some(expired_at_ms) = expired_at_ms {
        emit_assistant_confirmation_expired(
            app,
            settings,
            &format!("{}:timeout", trigger),
            expired_at_ms,
        );
        let _ = emit_assistant_baseline_state(app, state, settings, trigger);
        return true;
    }
    false
}

fn emit_assistant_action_result(
    app: &AppHandle,
    settings: &Settings,
    state: AssistantOrchestratorState,
    result: &crate::workflow_agent::AgentExecutionResult,
    reason: &str,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let payload = AssistantActionResultEvent {
        state,
        reason: reason.to_string(),
        result: result.clone(),
        capability,
    };
    let _ = app.emit("assistant:action-result", &payload);
}

#[cfg(test)]
mod assistant_orchestrator_tests {
    use super::{assistant_baseline_state, assistant_capability_snapshot};
    use crate::state::{AssistantOrchestratorState, Settings};
    use crate::RuntimeCapability;

    fn settings_for_assistant_mode(
        product_mode: &str,
        workflow_enabled: bool,
        tts_enabled: bool,
        vision_enabled: bool,
    ) -> Settings {
        let mut settings = Settings::default();
        settings.product_mode = product_mode.to_string();

        if workflow_enabled {
            settings
                .module_settings
                .enabled_modules
                .insert(RuntimeCapability::WorkflowAgent.module_id().to_string());
            settings.workflow_agent.enabled = true;
        }
        if tts_enabled {
            settings
                .module_settings
                .enabled_modules
                .insert(RuntimeCapability::VoiceOutputTts.module_id().to_string());
            settings.voice_output_settings.enabled = true;
        }
        if vision_enabled {
            settings
                .module_settings
                .enabled_modules
                .insert(RuntimeCapability::VisionInput.module_id().to_string());
            settings.vision_input_settings.enabled = true;
        }

        settings
    }

    #[test]
    fn assistant_baseline_is_idle_when_product_mode_is_transcribe() {
        let settings = settings_for_assistant_mode("transcribe", true, true, true);
        let capability = assistant_capability_snapshot(&settings);
        assert!(!capability.assistant_mode);
        assert!(capability.hard_blocked);
        let (state, reason) = assistant_baseline_state(&capability);
        assert_eq!(state, AssistantOrchestratorState::Idle);
        assert_eq!(reason, "product_mode_transcribe");
    }

    #[test]
    fn assistant_baseline_is_listening_with_degraded_capabilities() {
        let settings = settings_for_assistant_mode("assistant", true, false, true);
        let capability = assistant_capability_snapshot(&settings);
        assert!(capability.assistant_mode);
        assert!(capability.assistant_core_available);
        assert!(capability.degraded);
        assert!(!capability.hard_blocked);
        assert!(capability
            .missing_capabilities
            .iter()
            .any(|id| id == "output_voice_tts"));
        let (state, reason) = assistant_baseline_state(&capability);
        assert_eq!(state, AssistantOrchestratorState::Listening);
        assert_eq!(reason, "assistant_degraded_capability");
    }

    #[test]
    fn assistant_baseline_is_idle_when_workflow_agent_is_unavailable() {
        let settings = settings_for_assistant_mode("assistant", false, true, true);
        let capability = assistant_capability_snapshot(&settings);
        assert!(capability.assistant_mode);
        assert!(!capability.assistant_core_available);
        assert!(capability.hard_blocked);
        let (state, reason) = assistant_baseline_state(&capability);
        assert_eq!(state, AssistantOrchestratorState::Idle);
        assert_eq!(reason, "assistant_core_unavailable");
    }
}

#[tauri::command]
pub(crate) fn agent_list_supported_actions() -> Vec<String> {
    vec![
        "gdd_generate_publish".to_string(),
        "session_recap".to_string(),
        "plan_status".to_string(),
        "confirm_or_cancel".to_string(),
        "reminder_capture".to_string(),
        "web_search".to_string(),
        "open_module".to_string(),
        "open_app".to_string(),
    ]
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct AssistantExecuteDirectActionRequest {
    intent: String,
    command_text: String,
}

pub(crate) fn normalize_assistant_action_text(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch.is_whitespace() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
}

fn open_external_target(target: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", target])
            .spawn()
            .map_err(|err| format!("Failed to open target '{target}': {err}"))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(target)
            .spawn()
            .map_err(|err| format!("Failed to open target '{target}': {err}"))?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|err| format!("Failed to open target '{target}': {err}"))?;
        return Ok(());
    }
}

fn extract_web_search_query(command_text: &str) -> String {
    let lower = normalize_assistant_action_text(command_text);
    let stripped = lower
        .replace("hey trispr", " ")
        .replace("trispr agent", " ")
        .replace("trispr", " ")
        .replace("search for", " ")
        .replace("look up", " ")
        .replace("google", " ")
        .replace("find online", " ")
        .replace("search", " ");
    let cleaned = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        command_text.trim().to_string()
    } else {
        cleaned
    }
}

fn resolve_module_target(command_text: &str) -> Option<(&'static str, &'static str)> {
    let text = normalize_assistant_action_text(command_text);
    if text.contains("gdd") || text.contains("game design") {
        return Some(("gdd_flow", "GDD Flow"));
    }
    if text.contains("voice output") || text.contains("tts") {
        return Some(("voice-output", "Voice Output"));
    }
    if text.contains("refinement") || text.contains("ai refinement") {
        return Some(("ai-refinement", "AI Refinement"));
    }
    if text.contains("assistant debug")
        || text.contains("assistant tab")
        || text.contains("agent tab")
    {
        return Some(("agent", "Assistant Debug"));
    }
    if text.contains("settings") {
        return Some(("settings", "Settings"));
    }
    if text.contains("module") {
        return Some(("modules", "Modules"));
    }
    if text.contains("transcription") || text.contains("transcribe") {
        return Some(("transcription", "Transcription"));
    }
    None
}

fn launch_named_app(command_text: &str) -> Result<&'static str, String> {
    #[cfg(target_os = "windows")]
    {
        let text = normalize_assistant_action_text(command_text);
        let launch =
            |program: &str, args: &[&str], label: &'static str| -> Result<&'static str, String> {
                std::process::Command::new(program)
                    .args(args)
                    .spawn()
                    .map_err(|err| format!("Failed to launch {label}: {err}"))?;
                Ok(label)
            };

        if text.contains("explorer") || text.contains("file explorer") || text.contains("files") {
            return launch("explorer", &[], "File Explorer");
        }
        if text.contains("notepad") {
            return launch("notepad.exe", &[], "Notepad");
        }
        if text.contains("calculator") || text.contains("calc") {
            return launch("calc.exe", &[], "Calculator");
        }
        if text.contains("terminal") || text.contains("powershell") {
            return launch("powershell.exe", &[], "Terminal");
        }
        if text.contains("cursor") {
            return launch("cmd", &["/C", "start", "", "cursor"], "Cursor");
        }
        if text.contains("browser") || text.contains("chrome") || text.contains("edge") {
            open_external_target("https://www.google.com")?;
            return Ok("Default Browser");
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = command_text;

    Err("No supported desktop app target was detected.".to_string())
}

#[tauri::command]
pub(crate) fn assistant_execute_direct_action(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AssistantExecuteDirectActionRequest,
) -> Result<crate::workflow_agent::AgentExecutionResult, String> {
    guarded_command!("assistant_execute_direct_action", {
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            settings.clone()
        };

        let assistant_mode = assistant_product_mode(&settings_snapshot) == "assistant";
        if assistant_mode {
            let _ = transition_assistant_state_with_settings(
                &app,
                state.inner(),
                &settings_snapshot,
                AssistantOrchestratorState::Executing,
                "assistant_execute_direct_action:start",
            );
        }

        let result = match request.intent.trim() {
            "reminder_capture" => {
                if !crate::modules::task_capture::task_capture_enabled(&settings_snapshot) {
                    tracing::info!("[task_capture] module disabled, rejecting intent");
                    crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message:
                            "Task Capture module is disabled. Enable 'task_capture' in modules."
                                .to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("module_disabled".to_string()),
                    }
                } else {
                    let tc_settings = settings_snapshot.task_capture_settings.clone();
                    tracing::info!("[task_capture] raw input: {:?}", request.command_text);

                    let matched_route = crate::modules::task_capture::find_matching_route(
                        &request.command_text,
                        &tc_settings,
                    );
                    let route = match matched_route {
                        Some(r) => {
                            tracing::info!("[task_capture] matched route: {:?}", r.label);
                            r.clone()
                        }
                        None => {
                            tracing::info!(
                                "[task_capture] no route matched, using first route as fallback"
                            );
                            tc_settings.routes.first().cloned().unwrap_or_default()
                        }
                    };

                    let raw_task =
                        crate::modules::task_capture::extract_task_text(&request.command_text);
                    tracing::info!("[task_capture] extracted text: {:?}", raw_task);
                    if raw_task.is_empty() {
                        crate::workflow_agent::AgentExecutionResult {
                            status: "failed".to_string(),
                            message: "No reminder text was detected.".to_string(),
                            draft: None,
                            publish_result: None,
                            queued_job: None,
                            error: Some("reminder_text_missing".to_string()),
                        }
                    } else {
                        let _ = app.emit(
                            "agent:execution-progress",
                            serde_json::json!({
                                "intent": "reminder_capture",
                                "stage": "enqueue",
                                "message": format!("{}: Eintrag erkannt", route.label),
                                "text": raw_task.clone(),
                            }),
                        );

                        let app_clone = app.clone();
                        let settings_clone = settings_snapshot.clone();
                        let queued_task = raw_task.clone();
                        let route_clone = route.clone();
                        let ai_enabled = tc_settings.ai_refinement_enabled;
                        let custom_prompt = tc_settings.refinement_prompt.clone();
                        tauri::async_runtime::spawn(async move {
                            let refined_text = if ai_enabled {
                                let _ = app_clone.emit(
                                    "agent:execution-progress",
                                    serde_json::json!({
                                        "intent": "reminder_capture",
                                        "stage": "refining",
                                        "message": "Ollama formuliert Task…",
                                        "text": queued_task.clone(),
                                    }),
                                );
                                let _ = update_overlay_refining_indicator(&app_clone, true);

                                let app_for_refine = app_clone.clone();
                                let settings_for_refine = settings_clone.clone();
                                let raw_for_refine = queued_task.clone();
                                let prompt_for_refine = custom_prompt.clone();
                                match tauri::async_runtime::spawn_blocking(move || {
                                    crate::modules::task_capture::refine_task_text(
                                        &app_for_refine,
                                        &settings_for_refine,
                                        &raw_for_refine,
                                        Some(&prompt_for_refine),
                                    )
                                })
                                .await
                                {
                                    Ok(text) if !text.trim().is_empty() => {
                                        tracing::info!(
                                            "[task_capture] refined text: {:?}",
                                            text.trim()
                                        );
                                        text.trim().to_string()
                                    }
                                    Ok(_) => {
                                        tracing::warn!(
                                            "[task_capture] refinement returned empty, using raw"
                                        );
                                        queued_task.clone()
                                    }
                                    Err(error) => {
                                        tracing::warn!(
                                            "[task_capture] refinement task join failed: {}",
                                            error
                                        );
                                        queued_task.clone()
                                    }
                                }
                            } else {
                                tracing::info!(
                                    "[task_capture] AI refinement disabled, using raw text"
                                );
                                queued_task.clone()
                            };

                            let _ = app_clone.emit(
                                "agent:execution-progress",
                                serde_json::json!({
                                    "intent": "reminder_capture",
                                    "stage": "posting",
                                    "message": format!("Task wird an {} gesendet…", route_clone.label),
                                    "text": refined_text.clone(),
                                }),
                            );

                            tracing::info!("[task_capture] posting to: {}", route_clone.endpoint);
                            let post_text = refined_text.clone();
                            let post_endpoint = route_clone.endpoint.clone();
                            let agenda_post_result =
                                match tauri::async_runtime::spawn_blocking(move || {
                                    crate::modules::task_capture::post_task_to_endpoint(
                                        &post_text,
                                        &post_endpoint,
                                    )
                                })
                                .await
                                {
                                    Ok(result) => result,
                                    Err(error) => Err(format!(
                                        "Task capture request task join failed: {}",
                                        error
                                    )),
                                };

                            let _ = update_overlay_refining_indicator(&app_clone, false);
                            tracing::info!("[task_capture] post result: {:?}", agenda_post_result);

                            let (result, notification_payload) = match agenda_post_result {
                                Ok(()) => (
                                    crate::workflow_agent::AgentExecutionResult {
                                        status: "completed".to_string(),
                                        message: format!("{}: {}", route_clone.label, refined_text),
                                        draft: None,
                                        publish_result: None,
                                        queued_job: None,
                                        error: None,
                                    },
                                    serde_json::json!({
                                        "ok": true,
                                        "title": route_clone.label,
                                        "message": format!("{}: {}", route_clone.label, refined_text),
                                        "text": refined_text,
                                    }),
                                ),
                                Err(error) => {
                                    tracing::error!("[task_capture] post failed: {}", error);
                                    (
                                        crate::workflow_agent::AgentExecutionResult {
                                            status: "failed".to_string(),
                                            message: format!(
                                                "{}-Fehler: {}",
                                                route_clone.label, error
                                            ),
                                            draft: None,
                                            publish_result: None,
                                            queued_job: None,
                                            error: Some(error.clone()),
                                        },
                                        serde_json::json!({
                                            "ok": false,
                                            "title": route_clone.label,
                                            "message": format!("Fehler: {}", error),
                                        }),
                                    )
                                }
                            };

                            let _ = app_clone.emit("agenda:notification", &notification_payload);
                            if result.status == "failed" {
                                let _ = app_clone.emit("agent:execution-failed", &result);
                            } else {
                                let _ = app_clone.emit("agent:execution-finished", &result);
                            }

                            if assistant_mode {
                                let assistant_state = if result.status == "failed" {
                                    AssistantOrchestratorState::Recovering
                                } else {
                                    AssistantOrchestratorState::Listening
                                };
                                emit_assistant_action_result(
                                    &app_clone,
                                    &settings_clone,
                                    assistant_state,
                                    &result,
                                    "assistant_execute_direct_action:reminder_capture",
                                );
                                let state_handle = app_clone.state::<AppState>();
                                let _ = emit_assistant_baseline_state(
                                    &app_clone,
                                    state_handle.inner(),
                                    &settings_clone,
                                    "assistant_execute_direct_action:reminder_capture_complete",
                                );
                            }

                            let result_status = result.status.clone();
                            let result_message = result.message.clone();
                            let _ = app_clone.emit(
                                "agent:execution-progress",
                                serde_json::json!({
                                    "intent": "reminder_capture",
                                    "stage": "done",
                                    "status": result_status,
                                    "message": result_message,
                                }),
                            );
                        });

                        crate::workflow_agent::AgentExecutionResult {
                            status: "queued".to_string(),
                            message: format!("{}: Task wird verarbeitet…", route.label),
                            draft: None,
                            publish_result: None,
                            queued_job: None,
                            error: None,
                        }
                    }
                }
            }
            "web_search" => {
                if !settings_snapshot.workflow_agent.online_enabled {
                    crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Web search is disabled in Offline mode.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("online_disabled".to_string()),
                    }
                } else {
                    let query = extract_web_search_query(&request.command_text);
                    let encoded: String =
                        url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
                    let target = format!("https://www.google.com/search?q={encoded}");
                    open_external_target(&target)?;
                    crate::workflow_agent::AgentExecutionResult {
                        status: "completed".to_string(),
                        message: format!("Opened web search for “{}”.", query.trim()),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: None,
                    }
                }
            }
            "open_module" => match resolve_module_target(&request.command_text) {
                Some((target, label)) => {
                    crate::show_main_window(&app);
                    let _ = app.emit(
                        "assistant:open-module",
                        serde_json::json!({
                            "target": target,
                            "reason": "assistant_execute_direct_action",
                        }),
                    );
                    crate::workflow_agent::AgentExecutionResult {
                        status: "completed".to_string(),
                        message: format!("Opened {}.", label),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: None,
                    }
                }
                None => crate::workflow_agent::AgentExecutionResult {
                    status: "failed".to_string(),
                    message: "No supported Trispr surface was detected in that request."
                        .to_string(),
                    draft: None,
                    publish_result: None,
                    queued_job: None,
                    error: Some("module_target_missing".to_string()),
                },
            },
            "open_app" => match launch_named_app(&request.command_text) {
                Ok(label) => crate::workflow_agent::AgentExecutionResult {
                    status: "completed".to_string(),
                    message: format!("Opened {}.", label),
                    draft: None,
                    publish_result: None,
                    queued_job: None,
                    error: None,
                },
                Err(error) => crate::workflow_agent::AgentExecutionResult {
                    status: "failed".to_string(),
                    message: error.clone(),
                    draft: None,
                    publish_result: None,
                    queued_job: None,
                    error: Some(error),
                },
            },
            _ => crate::workflow_agent::AgentExecutionResult {
                status: "failed".to_string(),
                message: "Unsupported direct assistant action.".to_string(),
                draft: None,
                publish_result: None,
                queued_job: None,
                error: Some("unsupported_direct_action".to_string()),
            },
        };

        if assistant_mode {
            let assistant_state = if result.status == "failed" {
                AssistantOrchestratorState::Recovering
            } else {
                AssistantOrchestratorState::Listening
            };
            emit_assistant_action_result(
                &app,
                &settings_snapshot,
                assistant_state,
                &result,
                "assistant_execute_direct_action",
            );
            let _ = emit_assistant_baseline_state(
                &app,
                state.inner(),
                &settings_snapshot,
                "assistant_execute_direct_action:complete",
            );
        }

        Ok(result)
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct AgentComposeUnknownReplyRequest {
    command_text: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct AgentComposeReplyResult {
    text: String,
    source: String,
    reason_code: String,
}

fn merged_workflow_wakewords(settings: &crate::modules::WorkflowAgentSettings) -> Vec<String> {
    let mut merged = settings.wakewords.clone();
    merged.extend(settings.wakeword_aliases.clone());
    merged
}

fn unknown_rule_reply(command_text: &str, online_enabled: bool) -> String {
    let normalized = command_text.to_lowercase();
    let english_hint = weather::weather_query_english_hint(&normalized);
    let weather_like = weather::weather_query_like(&normalized);
    if weather_like {
        if online_enabled {
            return weather::online_weather_unavailable_reply(command_text);
        }
        if english_hint {
            return "I do not have live weather access in local mode. I can still help with plan status, session recaps, or GDD drafts from your transcripts.".to_string();
        }
        return "Ich habe lokal keinen Live-Wetterzugriff. Ich kann dir aber einen Plan, Recap oder GDD-Draft aus deinen Transkripten erstellen.".to_string();
    }
    if english_hint {
        return "I can currently handle GDD drafts, session recaps, and plan status from your local transcripts. Please rephrase your request within that scope.".to_string();
    }
    "Ich kann aktuell GDD-Drafts, Session-Recaps und Plan-Status aus deinen lokalen Transkripten verarbeiten. Formuliere die Anfrage bitte in diesem Scope.".to_string()
}

pub(crate) fn ai_provider_is_local(provider: &str) -> bool {
    matches!(provider, "ollama" | "lm_studio" | "oobabooga")
}

#[tauri::command]
pub(crate) fn agent_compose_unknown_reply(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AgentComposeUnknownReplyRequest,
) -> Result<AgentComposeReplyResult, String> {
    guarded_command!("agent_compose_unknown_reply", {
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            settings.clone()
        };

        let command_text = request.command_text.trim();
        if command_text.is_empty() {
            return Ok(AgentComposeReplyResult {
                text: unknown_rule_reply("", false),
                source: "rule".to_string(),
                reason_code: "empty_command".to_string(),
            });
        }

        let workflow_cfg = &settings_snapshot.workflow_agent;
        let online_enabled = workflow_cfg.online_enabled;
        let weather_like = weather::weather_query_like(&command_text.to_lowercase());
        if weather_like && online_enabled {
            match weather::fetch_live_weather_reply(command_text) {
                Ok(text) => {
                    return Ok(AgentComposeReplyResult {
                        text,
                        source: "weather_api".to_string(),
                        reason_code: "weather_api_success".to_string(),
                    });
                }
                Err(error) => {
                    warn!("weather_api_error: {}", error);
                    return Ok(AgentComposeReplyResult {
                        text: weather::online_weather_unavailable_reply(command_text),
                        source: "rule".to_string(),
                        reason_code: "weather_api_error".to_string(),
                    });
                }
            }
        }
        if workflow_cfg.reply_mode != "hybrid_local_llm" {
            return Ok(AgentComposeReplyResult {
                text: unknown_rule_reply(command_text, online_enabled),
                source: "rule".to_string(),
                reason_code: "rule_only_mode".to_string(),
            });
        }

        if !settings_snapshot.ai_fallback.enabled {
            return Ok(AgentComposeReplyResult {
                text: unknown_rule_reply(command_text, online_enabled),
                source: "rule".to_string(),
                reason_code: "ai_refinement_disabled".to_string(),
            });
        }

        if !online_enabled && !ai_provider_is_local(&settings_snapshot.ai_fallback.provider) {
            return Ok(AgentComposeReplyResult {
                text: unknown_rule_reply(command_text, online_enabled),
                source: "rule".to_string(),
                reason_code: "non_local_provider_blocked".to_string(),
            });
        }

        let setup = match crate::ai_fallback::prepare_refinement(&app, &settings_snapshot) {
            Ok(value) => value,
            Err(error) => {
                return Ok(AgentComposeReplyResult {
                    text: unknown_rule_reply(command_text, online_enabled),
                    source: "rule".to_string(),
                    reason_code: format!("local_runtime_unavailable:{error}"),
                });
            }
        };

        let mut options = setup.options.clone();
        options.max_tokens = options.max_tokens.clamp(128, 512);
        options.custom_prompt = Some(if online_enabled {
            "You are Trispr. Reply in the same language as the user. Keep answers concise and practical. Online models are allowed, but live web lookup tools may be unavailable. Never fabricate citations or claim verified live data unless explicit tool results are provided. Output only the reply.".to_string()
        } else {
            "You are Trispr, a local assistant. Reply in the same language as the user. Keep answers concise and practical. Never claim live internet access. If the request needs real-time external data, say this capability is unavailable locally and suggest a supported local action. Output only the reply."
                .to_string()
        });
        options.enforce_language_guard = false;

        match setup
            .provider
            .refine_transcript(command_text, &setup.model, &options, &setup.api_key)
        {
            Ok(reply) => {
                let text = reply.text.trim();
                if text.is_empty() {
                    return Ok(AgentComposeReplyResult {
                        text: unknown_rule_reply(command_text, online_enabled),
                        source: "rule".to_string(),
                        reason_code: "local_llm_empty".to_string(),
                    });
                }
                Ok(AgentComposeReplyResult {
                    text: text.to_string(),
                    source: "local_llm".to_string(),
                    reason_code: "local_llm_success".to_string(),
                })
            }
            Err(error) => Ok(AgentComposeReplyResult {
                text: unknown_rule_reply(command_text, online_enabled),
                source: "rule".to_string(),
                reason_code: format!("local_llm_error:{error}"),
            }),
        }
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct AgentCancelPendingConfirmationRequest {
    reason: Option<String>,
}

#[tauri::command]
pub(crate) fn agent_cancel_pending_confirmation(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Option<AgentCancelPendingConfirmationRequest>,
) -> Result<crate::workflow_agent::AgentExecutionResult, String> {
    guarded_command!("agent_cancel_pending_confirmation", {
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            settings.clone()
        };
        let assistant_mode = assistant_product_mode(&settings_snapshot) == "assistant";
        let reason = request
            .and_then(|value| value.reason)
            .unwrap_or_else(|| "cancelled_by_user".to_string());
        let reason_trimmed = reason.trim().to_string();
        let pending = {
            let mut guard = ASSISTANT_PENDING_CONFIRMATION
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.take()
        };

        if pending.is_none() {
            return Ok(crate::workflow_agent::AgentExecutionResult {
                status: "cancelled".to_string(),
                message: "No pending confirmation to cancel.".to_string(),
                draft: None,
                publish_result: None,
                queued_job: None,
                error: None,
            });
        }

        let pending = pending.expect("checked is_some");
        let result = crate::workflow_agent::AgentExecutionResult {
            status: "cancelled".to_string(),
            message: "Pending confirmation cancelled.".to_string(),
            draft: None,
            publish_result: None,
            queued_job: None,
            error: None,
        };

        if assistant_mode {
            if reason_trimmed.eq_ignore_ascii_case("timeout") {
                emit_assistant_confirmation_expired(
                    &app,
                    &settings_snapshot,
                    "agent_cancel_pending_confirmation:timeout",
                    pending.expires_at_ms,
                );
            } else {
                emit_assistant_action_result(
                    &app,
                    &settings_snapshot,
                    AssistantOrchestratorState::Recovering,
                    &result,
                    "agent_cancel_pending_confirmation",
                );
            }
            let _ = emit_assistant_baseline_state(
                &app,
                state.inner(),
                &settings_snapshot,
                "agent_cancel_pending_confirmation",
            );
        }

        Ok(result)
    })
}

#[tauri::command]
pub(crate) fn agent_parse_command(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::workflow_agent::AgentParseCommandRequest,
) -> Result<crate::workflow_agent::AgentCommandParseResult, String> {
    guarded_command!("agent_parse_command", {
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            settings.clone()
        };
        let assistant_mode = assistant_product_mode(&settings_snapshot) == "assistant";
        if assistant_mode {
            let _ = expire_pending_confirmation_if_needed(
                &app,
                state.inner(),
                &settings_snapshot,
                "agent_parse_command",
            );
        }
        if assistant_mode {
            let _ = transition_assistant_state_with_settings(
                &app,
                state.inner(),
                &settings_snapshot,
                AssistantOrchestratorState::Parsing,
                "agent_parse_command:start",
            );
        }
        let workflow_settings = settings_snapshot.workflow_agent.clone();
        let intent_keywords = workflow_settings
            .intent_keywords
            .get("gdd_generate_publish")
            .cloned()
            .unwrap_or_default();
        let wakewords = merged_workflow_wakewords(&workflow_settings);
        let parsed = crate::workflow_agent::parse_command(&request, &wakewords, &intent_keywords);
        if parsed.detected || parsed.wakeword_matched {
            let _ = app.emit("agent:command-detected", &parsed);
            if assistant_mode {
                emit_assistant_intent_detected(
                    &app,
                    &settings_snapshot,
                    &parsed,
                    if parsed.detected {
                        "agent_parse_command:detected"
                    } else {
                        "agent_parse_command:wakeword_unknown"
                    },
                );
            }
        }
        if assistant_mode {
            let trigger = if parsed.detected {
                "agent_parse_command:detected"
            } else if parsed.wakeword_matched {
                "agent_parse_command:wakeword_unknown"
            } else {
                "agent_parse_command:ignored"
            };
            let _ = emit_assistant_baseline_state(&app, state.inner(), &settings_snapshot, trigger);
        }
        Ok(parsed)
    })
}

#[tauri::command]
pub(crate) fn search_transcript_sessions(
    state: State<'_, AppState>,
    mut request: crate::workflow_agent::SearchTranscriptSessionsRequest,
) -> Result<Vec<crate::workflow_agent::TranscriptSessionCandidate>, String> {
    guarded_command!("search_transcript_sessions", {
        let defaults = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            (
                settings.workflow_agent.session_gap_minutes,
                settings.workflow_agent.max_candidates,
            )
        };
        if request.session_gap_minutes.unwrap_or(0) == 0 {
            request.session_gap_minutes = Some(defaults.0);
        }
        if request.max_candidates.unwrap_or(0) == 0 {
            request.max_candidates = Some(defaults.1);
        }

        let entries = collect_all_transcript_entries(&state);
        let sessions = crate::workflow_agent::build_sessions(
            &entries,
            request.session_gap_minutes.unwrap_or(defaults.0),
        );
        Ok(crate::workflow_agent::score_sessions(&sessions, &request))
    })
}

#[tauri::command]
pub(crate) fn agent_build_execution_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::workflow_agent::AgentBuildExecutionPlanRequest,
) -> Result<crate::workflow_agent::AgentExecutionPlan, String> {
    guarded_command!("agent_build_execution_plan", {
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            settings.clone()
        };
        let assistant_mode = assistant_product_mode(&settings_snapshot) == "assistant";
        if assistant_mode {
            let _ = expire_pending_confirmation_if_needed(
                &app,
                state.inner(),
                &settings_snapshot,
                "agent_build_execution_plan",
            );
        }
        if request.intent.trim().is_empty() {
            return Err("Intent is required.".to_string());
        }
        if request.session_id.trim().is_empty() {
            return Err("Session id is required.".to_string());
        }
        const ALLOWED_LANGUAGES: &[&str] = &[
            "source", "en", "de", "fr", "es", "it", "pt", "nl", "pl", "ru", "ja", "ko", "zh", "ar",
            "tr", "hi",
        ];
        let lang = request.target_language.trim();
        if !ALLOWED_LANGUAGES.contains(&lang) {
            return Err(format!(
                "Invalid target language '{}'. Allowed: {}",
                lang,
                ALLOWED_LANGUAGES.join(", ")
            ));
        }
        if assistant_mode {
            let _ = transition_assistant_state_with_settings(
                &app,
                state.inner(),
                &settings_snapshot,
                AssistantOrchestratorState::Planning,
                "agent_build_execution_plan:start",
            );
        }
        let plan = crate::workflow_agent::default_execution_plan(&request);
        let _ = app.emit("agent:plan-ready", &plan);
        if assistant_mode {
            let _ = transition_assistant_state_with_settings(
                &app,
                state.inner(),
                &settings_snapshot,
                AssistantOrchestratorState::AwaitingConfirm,
                "agent_build_execution_plan:ready",
            );
            emit_assistant_plan_ready(
                &app,
                &settings_snapshot,
                &plan,
                "agent_build_execution_plan:ready",
            );
            emit_assistant_awaiting_confirmation(
                &app,
                &settings_snapshot,
                &plan,
                "agent_build_execution_plan:ready",
            );
        }
        Ok(plan)
    })
}

#[tauri::command]
pub(crate) fn agent_execute_gdd_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::workflow_agent::AgentExecuteGddPlanRequest,
) -> Result<crate::workflow_agent::AgentExecutionResult, String> {
    guarded_command!("agent_execute_gdd_plan", {
        let plan = request.plan.clone();
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            settings.clone()
        };
        let assistant_mode = assistant_product_mode(&settings_snapshot) == "assistant";
        if assistant_mode {
            let _ = expire_pending_confirmation_if_needed(
                &app,
                state.inner(),
                &settings_snapshot,
                "agent_execute_gdd_plan",
            );
        }
        if assistant_mode {
            let _ = transition_assistant_state_with_settings(
                &app,
                state.inner(),
                &settings_snapshot,
                AssistantOrchestratorState::Executing,
                "agent_execute_gdd_plan:start",
            );
        }
        let confirmation_token = request
            .confirmation_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string);

        if let Some(token) = confirmation_token.as_deref() {
            match consume_pending_confirmation(&plan, token) {
                Ok(()) => {}
                Err(PendingConfirmationError::Missing) => {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "No pending confirmation for this execution.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("confirmation_missing".to_string()),
                    };
                    if assistant_mode {
                        emit_assistant_action_result(
                            &app,
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            &result,
                            "agent_execute_gdd_plan:confirmation_missing",
                        );
                        let _ = emit_assistant_baseline_state(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_missing",
                        );
                    }
                    return Ok(result);
                }
                Err(PendingConfirmationError::Expired { expired_at_ms }) => {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Confirmation token expired. Build a new plan and confirm again."
                            .to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("confirmation_expired".to_string()),
                    };
                    if assistant_mode {
                        emit_assistant_confirmation_expired(
                            &app,
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_expired",
                            expired_at_ms,
                        );
                        emit_assistant_action_result(
                            &app,
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            &result,
                            "agent_execute_gdd_plan:confirmation_expired",
                        );
                        let _ = emit_assistant_baseline_state(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_expired",
                        );
                    }
                    return Ok(result);
                }
                Err(PendingConfirmationError::TokenMismatch) => {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Invalid confirmation token.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("confirmation_token_mismatch".to_string()),
                    };
                    if assistant_mode {
                        emit_assistant_action_result(
                            &app,
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            &result,
                            "agent_execute_gdd_plan:confirmation_token_mismatch",
                        );
                        let _ = emit_assistant_baseline_state(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_token_mismatch",
                        );
                    }
                    return Ok(result);
                }
                Err(PendingConfirmationError::PlanMismatch) => {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Confirmation token does not match the active plan.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("confirmation_plan_mismatch".to_string()),
                    };
                    if assistant_mode {
                        emit_assistant_action_result(
                            &app,
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            &result,
                            "agent_execute_gdd_plan:confirmation_plan_mismatch",
                        );
                        let _ = emit_assistant_baseline_state(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_plan_mismatch",
                        );
                    }
                    return Ok(result);
                }
            }
        } else {
            clear_pending_confirmation_for_plan(&plan);
        }

        let execution_result =
            (|| -> Result<crate::workflow_agent::AgentExecutionResult, String> {
                if plan.intent != "gdd_generate_publish" {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Unsupported agent intent.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some(format!("Unsupported intent '{}'.", plan.intent)),
                    };
                    let _ = app.emit("agent:execution-failed", &result);
                    return Ok(result);
                }

                let _ = app.emit(
                    "agent:execution-progress",
                    serde_json::json!({
                        "session_id": plan.session_id,
                        "stage": "load_session",
                    }),
                );

                let workflow_gap_minutes = settings_snapshot.workflow_agent.session_gap_minutes;
                let preset_clones = settings_snapshot.gdd_module_settings.preset_clones.clone();
                let confluence_settings = settings_snapshot.confluence_settings.clone();
                let one_click_threshold = settings_snapshot
                    .gdd_module_settings
                    .one_click_confidence_threshold;
                let vision_bridge_enabled =
                    capability_enabled(&settings_snapshot, RuntimeCapability::VisionInput);
                let tts_bridge_enabled =
                    capability_enabled(&settings_snapshot, RuntimeCapability::VoiceOutputTts)
                        && settings_snapshot.workflow_agent.voice_feedback_enabled;
                let maybe_agent_speak = |context: &str, message: &str| {
                    if !tts_bridge_enabled || message.trim().is_empty() {
                        return;
                    }
                    let request = crate::multimodal_io::TtsSpeakRequest {
                        provider: String::new(),
                        text: message.trim().to_string(),
                        rate: None,
                        volume: None,
                        context: Some(context.to_string()),
                    };
                    if let Err(tts_error) =
                        crate::multimodal_io::speak_tts_internal(&app, state.inner(), request)
                    {
                        info!("workflow_agent tts skipped: {}", tts_error);
                    }
                };
                let entries = collect_all_transcript_entries(&state);
                let sessions =
                    crate::workflow_agent::build_sessions(&entries, workflow_gap_minutes);
                let session = match sessions
                    .iter()
                    .find(|candidate| candidate.id == plan.session_id)
                    .cloned()
                {
                    Some(session) => session,
                    None => {
                        let result = crate::workflow_agent::AgentExecutionResult {
                            status: "failed".to_string(),
                            message: format!("Session '{}' not found.", plan.session_id),
                            draft: None,
                            publish_result: None,
                            queued_job: None,
                            error: Some(format!("Session '{}' not found.", plan.session_id)),
                        };
                        let _ = app.emit("agent:execution-failed", &result);
                        return Ok(result);
                    }
                };

                let transcript = session
                    .entries
                    .iter()
                    .map(|entry| entry.text.trim())
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if transcript.trim().is_empty() {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Session has no transcript content.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("Session content was empty.".to_string()),
                    };
                    let _ = app.emit("agent:execution-failed", &result);
                    return Ok(result);
                }

                let title = request
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("GDD Session {}", session.start_ms));
                let target_language = plan.target_language.trim().to_string();
                let mut template_hints: Vec<String> = Vec::new();
                if !target_language.is_empty() {
                    template_hints.push(format!(
                    "Target output language preference: {}. Keep source facts unchanged and avoid invention.",
                    target_language
                ));
                }
                if vision_bridge_enabled {
                    let _ = app.emit(
                        "agent:execution-progress",
                        serde_json::json!({
                            "session_id": plan.session_id,
                            "stage": "vision_context",
                        }),
                    );
                    match crate::multimodal_io::capture_vision_snapshot_internal(
                        &app,
                        state.inner(),
                    ) {
                        Ok(snapshot) => {
                            let dimensions = match (snapshot.width, snapshot.height) {
                                (Some(w), Some(h)) => format!("{}x{}", w, h),
                                _ => "unknown".to_string(),
                            };
                            let scope = snapshot
                                .source_scope
                                .as_deref()
                                .unwrap_or("unknown")
                                .to_string();
                            template_hints.push(format!(
                            "Vision context available (scope={}, sources={}, frame={}, timestamp_ms={}). Treat this as supporting context only.",
                            scope,
                            snapshot.source_count,
                            dimensions,
                            snapshot.timestamp_ms
                        ));
                            let _ = app.emit(
                                "agent:execution-progress",
                                serde_json::json!({
                                    "session_id": plan.session_id,
                                    "stage": "vision_context_ready",
                                    "source_scope": scope,
                                    "source_count": snapshot.source_count,
                                    "timestamp_ms": snapshot.timestamp_ms,
                                }),
                            );
                        }
                        Err(error) => {
                            warn!("workflow_agent vision context unavailable: {}", error);
                            let _ = app.emit(
                                "agent:execution-progress",
                                serde_json::json!({
                                    "session_id": plan.session_id,
                                    "stage": "vision_context_unavailable",
                                    "error": error,
                                }),
                            );
                        }
                    }
                }
                let template_hint = if template_hints.is_empty() {
                    None
                } else {
                    Some(template_hints.join(" "))
                };

                let _ = app.emit(
                    "agent:execution-progress",
                    serde_json::json!({
                        "session_id": plan.session_id,
                        "stage": "generate_draft",
                        "target_language": target_language,
                    }),
                );

                let draft_request = crate::gdd::GenerateGddDraftRequest {
                    transcript,
                    preset_id: request.preset_id.clone(),
                    title: Some(title.clone()),
                    max_chunk_chars: request.max_chunk_chars,
                    template_hint,
                    template_label: Some("workflow_agent".to_string()),
                };
                let draft = crate::gdd::generate_draft(&draft_request, &preset_clones);

                let publish_after_draft = crate::workflow_agent::should_publish_after_draft(&plan);
                if !publish_after_draft {
                    let skipped_reason = if plan.publish {
                        "Draft generated. Publish skipped because the execution lane had no publish step."
                    } else {
                        "Draft generated. Publish skipped by plan."
                    };
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "completed".to_string(),
                        message: skipped_reason.to_string(),
                        draft: Some(draft.clone()),
                        publish_result: None,
                        queued_job: None,
                        error: None,
                    };
                    let _ = app.emit("agent:execution-finished", &result);
                    maybe_agent_speak(
                        "agent_reply",
                        "Workflow Agent: Draft generated. Publish was skipped.",
                    );
                    return Ok(result);
                }

                let space_key = request
                    .space_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        let fallback = confluence_settings.default_space_key.trim();
                        if fallback.is_empty() {
                            None
                        } else {
                            Some(fallback.to_string())
                        }
                    })
                    .ok_or_else(|| "No Confluence space key provided for publish.".to_string())?;
                let parent_page_id = request
                    .parent_page_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        let fallback = confluence_settings.default_parent_page_id.trim();
                        if fallback.is_empty() {
                            None
                        } else {
                            Some(fallback.to_string())
                        }
                    });
                let target_page_id = request
                    .target_page_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);

                let _ = app.emit(
                    "agent:execution-progress",
                    serde_json::json!({
                        "session_id": plan.session_id,
                        "stage": "publish_or_queue",
                        "space_key": space_key,
                    }),
                );

                let storage_body = crate::gdd::render_storage::render_confluence_storage(&draft);
                let publish_request = crate::gdd::confluence::ConfluencePublishRequest {
                    title,
                    storage_body,
                    space_key: space_key.clone(),
                    parent_page_id,
                    target_page_id,
                };

                let publish_result =
                    crate::gdd::confluence::publish(&app, &confluence_settings, &publish_request);
                match publish_result {
                    Ok(publish) => {
                        {
                            let mut settings = state
                                .settings
                                .write()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let route_key = crate::gdd::confluence::routing_key_for(
                                &space_key,
                                &publish_request.title,
                            );
                            settings
                                .confluence_settings
                                .routing_memory
                                .insert(route_key, publish.page_id.clone());
                            normalize_confluence_settings(&mut settings.confluence_settings);
                            let _ = save_settings_file(&app, &settings);
                            let _ = app.emit("settings-changed", settings.clone());
                        }
                        let result = crate::workflow_agent::AgentExecutionResult {
                            status: "completed".to_string(),
                            message: "Draft generated and published to Confluence.".to_string(),
                            draft: Some(draft),
                            publish_result: Some(publish),
                            queued_job: None,
                            error: None,
                        };
                        let _ = app.emit("agent:execution-finished", &result);
                        maybe_agent_speak(
                            "agent_reply",
                            "Workflow Agent: Draft generated and published to Confluence.",
                        );
                        Ok(result)
                    }
                    Err(error) => {
                        if crate::gdd::publish_queue::is_queueable_publish_error(&error) {
                            let queue_request =
                                crate::gdd::publish_queue::GddPublishOrQueueRequest {
                                    draft: draft.clone(),
                                    publish_request,
                                    routing_confidence: Some(one_click_threshold),
                                    routing_reasoning: Some("workflow_agent execution".to_string()),
                                };
                            let queued_job = crate::gdd::publish_queue::queue_publish_request(
                                &app,
                                &queue_request,
                                &error,
                            )?;
                            let result = crate::workflow_agent::AgentExecutionResult {
                                status: "queued".to_string(),
                                message: "Confluence unavailable. Publish request queued locally."
                                    .to_string(),
                                draft: Some(draft),
                                publish_result: None,
                                queued_job: Some(queued_job),
                                error: Some(error),
                            };
                            let _ = app.emit("agent:execution-finished", &result);
                            maybe_agent_speak(
                            "agent_event",
                            "Workflow Agent: Confluence unavailable. Publish request was queued locally.",
                        );
                            Ok(result)
                        } else {
                            let result = crate::workflow_agent::AgentExecutionResult {
                                status: "failed".to_string(),
                                message: "Publish failed with non-queueable error.".to_string(),
                                draft: Some(draft),
                                publish_result: None,
                                queued_job: None,
                                error: Some(error.clone()),
                            };
                            let _ = app.emit("agent:execution-failed", &result);
                            maybe_agent_speak(
                                "agent_event",
                                "Workflow Agent: Publish failed with a non-queueable error.",
                            );
                            Ok(result)
                        }
                    }
                }
            })();

        match execution_result {
            Ok(result) => {
                if assistant_mode {
                    let assistant_state = if result.status == "failed" {
                        let _ = transition_assistant_state_with_settings(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            "agent_execute_gdd_plan:result_failed",
                        );
                        AssistantOrchestratorState::Recovering
                    } else {
                        AssistantOrchestratorState::Executing
                    };
                    emit_assistant_action_result(
                        &app,
                        &settings_snapshot,
                        assistant_state,
                        &result,
                        "agent_execute_gdd_plan:result",
                    );
                    let _ = emit_assistant_baseline_state(
                        &app,
                        state.inner(),
                        &settings_snapshot,
                        "agent_execute_gdd_plan:complete",
                    );
                }
                Ok(result)
            }
            Err(error) => {
                if assistant_mode {
                    let _ = transition_assistant_state_with_settings(
                        &app,
                        state.inner(),
                        &settings_snapshot,
                        AssistantOrchestratorState::Recovering,
                        "agent_execute_gdd_plan:error",
                    );
                    let synthetic_result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Workflow execution failed before completion.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some(error.clone()),
                    };
                    emit_assistant_action_result(
                        &app,
                        &settings_snapshot,
                        AssistantOrchestratorState::Recovering,
                        &synthetic_result,
                        "agent_execute_gdd_plan:error",
                    );
                    let _ = emit_assistant_baseline_state(
                        &app,
                        state.inner(),
                        &settings_snapshot,
                        "agent_execute_gdd_plan:error",
                    );
                }
                Err(error)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_wakewords() -> Vec<String> {
        vec!["trispr".to_string(), "hey trispr".to_string()]
    }

    fn make_keywords() -> Vec<String> {
        vec![
            "gdd".to_string(),
            "game design document".to_string(),
            "draft".to_string(),
        ]
    }

    fn make_entry(id: &str, text: &str, timestamp_ms: u64) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            text: text.to_string(),
            timestamp_ms,
            source: "mic".to_string(),
            speaker_name: None,
            refinement: None,
        }
    }

    fn make_entry_with_source(
        id: &str,
        text: &str,
        timestamp_ms: u64,
        source: &str,
    ) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            text: text.to_string(),
            timestamp_ms,
            source: source.to_string(),
            speaker_name: None,
            refinement: None,
        }
    }

    // --- parse_command tests ---

    #[test]
    fn detects_intent_with_wakeword_and_keyword() {
        let req = AgentParseCommandRequest {
            command_text: "hey trispr create a gdd for today".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "gdd_generate_publish");
        assert!(result.confidence >= 0.8);
    }

    #[test]
    fn detects_wakeword_alias_variant() {
        let req = AgentParseCommandRequest {
            command_text: "hey trispa create a gdd for today".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "gdd_generate_publish");
        assert!(result.wakeword_matched);
    }

    #[test]
    fn detects_intent_with_punctuation_around_wakeword() {
        let req = AgentParseCommandRequest {
            command_text: "Hey, Trispr! plan status please.".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "plan_status");
        assert!(result.wakeword_matched);
    }

    #[test]
    fn no_detection_without_wakeword() {
        let req = AgentParseCommandRequest {
            command_text: "create a gdd and draft something".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(!result.detected);
        assert_eq!(result.intent, "unknown");
    }

    #[test]
    fn no_detection_without_keyword() {
        let req = AgentParseCommandRequest {
            command_text: "trispr please do something".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(!result.detected);
    }

    #[test]
    fn detects_session_recap_intent() {
        let req = AgentParseCommandRequest {
            command_text: "hey trispr recap yesterday session".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "session_recap");
    }

    #[test]
    fn detects_plan_status_intent() {
        let req = AgentParseCommandRequest {
            command_text: "trispr plan status".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "plan_status");
    }

    #[test]
    fn detects_confirm_or_cancel_intent() {
        let req = AgentParseCommandRequest {
            command_text: "hey trispr bestätigen".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "confirm_or_cancel");
    }

    #[test]
    fn detects_web_search_intent() {
        let req = AgentParseCommandRequest {
            command_text: "hey trispr search for weather in Berlin".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "web_search");
    }

    #[test]
    fn detects_open_module_intent() {
        let req = AgentParseCommandRequest {
            command_text: "hey trispr open settings".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "open_module");
    }

    #[test]
    fn bare_todo_does_not_trigger_reminder_capture() {
        let req = AgentParseCommandRequest {
            command_text: "hey trispr we discussed the todo list in the meeting".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert_ne!(result.intent, "reminder_capture");
    }

    #[test]
    fn bare_agenda_does_not_trigger_reminder_capture() {
        let req = AgentParseCommandRequest {
            command_text: "trispr the agenda was about game balance".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert_ne!(result.intent, "reminder_capture");
    }

    #[test]
    fn erinnere_mich_triggers_reminder_capture() {
        let req = AgentParseCommandRequest {
            command_text: "trispr erinnere mich daran die Mail zu schreiben".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "reminder_capture");
    }

    #[test]
    fn add_to_my_agenda_triggers_reminder_capture() {
        let req = AgentParseCommandRequest {
            command_text: "hey trispr add to my agenda review the PR".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.detected);
        assert_eq!(result.intent, "reminder_capture");
    }

    #[test]
    fn detects_temporal_hint_today() {
        let req = AgentParseCommandRequest {
            command_text: "trispr gdd from today's meeting".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert_eq!(result.temporal_hint.as_deref(), Some("today"));
        assert!(result.confidence >= 0.9);
    }

    #[test]
    fn detects_temporal_hint_gestern() {
        let req = AgentParseCommandRequest {
            command_text: "trispr draft gdd von gestern".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert_eq!(result.temporal_hint.as_deref(), Some("gestern"));
    }

    #[test]
    fn detects_topic_hint_after_about() {
        let req = AgentParseCommandRequest {
            command_text: "trispr gdd about combat mechanics".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert_eq!(result.topic_hint.as_deref(), Some("combat mechanics"));
    }

    #[test]
    fn detects_publish_flag() {
        let req = AgentParseCommandRequest {
            command_text: "trispr gdd publish to confluence".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.publish_requested);
    }

    #[test]
    fn confidence_is_clamped_to_one() {
        let req = AgentParseCommandRequest {
            command_text: "hey trispr gdd today about combat mechanics".to_string(),
            source: None,
        };
        let result = parse_command(&req, &make_wakewords(), &make_keywords());
        assert!(result.confidence <= 1.0);
    }

    // --- build_sessions tests ---

    #[test]
    fn empty_entries_returns_empty() {
        let sessions = build_sessions(&[], 20);
        assert!(sessions.is_empty());
    }

    #[test]
    fn single_entry_creates_one_session() {
        let entries = vec![make_entry("e1", "hello world", 1_000_000)];
        let sessions = build_sessions(&entries, 20);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].entries.len(), 1);
    }

    #[test]
    fn entries_within_gap_are_grouped() {
        let entries = vec![
            make_entry("e1", "first", 0),
            make_entry("e2", "second", 5 * 60_000), // 5 min later
        ];
        let sessions = build_sessions(&entries, 20);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].entries.len(), 2);
    }

    #[test]
    fn entries_beyond_gap_create_separate_sessions() {
        let entries = vec![
            make_entry("e1", "first", 0),
            make_entry("e2", "second", 30 * 60_000), // 30 min later (> 20 min gap)
        ];
        let sessions = build_sessions(&entries, 20);
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn adaptive_gap_merge_keeps_continuous_conversation_together() {
        let entries = vec![
            make_entry_with_source("e1", "combat mechanics balancing", 0, "mic"),
            make_entry_with_source(
                "e2",
                "combat mechanics balancing follow-up",
                21 * 60_000,
                "system",
            ),
        ];
        let sessions = build_sessions(&entries, 20);
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn adaptive_gap_merge_still_splits_when_gap_is_too_large() {
        let entries = vec![
            make_entry_with_source("e1", "combat mechanics balancing", 0, "mic"),
            make_entry_with_source(
                "e2",
                "combat mechanics balancing follow-up",
                45 * 60_000,
                "system",
            ),
        ];
        let sessions = build_sessions(&entries, 20);
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn sessions_sorted_most_recent_first() {
        let entries = vec![
            make_entry("e1", "old", 0),
            make_entry("e2", "new", 60 * 60_000), // 1 hour later
        ];
        let sessions = build_sessions(&entries, 20);
        assert_eq!(sessions.len(), 2);
        assert!(sessions[0].start_ms > sessions[1].start_ms);
    }

    // --- score_sessions tests ---

    #[test]
    fn topic_match_raises_score() {
        let entries_match = vec![make_entry("e1", "combat mechanics discussion", 0)];
        let entries_no_match = vec![make_entry("e2", "unrelated content here", 0)];
        let sessions = vec![
            SessionBucket {
                id: "s1".to_string(),
                start_ms: 0,
                end_ms: 0,
                entries: entries_match,
            },
            SessionBucket {
                id: "s2".to_string(),
                start_ms: 0,
                end_ms: 0,
                entries: entries_no_match,
            },
        ];
        let req = SearchTranscriptSessionsRequest {
            temporal_hint: None,
            topic_hint: Some("combat mechanics".to_string()),
            session_gap_minutes: Some(20),
            max_candidates: Some(5),
        };
        let scored = score_sessions(&sessions, &req);
        assert_eq!(scored[0].session_id, "s1");
        assert!(scored[0].score > scored[1].score);
    }

    #[test]
    fn max_candidates_limits_results() {
        let entries: Vec<_> = (0..10)
            .map(|i| make_entry(&format!("e{i}"), "text", i * 60_000))
            .collect();
        let sessions = build_sessions(&entries, 1);
        let req = SearchTranscriptSessionsRequest {
            temporal_hint: None,
            topic_hint: None,
            session_gap_minutes: Some(1),
            max_candidates: Some(3),
        };
        let scored = score_sessions(&sessions, &req);
        assert!(scored.len() <= 3);
    }

    #[test]
    fn score_prefers_richer_mixed_source_session_when_recency_matches() {
        let sessions = vec![
            SessionBucket {
                id: "s_rich".to_string(),
                start_ms: 0,
                end_ms: 10 * 60_000,
                entries: vec![
                    make_entry_with_source("e1", "combat loop draft", 0, "mic"),
                    make_entry_with_source("e2", "combat loop feedback", 2 * 60_000, "system"),
                    make_entry_with_source("e3", "combat loop action items", 4 * 60_000, "mic"),
                ],
            },
            SessionBucket {
                id: "s_thin".to_string(),
                start_ms: 0,
                end_ms: 10 * 60_000,
                entries: vec![make_entry_with_source("e4", "combat", 0, "mic")],
            },
        ];
        let req = SearchTranscriptSessionsRequest {
            temporal_hint: None,
            topic_hint: None,
            session_gap_minutes: Some(20),
            max_candidates: Some(5),
        };
        let scored = score_sessions(&sessions, &req);
        assert_eq!(
            scored.first().map(|item| item.session_id.as_str()),
            Some("s_rich")
        );
        assert!(scored
            .first()
            .map(|item| item.reasoning.contains("continuity="))
            .unwrap_or(false));
        assert!(scored
            .first()
            .map(|item| item.reasoning.contains("archive="))
            .unwrap_or(false));
    }

    #[test]
    fn default_execution_plan_surfaces_transparent_suggestion_metadata() {
        let request = AgentBuildExecutionPlanRequest {
            intent: "gdd_generate_publish".to_string(),
            session_id: "s_123_456".to_string(),
            target_language: "en".to_string(),
            publish: true,
            command_text: Some(
                "hey trispr build a gdd draft for yesterday about combat balancing".to_string(),
            ),
            temporal_hint: Some("yesterday".to_string()),
            topic_hint: Some("combat balancing".to_string()),
            parse_confidence: Some(0.87),
        };
        let plan = default_execution_plan(&request);
        assert_eq!(plan.intent, "gdd_generate_publish");
        assert_eq!(plan.session_id, "s_123_456");
        assert!(plan
            .recognized_signals
            .iter()
            .any(|item| item.contains("parse_confidence=0.87")));
        assert!(plan
            .recognized_signals
            .iter()
            .any(|item| item.contains("topic_hint=combat balancing")));
        assert!(plan
            .assumptions
            .iter()
            .any(|item| item.contains("requires explicit confirmation")));
        assert!(plan
            .proposed_actions
            .iter()
            .any(|item| item.contains("publish to Confluence")));
        assert_eq!(plan.analysis_steps.len(), 2);
        assert_eq!(plan.execution_steps.len(), 1);
        assert_eq!(plan.steps.len(), 3);
    }

    #[test]
    fn default_execution_plan_omits_side_effect_lane_when_publish_is_false() {
        let request = AgentBuildExecutionPlanRequest {
            intent: "gdd_generate_publish".to_string(),
            session_id: "s_123_789".to_string(),
            target_language: "source".to_string(),
            publish: false,
            command_text: Some("hey trispr build draft".to_string()),
            temporal_hint: None,
            topic_hint: None,
            parse_confidence: Some(0.72),
        };
        let plan = default_execution_plan(&request);
        assert_eq!(plan.analysis_steps.len(), 2);
        assert!(plan.execution_steps.is_empty());
        assert_eq!(plan.steps.len(), 2);
    }

    #[test]
    fn should_publish_after_draft_requires_publish_flag_and_publish_step() {
        let publish_plan = AgentExecutionPlan {
            intent: "gdd_generate_publish".to_string(),
            session_id: "s_1".to_string(),
            session_title: "Session s_1".to_string(),
            target_language: "source".to_string(),
            publish: true,
            analysis_steps: vec![],
            execution_steps: vec![AgentExecutionStep {
                id: "publish_or_queue".to_string(),
                title: "Publish to Confluence".to_string(),
                status: "pending".to_string(),
                detail: None,
            }],
            steps: vec![],
            recognized_signals: vec![],
            assumptions: vec![],
            proposed_actions: vec![],
            summary: "x".to_string(),
        };
        assert!(should_publish_after_draft(&publish_plan));

        let missing_publish_step = AgentExecutionPlan {
            publish: true,
            execution_steps: vec![],
            steps: vec![],
            ..publish_plan.clone()
        };
        assert!(!should_publish_after_draft(&missing_publish_step));

        let publish_flag_off = AgentExecutionPlan {
            publish: false,
            execution_steps: vec![AgentExecutionStep {
                id: "publish_or_queue".to_string(),
                title: "Publish to Confluence".to_string(),
                status: "pending".to_string(),
                detail: None,
            }],
            ..publish_plan
        };
        assert!(!should_publish_after_draft(&publish_flag_off));
    }

    #[test]
    fn copilot_flow_conversation_to_suggestions_to_draft_only_plan() {
        let entries = vec![
            make_entry_with_source(
                "old-1",
                "legacy unrelated discussion about build scripts",
                0,
                "mic",
            ),
            make_entry_with_source(
                "old-2",
                "more unrelated operational notes",
                2 * 60_000,
                "system",
            ),
            make_entry_with_source(
                "new-1",
                "combat balancing goals and damage curve",
                30 * 60_000,
                "mic",
            ),
            make_entry_with_source(
                "new-2",
                "combat balancing enemy hp and stamina loop",
                32 * 60_000,
                "system",
            ),
            make_entry_with_source(
                "new-3",
                "combat balancing follow-up for boss pacing",
                34 * 60_000,
                "mic",
            ),
        ];
        let parse_request = AgentParseCommandRequest {
            command_text: "hey trispr create a gdd draft about combat balancing".to_string(),
            source: Some("test".to_string()),
        };
        let parse = parse_command(&parse_request, &make_wakewords(), &make_keywords());
        assert!(parse.detected);
        assert_eq!(parse.intent, "gdd_generate_publish");
        assert!(!parse.publish_requested);

        let session_request = SearchTranscriptSessionsRequest {
            temporal_hint: parse.temporal_hint.clone(),
            topic_hint: parse.topic_hint.clone(),
            session_gap_minutes: Some(20),
            max_candidates: Some(3),
        };
        let sessions = build_sessions(&entries, 20);
        let scored = score_sessions(&sessions, &session_request);
        assert!(!scored.is_empty());

        let top = scored.first().unwrap();
        let plan_request = AgentBuildExecutionPlanRequest {
            intent: parse.intent.clone(),
            session_id: top.session_id.clone(),
            target_language: "source".to_string(),
            publish: parse.publish_requested,
            command_text: Some(parse.command_text.clone()),
            temporal_hint: parse.temporal_hint.clone(),
            topic_hint: parse.topic_hint.clone(),
            parse_confidence: Some(parse.confidence),
        };
        let plan = default_execution_plan(&plan_request);
        assert!(plan
            .recognized_signals
            .iter()
            .any(|item| item.contains("topic_hint=combat balancing")));
        assert_eq!(plan.analysis_steps.len(), 2);
        assert!(plan.execution_steps.is_empty());
        assert!(!should_publish_after_draft(&plan));
    }

    #[test]
    fn copilot_flow_conversation_to_publish_plan_requires_execution_lane() {
        let parse_request = AgentParseCommandRequest {
            command_text: "hey trispr create gdd about combat and publish to confluence"
                .to_string(),
            source: Some("test".to_string()),
        };
        let parse = parse_command(&parse_request, &make_wakewords(), &make_keywords());
        assert!(parse.detected);
        assert!(parse.publish_requested);

        let plan_request = AgentBuildExecutionPlanRequest {
            intent: parse.intent.clone(),
            session_id: "s_publish".to_string(),
            target_language: "en".to_string(),
            publish: true,
            command_text: Some(parse.command_text.clone()),
            temporal_hint: parse.temporal_hint.clone(),
            topic_hint: parse.topic_hint.clone(),
            parse_confidence: Some(parse.confidence),
        };
        let plan = default_execution_plan(&plan_request);
        assert_eq!(plan.analysis_steps.len(), 2);
        assert_eq!(plan.execution_steps.len(), 1);
        assert!(plan
            .proposed_actions
            .iter()
            .any(|item| item.contains("publish to Confluence")));
        assert!(should_publish_after_draft(&plan));
    }
}
