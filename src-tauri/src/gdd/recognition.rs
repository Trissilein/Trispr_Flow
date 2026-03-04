use crate::gdd::{GddPreset, GddRecognitionCandidate, GddRecognitionResult};

fn keyword_hits(text: &str, keyword: &str) -> u32 {
    let normalized_keyword = keyword.trim().to_lowercase();
    if normalized_keyword.is_empty() {
        return 0;
    }
    text.match_indices(normalized_keyword.as_str()).count() as u32
}

pub fn detect_preset(transcript: &str, presets: &[GddPreset]) -> GddRecognitionResult {
    let lower = transcript.to_lowercase();
    let mut scored = presets
        .iter()
        .map(|preset| {
            let hits = preset
                .keywords
                .iter()
                .map(|keyword| keyword_hits(&lower, keyword))
                .sum::<u32>();
            let score = if preset.keywords.is_empty() {
                0.0
            } else {
                hits as f32 / preset.keywords.len() as f32
            };
            (preset, hits, score)
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let top = scored.first();
    let second = scored.get(1);

    let suggested = top
        .map(|(preset, _hits, _score)| preset.id.clone())
        .unwrap_or_else(|| "universal_strict".to_string());

    let confidence = match (top, second) {
        (Some((_preset, top_hits, top_score)), Some((_second_preset, second_hits, second_score))) => {
            if *top_hits == 0 {
                0.2
            } else {
                let margin = (top_score - second_score).max(0.0);
                let hit_ratio = (*top_hits as f32 / (*top_hits + *second_hits).max(1) as f32).clamp(0.0, 1.0);
                (0.45 + margin * 0.35 + hit_ratio * 0.2).clamp(0.0, 1.0)
            }
        }
        (Some((_preset, top_hits, _top_score)), None) => {
            if *top_hits > 0 {
                0.75
            } else {
                0.25
            }
        }
        _ => 0.2,
    };

    let candidates = scored
        .iter()
        .take(3)
        .map(|(preset, _hits, score)| GddRecognitionCandidate {
            preset_id: preset.id.clone(),
            label: preset.name.clone(),
            score: (*score).clamp(0.0, 1.0),
        })
        .collect::<Vec<_>>();

    let mut reasoning_snippets = Vec::new();
    if let Some((preset, hits, score)) = top {
        reasoning_snippets.push(format!(
            "Top preset '{}' matched {} keyword hits (score {:.2}).",
            preset.name, hits, score
        ));
    } else {
        reasoning_snippets.push("No preset signals found; defaulting to universal preset.".to_string());
    }

    if confidence < 0.5 {
        reasoning_snippets.push(
            "Low confidence: similar scores or limited domain evidence. Allow manual override."
                .to_string(),
        );
    }

    GddRecognitionResult {
        suggested_preset_id: suggested,
        confidence,
        candidates,
        reasoning_snippets,
    }
}
