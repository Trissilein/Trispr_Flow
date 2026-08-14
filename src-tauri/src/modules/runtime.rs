//! Lifecycle bookkeeping for module sidecar processes.
//!
//! A *module sidecar* is a self-contained executable shipped inside a
//! downloaded module package (`kind = "sidecar"` or `kind = "runtime"`).
//! Today's sidecars (e.g. `opus`) are invoked as short-lived one-shot
//! processes per call and don't register here. This module only tracks
//! whatever long-running sidecars a future module registers, so app exit
//! can clean them up.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::AppHandle;

use crate::paths::resolve_modules_dir;
use crate::state::AppState;

/// Resolve the path to a file inside an installed module package.
/// `rel` is a relative subpath from the module root, e.g. `"bin/trispr-opus.exe"`.
pub fn resolve_module_binary(app: &AppHandle, module_id: &str, rel: &str) -> PathBuf {
    resolve_modules_dir(app).join(module_id).join(rel)
}

/// Stop all running module sidecars. Called from `cleanup_managed_processes` on app exit.
pub fn terminate_all_module_sidecars(state: &AppState) {
    let mut map = state
        .module_sidecars
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    for (_, mut child) in map.drain() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Type alias so callers importing only this module don't need to spell out
/// the full `std::collections` path.
pub type ModuleSidecarMap = Mutex<HashMap<String, std::process::Child>>;

/// Construct the default (empty) sidecar map for `AppState` initialization.
pub fn default_sidecar_map() -> ModuleSidecarMap {
    Mutex::new(HashMap::new())
}
