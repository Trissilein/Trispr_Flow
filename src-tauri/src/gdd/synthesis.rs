use std::collections::HashMap;

use crate::gdd::extraction::ExtractedFact;
use crate::gdd::{GddDraft, GddPreset, GddSectionDraft};

pub fn synthesize_draft(
    preset: &GddPreset,
    facts: &[ExtractedFact],
    requested_title: Option<&str>,
    chunk_count: usize,
    template_hint: Option<&str>,
    template_label: Option<&str>,
) -> GddDraft {
    let mut by_section: HashMap<String, Vec<String>> = HashMap::new();
    for fact in facts {
        by_section
            .entry(fact.section_id.clone())
            .or_default()
            .push(fact.statement.clone());
    }

    let sections = preset
        .sections
        .iter()
        .map(|section| {
            let lines = by_section.get(&section.id).cloned().unwrap_or_default();
            if lines.is_empty() {
                GddSectionDraft {
                    id: section.id.clone(),
                    title: section.title.clone(),
                    content: "TBD - insufficient evidence in transcript.".to_string(),
                    evidence_gap: true,
                }
            } else {
                let content = lines
                    .into_iter()
                    .take(12)
                    .map(|line| format!("- {}", line))
                    .collect::<Vec<_>>()
                    .join("\n");
                GddSectionDraft {
                    id: section.id.clone(),
                    title: section.title.clone(),
                    content,
                    evidence_gap: false,
                }
            }
        })
        .collect::<Vec<_>>();

    let title = requested_title
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Game Design Document".to_string());

    let mut summary = format!(
        "Generated from {} transcript chunk(s) using preset '{}'.",
        chunk_count, preset.name
    );
    if let Some(label) = template_label
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        summary.push_str(&format!(" Template guidance source: '{}'.", label));
    }
    if let Some(hint) = template_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let lines = hint
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(3)
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            summary.push_str(" Guidance excerpt: ");
            summary.push_str(&lines.join(" | "));
            summary.push('.');
        }
    }

    GddDraft {
        preset_id: preset.id.clone(),
        title,
        summary,
        sections,
        chunk_count,
        generated_at_iso: chrono::Utc::now().to_rfc3339(),
    }
}
