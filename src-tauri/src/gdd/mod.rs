pub mod confluence;
pub mod extraction;
pub mod presets;
pub mod publish_queue;
pub mod recognition;
pub mod render_storage;
pub mod synthesis;
pub mod template_sources;
pub mod validation;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::modules::normalize_gdd_module_settings;
use crate::state::{save_settings_file, AppState};

pub use presets::{list_presets, preset_by_id, universal_preset, GddPreset, GddPresetClone};
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

#[tauri::command]
pub(crate) fn list_gdd_presets(state: State<'_, AppState>) -> Vec<GddPreset> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    list_presets(&settings.gdd_module_settings.preset_clones)
}

#[tauri::command]
pub(crate) fn save_gdd_preset_clone(
    app: AppHandle,
    state: State<'_, AppState>,
    mut preset: GddPresetClone,
) -> Result<Vec<GddPreset>, String> {
    crate::guarded_command!("save_gdd_preset_clone", {
        preset.id = preset.id.trim().to_lowercase();
        if preset.id.is_empty() {
            return Err("Preset clone id cannot be empty.".to_string());
        }
        if preset.section_order.is_empty() {
            return Err("Preset clone requires at least one section.".to_string());
        }
        if preset.name.trim().is_empty() {
            return Err("Preset clone name cannot be empty.".to_string());
        }

        let snapshot = {
            let mut settings = state
                .settings
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(existing) = settings
                .gdd_module_settings
                .preset_clones
                .iter_mut()
                .find(|candidate| candidate.id == preset.id)
            {
                *existing = preset;
            } else {
                settings.gdd_module_settings.preset_clones.push(preset);
            }
            normalize_gdd_module_settings(&mut settings.gdd_module_settings);
            settings.clone()
        };

        save_settings_file(&app, &snapshot)?;
        let _ = app.emit("settings-changed", snapshot.clone());

        Ok(list_presets(&snapshot.gdd_module_settings.preset_clones))
    })
}

#[tauri::command]
pub(crate) fn detect_gdd_preset(
    state: State<'_, AppState>,
    request: DetectGddPresetRequest,
) -> GddRecognitionResult {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let presets = list_presets(&settings.gdd_module_settings.preset_clones);
    detect_preset(&request.transcript, &presets)
}

#[tauri::command]
pub(crate) fn generate_gdd_draft(
    app: AppHandle,
    state: State<'_, AppState>,
    request: GenerateGddDraftRequest,
) -> Result<GddDraft, String> {
    crate::guarded_command!("generate_gdd_draft", {
        let _ = app.emit(
            "gdd:generation-started",
            serde_json::json!({ "preset": request.preset_id }),
        );
        let draft = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            generate_draft(&request, &settings.gdd_module_settings.preset_clones)
        };
        let markdown_preview = render_storage::render_markdown(&draft);
        let confluence_storage = render_storage::render_confluence_storage(&draft);
        let _ = app.emit(
            "gdd:generation-progress",
            serde_json::json!({
                "stage": "synthesized",
                "chunk_count": draft.chunk_count,
                "markdown_chars": markdown_preview.len(),
                "storage_chars": confluence_storage.len(),
            }),
        );
        let _ = app.emit("gdd:generation-finished", &draft);
        Ok(draft)
    })
}

#[tauri::command]
pub(crate) fn validate_gdd_draft(draft: GddDraft) -> ValidateGddDraftResult {
    validate_draft(&draft)
}

#[tauri::command]
pub(crate) fn render_gdd_for_confluence(draft: GddDraft) -> String {
    render_storage::render_confluence_storage(&draft)
}

#[tauri::command]
pub(crate) fn render_gdd_markdown(draft: GddDraft) -> String {
    render_storage::render_markdown(&draft)
}
