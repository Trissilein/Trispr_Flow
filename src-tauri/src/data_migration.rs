use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tracing::{info, warn};

use crate::paths::resolve_base_dir;

/// Migrates data from the legacy `%APPDATA%\com.trispr.flow\` location to the new
/// `%LOCALAPPDATA%\Trispr Flow\` location. Runs at startup; safe to call repeatedly
/// (skipped if new location already contains `settings.json`).
pub(crate) fn migrate_legacy_data(app: &AppHandle) {
    let new_base = resolve_base_dir(app);

    // Already migrated or fresh install — nothing to do.
    if new_base.join("settings.json").exists() {
        return;
    }

    // Collect legacy candidate paths (old Tauri app_config_dir / app_data_dir).
    let mut old_candidates: Vec<PathBuf> = Vec::new();
    if let Ok(p) = app.path().app_config_dir() {
        old_candidates.push(p);
    }
    if let Ok(p) = app.path().app_data_dir() {
        if !old_candidates.contains(&p) {
            old_candidates.push(p);
        }
    }

    for old_base in old_candidates {
        if old_base == new_base {
            continue;
        }
        if !old_base.exists() {
            continue;
        }
        // Only migrate if the old folder actually contains settings.json.
        if !old_base.join("settings.json").exists() {
            continue;
        }

        info!(
            "Migrating Trispr Flow data\n  from: {}\n  to:   {}",
            old_base.display(),
            new_base.display()
        );

        if let Err(e) = copy_dir_recursive(&old_base, &new_base) {
            warn!("Data migration failed: {}. Old data left in place.", e);
        } else {
            info!("Migration complete. Old folder preserved at: {}", old_base.display());
        }
        return; // Only one source needed.
    }
}

/// Recursively copies the contents of `src` into `dst`, skipping entries that
/// already exist in `dst`. Does NOT delete anything in `src`.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if src_path.is_file() && !dst_path.exists() {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
