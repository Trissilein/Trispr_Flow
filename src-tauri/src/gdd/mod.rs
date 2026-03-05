pub mod confluence;
pub mod extraction;
pub mod publish_queue;
pub mod presets;
pub mod recognition;
pub mod render_storage;
pub mod synthesis;
pub mod template_sources;
pub mod validation;

use serde::{Deserialize, Serialize};

pub use presets::{
    list_presets, preset_by_id, universal_preset, GddPreset, GddPresetClone,
};
pub use recognition::detect_preset;
pub use template_sources::{load_template_from_file, GddTemplateSourceResult};
pub use validation::validate_draft;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddRecognitionCandidate {
    pub preset_id: String,
    pub label: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddRecognitionResult {
    pub suggested_preset_id: String,
    pub confidence: f32,
    pub candidates: Vec<GddRecognitionCandidate>,
    pub reasoning_snippets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddSectionDraft {
    pub id: String,
    pub title: String,
    pub content: String,
    pub evidence_gap: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GddDraft {
    pub preset_id: String,
    pub title: String,
    pub summary: String,
    pub sections: Vec<GddSectionDraft>,
    pub chunk_count: usize,
    pub generated_at_iso: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectGddPresetRequest {
    pub transcript: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateGddDraftRequest {
    pub transcript: String,
    pub preset_id: Option<String>,
    pub title: Option<String>,
    pub max_chunk_chars: Option<usize>,
    pub template_hint: Option<String>,
    pub template_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateGddDraftResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

pub fn generate_draft(request: &GenerateGddDraftRequest, clones: &[GddPresetClone]) -> GddDraft {
    let all_presets = list_presets(clones);
    let recognition = recognition::detect_preset(&request.transcript, &all_presets);
    let selected_preset_id = request
        .preset_id
        .as_ref()
        .filter(|preset_id| all_presets.iter().any(|preset| preset.id == **preset_id))
        .cloned()
        .unwrap_or_else(|| recognition.suggested_preset_id.clone());
    let preset = preset_by_id(&selected_preset_id, &all_presets).unwrap_or_else(universal_preset);

    let max_chunk_chars = request.max_chunk_chars.unwrap_or(3_500).clamp(800, 8_000);
    let extraction = extraction::extract_facts(&request.transcript, max_chunk_chars);
    synthesis::synthesize_draft(
        &preset,
        &extraction.facts,
        request.title.as_deref(),
        extraction.chunk_count,
        request.template_hint.as_deref(),
        request.template_label.as_deref(),
    )
}
