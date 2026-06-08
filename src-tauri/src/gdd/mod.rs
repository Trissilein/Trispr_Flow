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
use std::collections::HashSet;
use tauri::{AppHandle, Emitter, State};

use crate::modules::{
    canonicalize_module_id, normalize_gdd_module_settings, package as module_package,
    registry as module_registry, GDD_MODULE_ID,
};
use crate::state::{save_settings_file, AppState, Settings};

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

pub(crate) fn require_gdd_module_active_from_ids(
    settings: &Settings,
    installed_package_ids: &HashSet<String>,
) -> Result<(), String> {
    if !module_registry::module_is_installed_with_packages(
        &settings.module_settings,
        GDD_MODULE_ID,
        installed_package_ids,
    ) {
        return Err(
            "GDD module assets are not installed. Install module package 'gdd' first.".to_string(),
        );
    }
    if !settings
        .module_settings
        .enabled_modules
        .iter()
        .any(|module_id| canonicalize_module_id(module_id) == GDD_MODULE_ID)
    {
        return Err("GDD module is disabled. Enable module 'gdd' first.".to_string());
    }
    if !settings.gdd_module_settings.enabled {
        return Err("GDD automation is disabled in settings.".to_string());
    }
    Ok(())
}

pub(crate) fn require_gdd_module_active(
    app: &AppHandle,
    settings: &Settings,
) -> Result<(), String> {
    let modules_dir = crate::paths::resolve_modules_dir(app);
    let installed_package_ids = if modules_dir.is_dir() {
        module_package::scan_installed_module_ids(&modules_dir)?
    } else {
        HashSet::new()
    };
    require_gdd_module_active_from_ids(settings, &installed_package_ids)
}

#[tauri::command]
pub(crate) fn list_gdd_presets(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<GddPreset>, String> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    require_gdd_module_active(&app, &settings)?;
    Ok(list_presets(&settings.gdd_module_settings.preset_clones))
}

#[tauri::command]
pub(crate) fn save_gdd_preset_clone(
    app: AppHandle,
    state: State<'_, AppState>,
    mut preset: GddPresetClone,
) -> Result<Vec<GddPreset>, String> {
    crate::guarded_command!("save_gdd_preset_clone", {
        {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_gdd_module_active(&app, &settings)?;
        }

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
    app: AppHandle,
    state: State<'_, AppState>,
    request: DetectGddPresetRequest,
) -> Result<GddRecognitionResult, String> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    require_gdd_module_active(&app, &settings)?;
    let presets = list_presets(&settings.gdd_module_settings.preset_clones);
    Ok(detect_preset(&request.transcript, &presets))
}

#[tauri::command]
pub(crate) fn generate_gdd_draft(
    app: AppHandle,
    state: State<'_, AppState>,
    request: GenerateGddDraftRequest,
) -> Result<GddDraft, String> {
    crate::guarded_command!("generate_gdd_draft", {
        {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_gdd_module_active(&app, &settings)?;
        }

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
pub(crate) fn validate_gdd_draft(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: GddDraft,
) -> Result<ValidateGddDraftResult, String> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    require_gdd_module_active(&app, &settings)?;
    Ok(validate_draft(&draft))
}

#[tauri::command]
pub(crate) fn render_gdd_for_confluence(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: GddDraft,
) -> Result<String, String> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    require_gdd_module_active(&app, &settings)?;
    Ok(render_storage::render_confluence_storage(&draft))
}

#[tauri::command]
pub(crate) fn render_gdd_markdown(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: GddDraft,
) -> Result<String, String> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    require_gdd_module_active(&app, &settings)?;
    Ok(render_storage::render_markdown(&draft))
}

#[cfg(test)]
mod module_gate_tests {
    use super::require_gdd_module_active_from_ids;
    use crate::modules::GDD_MODULE_ID;
    use crate::state::Settings;
    use std::collections::HashSet;

    fn gdd_settings(installed: bool, module_enabled: bool, setting_enabled: bool) -> Settings {
        let mut settings = Settings::default();
        if installed {
            settings
                .module_settings
                .module_overrides
                .insert("gdd.installed".to_string(), serde_json::json!(true));
        }
        if module_enabled {
            settings
                .module_settings
                .enabled_modules
                .insert(GDD_MODULE_ID.to_string());
        }
        settings.gdd_module_settings.enabled = setting_enabled;
        settings
    }

    #[test]
    fn gdd_gate_requires_installed_package_or_override() {
        let settings = gdd_settings(false, true, true);
        let result = require_gdd_module_active_from_ids(&settings, &HashSet::new());
        assert_eq!(
            result.unwrap_err(),
            "GDD module assets are not installed. Install module package 'gdd' first."
        );
    }

    #[test]
    fn gdd_gate_accepts_validated_package_id() {
        let settings = gdd_settings(false, true, true);
        let installed_package_ids = HashSet::from([GDD_MODULE_ID.to_string()]);
        assert!(require_gdd_module_active_from_ids(&settings, &installed_package_ids).is_ok());
    }

    #[test]
    fn gdd_gate_requires_enabled_module() {
        let settings = gdd_settings(true, false, true);
        let result = require_gdd_module_active_from_ids(&settings, &HashSet::new());
        assert_eq!(
            result.unwrap_err(),
            "GDD module is disabled. Enable module 'gdd' first."
        );
    }

    #[test]
    fn gdd_gate_requires_enabled_setting() {
        let settings = gdd_settings(true, true, false);
        let result = require_gdd_module_active_from_ids(&settings, &HashSet::new());
        assert_eq!(
            result.unwrap_err(),
            "GDD automation is disabled in settings."
        );
    }
}
