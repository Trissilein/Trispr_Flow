use std::collections::HashSet;

use super::{ModuleDescriptor, ModuleSettings};

#[derive(Debug, Clone)]
pub struct ModuleManifest {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub bundled: bool,
    pub core_always_on: bool,
    pub installed_by_default: bool,
    pub restart_required_on_enable: bool,
    pub dependencies: &'static [&'static str],
    pub permissions: &'static [&'static str],
}

pub fn manifests() -> Vec<ModuleManifest> {
    vec![
        ModuleManifest {
            id: "gdd",
            name: "GDD Automation",
            version: "0.2.0",
            bundled: true,
            core_always_on: true,
            installed_by_default: true,
            restart_required_on_enable: false,
            dependencies: &["integrations_confluence"],
            permissions: &[],
        },
        ModuleManifest {
            id: "analysis",
            name: "Analysis",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: false,
            restart_required_on_enable: true,
            dependencies: &[],
            permissions: &["filesystem_history", "filesystem_exports"],
        },
        ModuleManifest {
            id: "integrations_confluence",
            name: "Confluence Integration",
            version: "0.2.0",
            bundled: true,
            core_always_on: true,
            installed_by_default: true,
            restart_required_on_enable: false,
            dependencies: &[],
            permissions: &[],
        },
        ModuleManifest {
            id: "workflow_agent",
            name: "Workflow Agent",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: true,
            restart_required_on_enable: false,
            dependencies: &["gdd", "integrations_confluence"],
            permissions: &[
                "filesystem_history",
                "filesystem_exports",
                "network_confluence",
                "keyring_access",
            ],
        },
        ModuleManifest {
            id: "input_vision",
            name: "Screen Vision Input",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: false,
            restart_required_on_enable: false,
            dependencies: &["workflow_agent"],
            permissions: &["screen_capture"],
        },
        ModuleManifest {
            id: "output_voice_tts",
            name: "Voice Output (TTS)",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: false,
            restart_required_on_enable: false,
            dependencies: &["workflow_agent"],
            permissions: &["audio_output"],
        },
    ]
}

pub fn find_manifest(module_id: &str) -> Option<ModuleManifest> {
    manifests()
        .into_iter()
        .find(|manifest| manifest.id == module_id)
}

fn last_error_for(settings: &ModuleSettings, module_id: &str) -> Option<String> {
    settings
        .module_overrides
        .get(&format!("{}.last_error", module_id))
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

pub fn set_last_error(settings: &mut ModuleSettings, module_id: &str, error: &str) {
    settings.module_overrides.insert(
        format!("{}.last_error", module_id),
        serde_json::Value::String(error.to_string()),
    );
}

fn module_is_enabled(settings: &ModuleSettings, module_id: &str) -> bool {
    if let Some(manifest) = find_manifest(module_id) {
        if manifest.core_always_on {
            return true;
        }
    }
    settings.enabled_modules.contains(module_id)
}

pub fn module_is_installed(settings: &ModuleSettings, module_id: &str) -> bool {
    if let Some(manifest) = find_manifest(module_id) {
        if manifest.installed_by_default {
            return true;
        }
        return settings
            .module_overrides
            .get(&format!("{}.installed", module_id))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
    }
    false
}

fn dependency_is_satisfied(settings: &ModuleSettings, module_id: &str) -> bool {
    module_is_enabled(settings, module_id) || module_is_installed(settings, module_id)
}

pub fn missing_dependencies(settings: &ModuleSettings, module_id: &str) -> Vec<String> {
    let Some(manifest) = find_manifest(module_id) else {
        return Vec::new();
    };

    manifest
        .dependencies
        .iter()
        .filter(|dependency| !dependency_is_satisfied(settings, dependency))
        .map(|dependency| dependency.to_string())
        .collect()
}

pub fn modules_as_descriptors(settings: &ModuleSettings) -> Vec<ModuleDescriptor> {
    manifests()
        .into_iter()
        .map(|manifest| {
            let installed = module_is_installed(settings, manifest.id);
            let dependencies_satisfied = manifest
                .dependencies
                .iter()
                .all(|dependency| dependency_is_satisfied(settings, dependency));
            let last_error = last_error_for(settings, manifest.id);
            let state = if manifest.core_always_on && installed {
                "active"
            } else if !installed {
                "not_installed"
            } else if last_error.is_some() {
                "error"
            } else if module_is_enabled(settings, manifest.id) {
                if dependencies_satisfied {
                    "active"
                } else {
                    "error"
                }
            } else {
                "installed"
            };

            ModuleDescriptor {
                id: manifest.id.to_string(),
                name: manifest.name.to_string(),
                version: manifest.version.to_string(),
                state: state.to_string(),
                dependencies: manifest
                    .dependencies
                    .iter()
                    .map(|v| v.to_string())
                    .collect(),
                permissions: manifest.permissions.iter().map(|v| v.to_string()).collect(),
                restart_required: manifest.restart_required_on_enable,
                last_error,
                bundled: manifest.bundled,
                core: manifest.core_always_on,
                toggleable: !manifest.core_always_on,
            }
        })
        .collect()
}

pub fn known_permissions(module_id: &str) -> HashSet<String> {
    find_manifest(module_id)
        .map(|manifest| {
            manifest
                .permissions
                .iter()
                .map(|permission| permission.to_string())
                .collect()
        })
        .unwrap_or_default()
}
