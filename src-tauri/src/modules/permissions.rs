use std::collections::HashSet;

use super::{registry, ModuleSettings};

pub fn missing_permissions(settings: &ModuleSettings, module_id: &str) -> Vec<String> {
    let required = registry::known_permissions(module_id);
    if required.is_empty() {
        return Vec::new();
    }
    let granted = settings
        .consented_permissions
        .get(module_id)
        .cloned()
        .unwrap_or_default();

    let mut missing = required
        .difference(&granted)
        .cloned()
        .collect::<Vec<String>>();
    missing.sort();
    missing
}

pub fn grant_permissions(settings: &mut ModuleSettings, module_id: &str, permissions: &[String]) {
    let allowed = registry::known_permissions(module_id);
    if allowed.is_empty() {
        return;
    }
    let granted = settings
        .consented_permissions
        .entry(module_id.to_string())
        .or_insert_with(HashSet::new);

    for permission in permissions {
        if allowed.contains(permission) {
            granted.insert(permission.clone());
        }
    }
}
