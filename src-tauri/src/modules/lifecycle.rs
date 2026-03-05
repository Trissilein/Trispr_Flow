use super::{permissions, registry, ModuleSettings};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleLifecycleResult {
    pub module_id: String,
    pub state: String,
    pub restart_required: bool,
    pub missing_dependencies: Vec<String>,
    pub missing_permissions: Vec<String>,
    pub message: String,
}

pub fn enable_module(
    settings: &mut ModuleSettings,
    module_id: &str,
    grant_permissions: &[String],
) -> Result<ModuleLifecycleResult, String> {
    let manifest = registry::find_manifest(module_id)
        .ok_or_else(|| format!("Unknown module '{}'.", module_id))?;

    if !registry::module_is_installed(settings, module_id) {
        registry::set_last_error(settings, module_id, "Module is not installed.");
        return Err(format!(
            "Module '{}' is not installed. Install module assets first.",
            module_id
        ));
    }

    if !grant_permissions.is_empty() {
        permissions::grant_permissions(settings, module_id, grant_permissions);
    }

    let missing_dependencies = registry::missing_dependencies(settings, module_id);
    if !missing_dependencies.is_empty() {
        let message = format!(
            "Missing dependencies for '{}': {}",
            module_id,
            missing_dependencies.join(", ")
        );
        registry::set_last_error(settings, module_id, &message);
        return Err(message);
    }

    let missing_permissions = permissions::missing_permissions(settings, module_id);
    if !missing_permissions.is_empty() {
        let message = format!(
            "Missing consent for '{}': {}",
            module_id,
            missing_permissions.join(", ")
        );
        registry::set_last_error(settings, module_id, &message);
        return Err(message);
    }

    settings.enabled_modules.insert(module_id.to_string());
    settings
        .module_overrides
        .remove(&format!("{}.last_error", module_id));

    Ok(ModuleLifecycleResult {
        module_id: module_id.to_string(),
        state: "active".to_string(),
        restart_required: manifest.restart_required_on_enable,
        missing_dependencies: Vec::new(),
        missing_permissions: Vec::new(),
        message: "Module enabled.".to_string(),
    })
}

pub fn disable_module(settings: &mut ModuleSettings, module_id: &str) -> Result<ModuleLifecycleResult, String> {
    if registry::find_manifest(module_id).is_none() {
        return Err(format!("Unknown module '{}'.", module_id));
    }

    settings.enabled_modules.remove(module_id);
    settings
        .module_overrides
        .remove(&format!("{}.last_error", module_id));

    Ok(ModuleLifecycleResult {
        module_id: module_id.to_string(),
        state: "installed".to_string(),
        restart_required: false,
        missing_dependencies: Vec::new(),
        missing_permissions: Vec::new(),
        message: "Module disabled. Data preserved.".to_string(),
    })
}
