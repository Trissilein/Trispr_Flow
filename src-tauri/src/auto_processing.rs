// Auto-processing pipeline
// After VibeVoice-ASR transcription, auto-generates:
// 1. Chapter segmentation from speaker-diarized output
// 2. Meeting minutes (summary of key points via AI)
// 3. Summary (brief overview via AI)

use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::sidecar::{TranscriptionResponse, TranscriptionSegment};

// ============================================================================
// Types
// ============================================================================

/// Auto-processing result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
  pub chapters: Vec<AutoChapter>,
  pub summary: Option<String>,
  pub minutes: Option<Vec<MinuteItem>>,
  pub processing_time_ms: u64,
}

/// Auto-generated chapter from speaker segments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoChapter {
  pub id: String,
  pub label: String,
  pub start_time: f64,
  pub end_time: f64,
  pub speaker_count: usize,
  pub segment_count: usize,
  pub preview_text: String,
}

/// Meeting minute item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinuteItem {
  pub timestamp: f64,
  pub speaker: String,
  pub content: String,
  pub item_type: MinuteType,
}

/// Type of meeting minute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MinuteType {
  Decision,
  ActionItem,
  Discussion,
  Note,
}

// ============================================================================
// Chapter Generation from Speaker Segments
// ============================================================================

/// Generate chapters from speaker-diarized transcription.
/// A new chapter starts when:
/// - A long silence gap occurs (> threshold)
/// - A speaker change happens after extended monologue
/// - Time exceeds max chapter length
pub fn generate_chapters_from_segments(
  segments: &[TranscriptionSegment],
  silence_threshold_s: f64,
  max_chapter_length_s: f64,
) -> Vec<AutoChapter> {
  if segments.is_empty() {
    return vec![];
  }

  let mut chapters: Vec<AutoChapter> = Vec::new();
  let mut chapter_start_idx = 0;
  let mut chapter_num = 1;

  for i in 1..segments.len() {
    let prev = &segments[i - 1];
    let curr = &segments[i];

    // Check for chapter break conditions
    let gap = curr.start_time - prev.end_time;
    let chapter_duration = curr.start_time - segments[chapter_start_idx].start_time;

    let should_break =
      // Long silence gap
      gap >= silence_threshold_s
      // Chapter exceeds max length
      || chapter_duration >= max_chapter_length_s;

    if should_break {
      // Create chapter from accumulated segments
      let chapter_segments = &segments[chapter_start_idx..i];
      chapters.push(build_chapter(chapter_num, chapter_segments));
      chapter_num += 1;
      chapter_start_idx = i;
    }
  }

  // Add final chapter
  let remaining = &segments[chapter_start_idx..];
  if !remaining.is_empty() {
    chapters.push(build_chapter(chapter_num, remaining));
  }

  info!("Generated {} chapters from {} segments", chapters.len(), segments.len());
  chapters
}

fn build_chapter(num: usize, segments: &[TranscriptionSegment]) -> AutoChapter {
  let start_time = segments.first().map(|s| s.start_time).unwrap_or(0.0);
  let end_time = segments.last().map(|s| s.end_time).unwrap_or(0.0);

  // Count unique speakers in this chapter
  let speakers: std::collections::HashSet<&str> =
    segments.iter().map(|s| s.speaker.as_str()).collect();

  // Build preview text (first 100 chars)
  let full_text: String = segments
    .iter()
    .map(|s| s.text.as_str())
    .collect::<Vec<_>>()
    .join(" ");
  let preview = if full_text.len() > 100 {
    format!("{}...", &full_text[..97])
  } else {
    full_text
  };

  AutoChapter {
    id: format!("auto-chapter-{}", num),
    label: format!("Chapter {}", num),
    start_time,
    end_time,
    speaker_count: speakers.len(),
    segment_count: segments.len(),
    preview_text: preview,
  }
}

// ============================================================================
// Meeting Minutes Extraction
// ============================================================================

/// Extract key meeting minutes from segments.
/// Uses heuristics to identify decisions, action items, and discussion points.
pub fn extract_meeting_minutes(segments: &[TranscriptionSegment]) -> Vec<MinuteItem> {
  let mut minutes: Vec<MinuteItem> = Vec::new();

  // Keywords indicating different types
  let decision_keywords = ["decided", "agreed", "approved", "rejected", "final", "conclusion"];
  let action_keywords = ["will do", "action", "todo", "task", "responsible", "deadline", "by next"];

  for segment in segments {
    let lower_text = segment.text.to_lowercase();

    // Check for decisions
    if decision_keywords.iter().any(|kw| lower_text.contains(kw)) {
      minutes.push(MinuteItem {
        timestamp: segment.start_time,
        speaker: segment.speaker.clone(),
        content: segment.text.clone(),
        item_type: MinuteType::Decision,
      });
      continue;
    }

    // Check for action items
    if action_keywords.iter().any(|kw| lower_text.contains(kw)) {
      minutes.push(MinuteItem {
        timestamp: segment.start_time,
        speaker: segment.speaker.clone(),
        content: segment.text.clone(),
        item_type: MinuteType::ActionItem,
      });
      continue;
    }

    // All other segments are discussion points
    // Only include substantial segments (>20 chars)
    if segment.text.len() > 20 {
      minutes.push(MinuteItem {
        timestamp: segment.start_time,
        speaker: segment.speaker.clone(),
        content: segment.text.clone(),
        item_type: MinuteType::Discussion,
      });
    }
  }

  info!("Extracted {} meeting minute items", minutes.len());
  minutes
}

// ============================================================================
// Summary Generation (placeholder for AI API call)
// ============================================================================

/// Generate a text summary from transcription segments.
/// Currently returns a locally-generated summary.
/// Will be replaced with AI provider API call in v0.7.0.
pub fn generate_summary(segments: &[TranscriptionSegment]) -> String {
  if segments.is_empty() {
    return "No transcription available.".to_string();
  }

  // Count speakers
  let speakers: std::collections::HashSet<&str> =
    segments.iter().map(|s| s.speaker.as_str()).collect();

  let duration = segments
    .last()
    .map(|s| s.end_time)
    .unwrap_or(0.0);

  let total_words: usize = segments
    .iter()
    .map(|s| s.text.split_whitespace().count())
    .sum();

  // Build a simple summary
  format!(
    "Transcription summary: {} speakers, {:.0} seconds duration, {} segments, ~{} words total.",
    speakers.len(),
    duration,
    segments.len(),
    total_words,
  )
}

// ============================================================================
// Full Auto-Processing Pipeline
// ============================================================================

/// Run the full auto-processing pipeline on transcription results
pub fn run_auto_processing(
  transcription: &TranscriptionResponse,
  silence_threshold_s: f64,
  max_chapter_length_s: f64,
) -> ProcessingResult {
  let start = std::time::Instant::now();

  info!(
    "Running auto-processing on {} segments",
    transcription.segments.len()
  );

  // 1. Generate chapters
  let chapters = generate_chapters_from_segments(
    &transcription.segments,
    silence_threshold_s,
    max_chapter_length_s,
  );

  // 2. Extract meeting minutes
  let minutes = extract_meeting_minutes(&transcription.segments);

  // 3. Generate summary
  let summary = generate_summary(&transcription.segments);

  let processing_time_ms = start.elapsed().as_millis() as u64;

  info!(
    "Auto-processing complete: {} chapters, {} minutes, summary generated in {} ms",
    chapters.len(),
    minutes.len(),
    processing_time_ms
  );

  ProcessingResult {
    chapters,
    summary: Some(summary),
    minutes: Some(minutes),
    processing_time_ms,
  }
}
