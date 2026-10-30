//! Helpers for resolving installed module binaries.

use std::path::PathBuf;

use tauri::AppHandle;

use crate::paths::resolve_modules_dir;

/// Resolve the path to a file inside an installed module package.
/// `rel` is a relative subpath from the module root, e.g. `"bin/trispr-opus.exe"`.
pub fn resolve_module_binary(app: &AppHandle, module_id: &str, rel: &str) -> PathBuf {
    resolve_modules_dir(app).join(module_id).join(rel)
}
