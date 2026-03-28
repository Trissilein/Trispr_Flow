use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::state::HistoryEntry;

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

fn normalize(text: &str) -> String {
    text.trim().to_lowercase()
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
        joined.truncate(180);
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
    let keyword_hits = intent_keywords
        .iter()
        .filter(|keyword| {
            let normalized_keyword = normalize(keyword);
            !normalized_keyword.is_empty() && normalized.contains(&normalized_keyword)
        })
        .count();

    let intent_detected = wakeword_matched && keyword_hits > 0;
    let intent = if intent_detected {
        "gdd_generate_publish".to_string()
    } else {
        "unknown".to_string()
    };
    let temporal_hint = detect_temporal_hint(&normalized);
    let topic_hint = detect_topic_hint(&normalized);
    let publish_requested = parse_publish_requested(&normalized);

    let mut confidence: f32 = 0.0;
    if wakeword_matched {
        confidence += 0.45;
    }
    if keyword_hits > 0 {
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
        publish_requested,
        wakeword_matched,
        temporal_hint,
        topic_hint,
        reasoning: format!(
            "wakeword={}, keyword_hits={}, publish_hint={}",
            wakeword_matched, keyword_hits, publish_requested
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
