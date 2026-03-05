pub mod health;
pub mod lifecycle;
pub mod permissions;
pub mod registry;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::gdd::GddPresetClone;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleHealthStatus {
    pub module_id: String,
    pub state: String, // "ok" | "degraded" | "error"
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GddModuleSettings {
    pub enabled: bool,
    pub default_preset_id: String,
    pub detect_preset_automatically: bool,
    pub prefer_one_click_publish: bool,
    pub preset_clones: Vec<GddPresetClone>,
}

impl Default for GddModuleSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            default_preset_id: "universal_strict".to_string(),
            detect_preset_automatically: true,
            prefer_one_click_publish: false,
            preset_clones: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfluenceSettings {
    pub enabled: bool,
    pub site_base_url: String,
    pub oauth_cloud_id: String,
    pub default_space_key: String,
    pub api_user_email: String,
    pub default_parent_page_id: String,
    pub auth_mode: String, // "oauth" | "api_token"
    pub routing_memory: HashMap<String, String>, // key -> page_id
}

impl Default for ConfluenceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            site_base_url: String::new(),
            oauth_cloud_id: String::new(),
            default_space_key: String::new(),
            api_user_email: String::new(),
            default_parent_page_id: String::new(),
            auth_mode: "oauth".to_string(),
            routing_memory: HashMap::new(),
        }
    }
}

pub fn normalize_module_settings(settings: &mut ModuleSettings) {
    let enabled = settings
        .enabled_modules
        .iter()
        .filter_map(|module_id| registry::find_manifest(module_id).map(|_| module_id.clone()))
        .collect::<HashSet<_>>();
    settings.enabled_modules = enabled;

    let mut normalized_permissions: HashMap<String, HashSet<String>> = HashMap::new();
    for (module_id, permissions) in &settings.consented_permissions {
        if let Some(manifest) = registry::find_manifest(module_id) {
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
            normalized_permissions.insert(module_id.clone(), kept);
        }
    }
    settings.consented_permissions = normalized_permissions;
}

pub fn normalize_gdd_module_settings(settings: &mut GddModuleSettings) {
    if settings.default_preset_id.trim().is_empty() {
        settings.default_preset_id = "universal_strict".to_string();
    }
    settings.preset_clones.retain(|preset| !preset.id.trim().is_empty());
}

pub fn normalize_confluence_settings(settings: &mut ConfluenceSettings) {
    settings.site_base_url = settings.site_base_url.trim().trim_end_matches('/').to_string();
    settings.oauth_cloud_id = settings.oauth_cloud_id.trim().to_string();
    settings.default_space_key = settings.default_space_key.trim().to_string();
    settings.api_user_email = settings.api_user_email.trim().to_string();
    settings.default_parent_page_id = settings.default_parent_page_id.trim().to_string();
    settings.auth_mode = match settings.auth_mode.as_str() {
        "api_token" => "api_token".to_string(),
        _ => "oauth".to_string(),
    };
    settings
        .routing_memory
        .retain(|key, value| !key.trim().is_empty() && !value.trim().is_empty());
}
