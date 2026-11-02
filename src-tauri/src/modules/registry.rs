use std::collections::HashSet;

use super::{canonicalize_module_id, ModuleDescriptor, ModuleSettings};

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
}

pub fn manifests() -> Vec<ModuleManifest> {
    vec![
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
        },
        ModuleManifest {
            id: "input_vision",
            name: "Screen Vision Input",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: false,
            restart_required_on_enable: false,
            dependencies: &[],
            permissions: &["screen_capture"],
            surface: "assistant",
        },
        ModuleManifest {
            id: "output_voice_tts",
            name: "Voice Output (TTS)",
            version: "0.1.0",
            bundled: true,
            core_always_on: false,
            installed_by_default: true,
            restart_required_on_enable: false,
            dependencies: &[],
            permissions: &["audio_output"],
            surface: "assistant",
        },
        ModuleManifest {
            // Phase 1b: installed_by_default=false — hyperframes is now a
            // downloadable runtime module (module-sidecars/video_gen/).
            id: "output_video_generation",
            name: "Video Generation",
            version: "0.1.0",
            bundled: false,
            core_always_on: false,
            installed_by_default: false,
            restart_required_on_enable: false,
            dependencies: &[],
            permissions: &["filesystem_exports"],
            surface: "shared",
        },
        ModuleManifest {
            // Code-out module: the `trispr-opus` sidecar + bundled FFmpeg are
            // downloaded on demand (not in the lean core). No restart needed —
            // the session manager resolves the sidecar per flush/finalize.
            id: "opus",
            name: "Opus Export",
            version: "0.1.0",
            bundled: false,
            core_always_on: false,
            installed_by_default: false,
            restart_required_on_enable: false,
            dependencies: &[],
            permissions: &[],
            surface: "shared",
        },
        ModuleManifest {
            // Runtime module: piper.exe + DLLs + espeak data downloaded on demand.
            // Core resolves the binary from the module install dir; graceful no-op
            // when absent (speak_tts returns an error pointing to the Modules tab).
            id: "piper_tts",
            name: "Piper Local TTS",
            version: "2023.11.14-2",
            bundled: false,
            core_always_on: false,
            installed_by_default: false,
            restart_required_on_enable: false,
            dependencies: &[],
            permissions: &["audio_output"],
            surface: "shared",
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

pub fn module_is_installed_with_packages(
    settings: &ModuleSettings,
    module_id: &str,
    installed_package_ids: &HashSet<String>,
) -> bool {
    let module_id = canonicalize_module_id(module_id);
    if let Some(manifest) = find_manifest(module_id) {
        if manifest.installed_by_default {
            return true;
        }
        if installed_package_ids
            .iter()
            .any(|installed| canonicalize_module_id(installed) == module_id)
        {
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

fn dependency_is_satisfied_with_packages(
    settings: &ModuleSettings,
    module_id: &str,
    installed_package_ids: &HashSet<String>,
) -> bool {
    module_is_enabled(settings, module_id)
        || module_is_installed_with_packages(settings, module_id, installed_package_ids)
}

pub fn missing_dependencies_with_packages(
    settings: &ModuleSettings,
    module_id: &str,
    installed_package_ids: &HashSet<String>,
) -> Vec<String> {
    let Some(manifest) = find_manifest(module_id) else {
        return Vec::new();
    };

    manifest
        .dependencies
        .iter()
        .filter(|dependency| {
            !dependency_is_satisfied_with_packages(settings, dependency, installed_package_ids)
        })
        .map(|dependency| dependency.to_string())
        .collect()
}

pub fn modules_as_descriptors(settings: &ModuleSettings) -> Vec<ModuleDescriptor> {
    modules_as_descriptors_with_packages(settings, &HashSet::new())
}

pub fn modules_as_descriptors_with_packages(
    settings: &ModuleSettings,
    installed_package_ids: &HashSet<String>,
) -> Vec<ModuleDescriptor> {
    manifests()
        .into_iter()
        .map(|manifest| {
            let installed =
                module_is_installed_with_packages(settings, manifest.id, installed_package_ids);
            let dependencies_satisfied = manifest.dependencies.iter().all(|dependency| {
                dependency_is_satisfied_with_packages(settings, dependency, installed_package_ids)
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_is_known_on_demand_module() {
        // The opus module must be in the registry so downloaded packages with
        // id "opus" pass `known_module_id` validation. It is on-demand: not
        // bundled, not installed by default.
        let manifest = find_manifest("opus").expect("opus manifest should exist");
        assert!(!manifest.bundled);
        assert!(!manifest.core_always_on);
        assert!(!manifest.installed_by_default);

        let settings = ModuleSettings::default();
        let descriptor = modules_as_descriptors(&settings)
            .into_iter()
            .find(|module| module.id == "opus")
            .expect("opus descriptor should exist");
        assert_eq!(descriptor.state, "not_installed");
        assert!(descriptor.toggleable);

        // Present once a validated package is installed.
        let installed = HashSet::from(["opus".to_string()]);
        let descriptor = modules_as_descriptors_with_packages(&settings, &installed)
            .into_iter()
            .find(|module| module.id == "opus")
            .expect("opus descriptor should exist");
        assert_eq!(descriptor.state, "installed");
    }

    #[test]
    fn analysis_is_no_longer_registered() {
        assert!(find_manifest("analysis").is_none());
        assert!(!modules_as_descriptors(&ModuleSettings::default())
            .iter()
            .any(|module| module.id == "analysis"));
    }

    #[test]
    fn removed_modules_are_no_longer_registered() {
        for id in [
            "gdd",
            "integrations_confluence",
            "task_capture",
            "assistant_core",
            "assistant_presence",
            "workflow_agent",
        ] {
            assert!(find_manifest(id).is_none(), "{id} must not be registered");
        }
    }

    #[test]
    fn piper_tts_is_known_on_demand_module() {
        // piper_tts must be in the registry so downloaded packages with id
        // "piper_tts" pass `known_module_id` validation. On-demand: not
        // bundled, not installed by default.
        let manifest = find_manifest("piper_tts").expect("piper_tts manifest should exist");
        assert!(!manifest.bundled);
        assert!(!manifest.core_always_on);
        assert!(!manifest.installed_by_default);

        let settings = ModuleSettings::default();
        let descriptor = modules_as_descriptors(&settings)
            .into_iter()
            .find(|module| module.id == "piper_tts")
            .expect("piper_tts descriptor should exist");
        assert_eq!(descriptor.state, "not_installed");
        assert!(descriptor.toggleable);

        let installed = HashSet::from(["piper_tts".to_string()]);
        let descriptor = modules_as_descriptors_with_packages(&settings, &installed)
            .into_iter()
            .find(|module| module.id == "piper_tts")
            .expect("piper_tts descriptor should exist");
        assert_eq!(descriptor.state, "installed");
    }
}
