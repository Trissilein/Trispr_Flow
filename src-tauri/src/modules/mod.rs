pub mod delivery;
pub mod health;
pub mod lifecycle;
pub mod lifecycle_coordinator;
pub mod package;
pub mod permissions;
pub mod registry;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Module ids removed from the app. Saved settings may still mention them.
const REMOVED_MODULE_IDS: &[&str] = &[
    "analysis",
    "assistant_core",
    "assistant_presence",
    "gdd",
    "input_vision",
    "integrations_confluence",
    "output_video_generation",
    "output_voice_tts",
    "piper_tts",
    "task_capture",
    "workflow_agent",
];

pub fn canonicalize_module_id(module_id: &str) -> &str {
    module_id.trim()
}

fn is_removed_module_key(key: &str) -> bool {
    REMOVED_MODULE_IDS
        .iter()
        .any(|id| key == *id || key.starts_with(&format!("{id}.")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDescriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub state: String, // "not_installed" | "installed" | "enabled" | "active" | "error"
    pub dependencies: Vec<String>,
    pub permissions: Vec<String>,
    pub restart_required: bool,
    pub last_error: Option<String>,
    pub bundled: bool,
    pub core: bool,
    pub toggleable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleHealthStatus {
    pub module_id: String,
    pub state: String, // "ok" | "needs_setup" | "fallback_active" | "local_warning" | "release_blocker" | "degraded" | "error"
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleUpdateInfo {
    pub module_id: String,
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModuleSettings {
    pub enabled_modules: HashSet<String>,
    pub consented_permissions: HashMap<String, HashSet<String>>,
    pub module_overrides: HashMap<String, serde_json::Value>,
}

impl Default for ModuleSettings {
    fn default() -> Self {
        Self {
            enabled_modules: HashSet::new(),
            consented_permissions: HashMap::new(),
            module_overrides: HashMap::new(),
        }
    }
}

pub fn normalize_module_settings(settings: &mut ModuleSettings) {
    let enabled = settings
        .enabled_modules
        .iter()
        .filter_map(|module_id| {
            let normalized = canonicalize_module_id(module_id);
            registry::find_manifest(normalized).map(|_| normalized.to_string())
        })
        .collect::<HashSet<_>>();
    settings.enabled_modules = enabled;

    let mut normalized_permissions: HashMap<String, HashSet<String>> = HashMap::new();
    for (module_id, permissions) in &settings.consented_permissions {
        let normalized_module_id = canonicalize_module_id(module_id).to_string();
        if let Some(manifest) = registry::find_manifest(&normalized_module_id) {
            let allowed = manifest
                .permissions
                .iter()
                .map(|permission| permission.to_string())
                .collect::<HashSet<_>>();
            let kept = permissions
                .iter()
                .filter(|permission| allowed.contains(*permission))
                .cloned()
                .collect::<HashSet<_>>();
            normalized_permissions
                .entry(normalized_module_id)
                .and_modify(|existing| existing.extend(kept.clone()))
                .or_insert(kept);
        }
    }
    settings.consented_permissions = normalized_permissions;

    let normalized_overrides = settings
        .module_overrides
        .iter()
        .filter(|(key, _)| !is_removed_module_key(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<HashMap<_, _>>();
    settings.module_overrides = normalized_overrides;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_removed_analysis_settings() {
        let mut settings = ModuleSettings::default();
        settings.enabled_modules.insert("analysis".to_string());
        settings
            .enabled_modules
            .insert("assistant_core".to_string());
        settings.consented_permissions.insert(
            "analysis".to_string(),
            HashSet::from(["filesystem_history".to_string()]),
        );
        settings.module_overrides.insert(
            "analysis.last_error".to_string(),
            serde_json::Value::String("stale".to_string()),
        );
        settings.module_overrides.insert(
            "assistant_core.last_error".to_string(),
            serde_json::Value::String("stale".to_string()),
        );
        settings.module_overrides.insert(
            "custom_module.option".to_string(),
            serde_json::Value::Bool(true),
        );

        normalize_module_settings(&mut settings);

        assert!(!settings.enabled_modules.contains("analysis"));
        assert!(!settings.enabled_modules.contains("assistant_core"));
        assert!(!settings
            .module_overrides
            .contains_key("assistant_core.last_error"));
        assert!(!settings.consented_permissions.contains_key("analysis"));
        assert!(!settings
            .module_overrides
            .contains_key("analysis.last_error"));
        assert_eq!(
            settings.module_overrides.get("custom_module.option"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[test]
    fn drops_removed_vision_voice_video_settings() {
        let mut settings = ModuleSettings::default();
        let removed = [
            ("input_vision", "screen_capture"),
            ("output_voice_tts", "audio_output"),
            ("output_video_generation", "filesystem_exports"),
            ("piper_tts", "audio_output"),
        ];
        for (module_id, permission) in removed {
            settings.enabled_modules.insert(module_id.to_string());
            settings.consented_permissions.insert(
                module_id.to_string(),
                HashSet::from([permission.to_string()]),
            );
            settings.module_overrides.insert(
                format!("{module_id}.last_error"),
                serde_json::Value::String("stale".to_string()),
            );
        }
        settings.enabled_modules.insert("ai_refinement".to_string());

        normalize_module_settings(&mut settings);

        for (module_id, _) in removed {
            assert!(!settings.enabled_modules.contains(module_id));
            assert!(!settings.consented_permissions.contains_key(module_id));
            assert!(!settings
                .module_overrides
                .contains_key(&format!("{module_id}.last_error")));
        }
        assert!(settings.enabled_modules.contains("ai_refinement"));
    }
}
