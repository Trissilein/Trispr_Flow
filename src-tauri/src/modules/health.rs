use super::{registry, ModuleHealthStatus, ModuleSettings, ModuleUpdateInfo};

pub fn get_health(settings: &ModuleSettings, module_id: Option<&str>) -> Vec<ModuleHealthStatus> {
    let descriptors = registry::modules_as_descriptors(settings);
    descriptors
        .into_iter()
        .filter(|descriptor| {
            module_id
                .map(|target| target == descriptor.id.as_str())
                .unwrap_or(true)
        })
        .map(|descriptor| {
            let (state, detail) = match descriptor.state.as_str() {
                "active" => ("ok", "Module active.".to_string()),
                "installed" => ("degraded", "Installed but disabled.".to_string()),
                "not_installed" => ("degraded", "Module assets not installed.".to_string()),
                "error" => (
                    "error",
                    descriptor
                        .last_error
                        .unwrap_or_else(|| "Module is in error state.".to_string()),
                ),
                _ => ("error", "Unknown module state.".to_string()),
            };

            ModuleHealthStatus {
                module_id: descriptor.id,
                state: state.to_string(),
                detail,
            }
        })
        .collect()
}

pub fn check_updates(settings: &ModuleSettings, module_id: Option<&str>) -> Vec<ModuleUpdateInfo> {
    registry::modules_as_descriptors(settings)
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
