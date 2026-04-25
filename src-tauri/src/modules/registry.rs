use std::collections::HashSet;

use super::{
    canonicalize_module_id, AssistantActionDescriptor, ModuleDescriptor, ModuleSettings,
    ASSISTANT_CORE_MODULE_ID, ASSISTANT_PRESENCE_MODULE_ID,
};

#[derive(Debug, Clone, Copy)]
pub struct AssistantActionManifest {
    pub id: &'static str,
    pub label: &'static str,
    pub risk_level: &'static str,
    pub requires_online: bool,
    pub allowlist_eligible: bool,
}

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
    pub surface: &'static str,
    pub assistant_capable: bool,
    pub assistant_actions: &'static [AssistantActionManifest],
}

pub fn manifests() -> Vec<ModuleManifest> {
    const GDD_ACTIONS: &[AssistantActionManifest] = &[AssistantActionManifest {
        id: "gdd.generate_publish",
        label: "Generate GDD Draft",
        risk_level: "medium",
        requires_online: false,
        allowlist_eligible: false,
    }];
    const ASSISTANT_CORE_ACTIONS: &[AssistantActionManifest] = &[
        AssistantActionManifest {
            id: "web.search",
            label: "Web Search",
            risk_level: "low",
            requires_online: true,
            allowlist_eligible: true,
        },
        AssistantActionManifest {
            id: "app.open",
            label: "Open App",
            risk_level: "low",
            requires_online: false,
            allowlist_eligible: true,
        },
        AssistantActionManifest {
            id: "module.open",
            label: "Open Trispr Surface",
            risk_level: "low",
            requires_online: false,
            allowlist_eligible: true,
        },
        AssistantActionManifest {
            id: "gdd.generate_publish",
            label: "Generate GDD Draft",
            risk_level: "medium",
            requires_online: false,
            allowlist_eligible: false,
        },
    ];

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
            surface: "shared",
            assistant_capable: true,
            assistant_actions: GDD_ACTIONS,
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
            surface: "shared",
            assistant_capable: false,
            assistant_actions: &[],
        },
        ModuleManifest {
            id: "ai_refinement",
            name: "AI Refinement",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: true,
            restart_required_on_enable: false,
            dependencies: &[],
            permissions: &[],
            surface: "shared",
            assistant_capable: true,
            assistant_actions: &[],
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
            surface: "shared",
            assistant_capable: true,
            assistant_actions: &[],
        },
        ModuleManifest {
            id: ASSISTANT_CORE_MODULE_ID,
            name: "Assistant Core",
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
            surface: "assistant",
            assistant_capable: true,
            assistant_actions: ASSISTANT_CORE_ACTIONS,
        },
        ModuleManifest {
            id: ASSISTANT_PRESENCE_MODULE_ID,
            name: "Assistant Presence",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: true,
            restart_required_on_enable: false,
            dependencies: &[ASSISTANT_CORE_MODULE_ID],
            permissions: &[],
            surface: "ui",
            assistant_capable: false,
            assistant_actions: &[],
        },
        ModuleManifest {
            id: "input_vision",
            name: "Screen Vision Input",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: false,
            restart_required_on_enable: false,
            dependencies: &[ASSISTANT_CORE_MODULE_ID],
            permissions: &["screen_capture"],
            surface: "assistant",
            assistant_capable: true,
            assistant_actions: &[],
        },
        ModuleManifest {
            id: "output_voice_tts",
            name: "Voice Output (TTS)",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: true,
            restart_required_on_enable: false,
            dependencies: &[ASSISTANT_CORE_MODULE_ID],
            permissions: &["audio_output"],
            surface: "assistant",
            assistant_capable: true,
            assistant_actions: &[],
        },
        ModuleManifest {
            // Phase 1a: installed_by_default=true because hyperframes+Node are
            // present in the dev environment. Phase 1b will flip this to false
            // and introduce an explicit install flow via video_install_sidecar.
            id: "output_video_generation",
            name: "Video Generation",
            version: "0.1.0",
            bundled: false,
            core_always_on: false,
            installed_by_default: true,
            restart_required_on_enable: false,
            dependencies: &[],
            permissions: &["filesystem_exports"],
            surface: "shared",
            assistant_capable: false,
            assistant_actions: &[],
        },
    ]
}

pub fn find_manifest(module_id: &str) -> Option<ModuleManifest> {
    let module_id = canonicalize_module_id(module_id);
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
    let module_id = canonicalize_module_id(module_id);
    if let Some(manifest) = find_manifest(module_id) {
        if manifest.core_always_on {
            return true;
        }
    }
    settings
        .enabled_modules
        .iter()
        .any(|enabled| canonicalize_module_id(enabled) == module_id)
}

pub fn module_is_installed(settings: &ModuleSettings, module_id: &str) -> bool {
    let module_id = canonicalize_module_id(module_id);
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
                surface: manifest.surface.to_string(),
                assistant_capable: manifest.assistant_capable,
                assistant_actions: manifest
                    .assistant_actions
                    .iter()
                    .map(|action| AssistantActionDescriptor {
                        id: action.id.to_string(),
                        label: action.label.to_string(),
                        risk_level: action.risk_level.to_string(),
                        requires_online: action.requires_online,
                        allowlist_eligible: action.allowlist_eligible,
                    })
                    .collect(),
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
