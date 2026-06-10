use super::{
    canonicalize_module_id, permissions, registry, ModuleDescriptor, ModuleHealthStatus,
    ModuleSettings, ModuleUpdateInfo,
};
use std::collections::HashSet;

pub fn get_health_with_packages(
    settings: &ModuleSettings,
    module_id: Option<&str>,
    installed_package_ids: &HashSet<String>,
) -> Vec<ModuleHealthStatus> {
    let descriptors =
        registry::modules_as_descriptors_with_packages(settings, installed_package_ids);
    descriptors
        .into_iter()
        .filter(|descriptor| {
            module_id
                .map(|target| target == descriptor.id.as_str())
                .unwrap_or(true)
        })
        .map(|descriptor| classify_module_health(settings, descriptor))
        .collect()
}

fn module_enabled_by_intent(settings: &ModuleSettings, descriptor: &ModuleDescriptor) -> bool {
    descriptor.core
        || settings
            .enabled_modules
            .iter()
            .any(|enabled| canonicalize_module_id(enabled) == descriptor.id)
}

fn classify_module_health(
    settings: &ModuleSettings,
    descriptor: ModuleDescriptor,
) -> ModuleHealthStatus {
    let enabled_by_intent = module_enabled_by_intent(settings, &descriptor);
    let missing_permissions = if enabled_by_intent {
        permissions::missing_permissions(settings, &descriptor.id)
    } else {
        Vec::new()
    };

    let (state, detail) = match descriptor.state.as_str() {
        "active" if !missing_permissions.is_empty() => (
            "needs_setup",
            format!(
                "Consent required before this module is available: {}.",
                missing_permissions.join(", ")
            ),
        ),
        "active" => ("ok", "Module active.".to_string()),
        "installed" => (
            "ok",
            "Optional module installed and disabled until enabled.".to_string(),
        ),
        "not_installed" if descriptor.core => (
            "release_blocker",
            "Core module assets are not installed.".to_string(),
        ),
        "not_installed" if enabled_by_intent => (
            "needs_setup",
            "Module assets must be installed before this enabled module can run.".to_string(),
        ),
        "not_installed" => (
            "ok",
            "Optional module is not installed and is not required by the current flow.".to_string(),
        ),
        "error" if descriptor.core => (
            "release_blocker",
            descriptor
                .last_error
                .unwrap_or_else(|| "Core module is in error state.".to_string()),
        ),
        "error" if enabled_by_intent => (
            "needs_setup",
            descriptor
                .last_error
                .unwrap_or_else(|| "Enabled module needs setup before it can run.".to_string()),
        ),
        "error" => (
            "ok",
            descriptor
                .last_error
                .map(|error| {
                    format!(
                        "Optional module is disabled; previous setup issue is not active: {}",
                        error
                    )
                })
                .unwrap_or_else(|| {
                    "Optional module is disabled; previous setup issue is not active.".to_string()
                }),
        ),
        _ => ("error", "Unknown module state.".to_string()),
    };

    ModuleHealthStatus {
        module_id: descriptor.id,
        state: state.to_string(),
        detail,
    }
}

pub fn check_updates_with_packages(
    settings: &ModuleSettings,
    module_id: Option<&str>,
    installed_package_ids: &HashSet<String>,
) -> Vec<ModuleUpdateInfo> {
    registry::modules_as_descriptors_with_packages(settings, installed_package_ids)
        .into_iter()
        .filter(|descriptor| {
            module_id
                .map(|target| target == descriptor.id.as_str())
                .unwrap_or(true)
        })
        .map(|descriptor| ModuleUpdateInfo {
            module_id: descriptor.id,
            current_version: descriptor.version.clone(),
            latest_version: descriptor.version,
            update_available: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{registry, ASSISTANT_CORE_MODULE_ID};
    use std::collections::HashSet;

    fn health_for(module_id: &str, settings: &ModuleSettings) -> ModuleHealthStatus {
        get_health_with_packages(settings, Some(module_id), &HashSet::new())
            .into_iter()
            .next()
            .expect("module health should exist")
    }

    fn health_for_with_packages(
        module_id: &str,
        settings: &ModuleSettings,
        installed_package_ids: HashSet<String>,
    ) -> ModuleHealthStatus {
        get_health_with_packages(settings, Some(module_id), &installed_package_ids)
            .into_iter()
            .next()
            .expect("module health should exist")
    }

    #[test]
    fn fresh_default_optional_modules_do_not_report_global_setup_errors() {
        let settings = ModuleSettings::default();
        let health = get_health_with_packages(&settings, None, &HashSet::new());

        assert!(!health.is_empty());
        assert!(health.iter().all(|entry| entry.state == "ok"));
    }

    #[test]
    fn stale_error_on_disabled_optional_module_is_not_active_failure() {
        let mut settings = ModuleSettings::default();
        registry::set_last_error(
            &mut settings,
            ASSISTANT_CORE_MODULE_ID,
            "Missing consent for assistant_core: filesystem_history",
        );

        let health = health_for(ASSISTANT_CORE_MODULE_ID, &settings);

        assert_eq!(health.state, "ok");
        assert!(health.detail.contains("previous setup issue is not active"));
    }

    #[test]
    fn enabled_module_without_required_consent_needs_setup() {
        let mut settings = ModuleSettings::default();
        settings
            .enabled_modules
            .insert(ASSISTANT_CORE_MODULE_ID.to_string());

        let health = health_for_with_packages(
            ASSISTANT_CORE_MODULE_ID,
            &settings,
            HashSet::from(["gdd".to_string()]),
        );

        assert_eq!(health.state, "needs_setup");
        assert!(health.detail.contains("filesystem_history"));
    }

    #[test]
    fn enabled_not_installed_optional_module_needs_setup() {
        let mut settings = ModuleSettings::default();
        settings.enabled_modules.insert("gdd".to_string());

        let health = health_for("gdd", &settings);

        assert_eq!(health.state, "needs_setup");
        assert!(health.detail.contains("assets"));
    }

    #[test]
    fn core_error_is_release_blocker() {
        let settings = ModuleSettings::default();
        let descriptor = ModuleDescriptor {
            id: "trispr_core".to_string(),
            name: "Trispr Core".to_string(),
            version: "0.0.0".to_string(),
            state: "error".to_string(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
            restart_required: false,
            last_error: Some("Whisper runtime missing.".to_string()),
            bundled: true,
            core: true,
            toggleable: false,
            surface: "core".to_string(),
            assistant_capable: false,
            assistant_actions: Vec::new(),
        };

        let health = classify_module_health(&settings, descriptor);

        assert_eq!(health.state, "release_blocker");
        assert_eq!(health.detail, "Whisper runtime missing.");
    }
}
