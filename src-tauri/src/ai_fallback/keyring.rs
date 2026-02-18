use crate::paths::resolve_config_path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tauri::AppHandle;
use tracing::warn;

const KEYRING_SERVICE: &str = "com.trispr.flow.ai-fallback";
const FALLBACK_KEYS_FILE: &str = "ai_fallback_keys.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FileKeyStore {
    keys: HashMap<String, String>,
}

fn normalize_provider(provider: &str) -> Result<String, String> {
    let normalized = provider.trim().to_lowercase();
    if matches!(normalized.as_str(), "claude" | "openai" | "gemini") {
        Ok(normalized)
    } else {
        Err(format!("Unknown AI provider: {}", provider))
    }
}

fn fallback_file_path(app: &AppHandle) -> std::path::PathBuf {
    resolve_config_path(app, FALLBACK_KEYS_FILE)
}

fn load_file_store(app: &AppHandle) -> Result<FileKeyStore, String> {
    let path = fallback_file_path(app);
    if !path.exists() {
        return Ok(FileKeyStore::default());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("Failed to read key store: {}", e))?;
    serde_json::from_str(&raw).map_err(|e| format!("Failed to parse key store: {}", e))
}

fn save_file_store(app: &AppHandle, store: &FileKeyStore) -> Result<(), String> {
    let path = fallback_file_path(app);
    let raw =
        serde_json::to_string_pretty(store).map_err(|e| format!("Failed to serialize key store: {}", e))?;
    fs::write(path, raw).map_err(|e| format!("Failed to write key store: {}", e))
}

fn try_store_in_keyring(provider: &str, api_key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, provider)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry
        .set_password(api_key)
        .map_err(|e| format!("Failed to store key in system keyring: {}", e))
}

fn try_read_from_keyring(provider: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, provider)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(format!("Failed to read key from system keyring: {}", err)),
    }
}

fn try_delete_from_keyring(provider: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, provider)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(format!("Failed to delete key from system keyring: {}", err)),
    }
}

pub fn store_api_key(app: &AppHandle, provider: &str, api_key: &str) -> Result<(), String> {
    let provider = normalize_provider(provider)?;
    let key = api_key.trim();
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    if let Err(err) = try_store_in_keyring(&provider, key) {
        warn!(
            "System keyring storage unavailable for provider '{}': {}. Falling back to file storage.",
            provider, err
        );
        let mut store = load_file_store(app)?;
        store.keys.insert(provider, key.to_string());
        return save_file_store(app, &store);
    }

    let mut store = load_file_store(app)?;
    store.keys.remove(&provider);
    save_file_store(app, &store)?;
    Ok(())
}

pub fn read_api_key(app: &AppHandle, provider: &str) -> Result<Option<String>, String> {
    let provider = normalize_provider(provider)?;
    match try_read_from_keyring(&provider) {
        Ok(Some(key)) if !key.trim().is_empty() => return Ok(Some(key)),
        Ok(_) => {}
        Err(err) => {
            warn!(
                "System keyring read unavailable for provider '{}': {}. Falling back to file storage.",
                provider, err
            );
        }
    }

    let store = load_file_store(app)?;
    Ok(store.keys.get(&provider).cloned().filter(|value| !value.trim().is_empty()))
}

pub fn clear_api_key(app: &AppHandle, provider: &str) -> Result<(), String> {
    let provider = normalize_provider(provider)?;
    if let Err(err) = try_delete_from_keyring(&provider) {
        warn!(
            "System keyring delete unavailable for provider '{}': {}. Cleaning file fallback.",
            provider, err
        );
    }

    let mut store = load_file_store(app)?;
    store.keys.remove(&provider);
    save_file_store(app, &store)?;
    Ok(())
}
