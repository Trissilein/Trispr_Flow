use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionPlan {
    pub intent: String,
    pub session_id: String,
    pub session_title: String,
    pub target_language: String,
    pub publish: bool,
    pub steps: Vec<AgentExecutionStep>,
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

        if entry.timestamp_ms.saturating_sub(current_end) > gap_ms {
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
            let score = (temporal * 0.5 + topic * 0.3 + recency * 0.2).clamp(0.0, 1.0);

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
                    "temporal={:.2}, topic={:.2}, recency={:.2}",
                    temporal, topic, recency
                ),
            }
        })
        .collect();

    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    scored.truncate(max_candidates);
    scored
}

pub fn default_execution_plan(request: &AgentBuildExecutionPlanRequest) -> AgentExecutionPlan {
    AgentExecutionPlan {
        intent: request.intent.clone(),
        session_id: request.session_id.clone(),
        session_title: format!("Session {}", request.session_id),
        target_language: request.target_language.clone(),
        publish: request.publish,
        steps: vec![
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
            AgentExecutionStep {
                id: "publish_or_queue".to_string(),
                title: "Publish to Confluence or queue fallback".to_string(),
                status: "pending".to_string(),
                detail: None,
            },
        ],
        summary: format!(
            "Intent={} session={} target_language={} publish={}",
            request.intent, request.session_id, request.target_language, request.publish
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_wakewords() -> Vec<String> {
        vec!["trispr".to_string(), "hey trispr".to_string()]
    }

    fn make_keywords() -> Vec<String> {
        vec!["gdd".to_string(), "game design document".to_string(), "draft".to_string()]
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
            SessionBucket { id: "s1".to_string(), start_ms: 0, end_ms: 0, entries: entries_match },
            SessionBucket { id: "s2".to_string(), start_ms: 0, end_ms: 0, entries: entries_no_match },
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
}
