use crate::paths::resolve_config_path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tauri::AppHandle;
use tracing::warn;

const KEYRING_SERVICE: &str = "com.trispr.flow.confluence";
const FALLBACK_KEYS_FILE: &str = "confluence_keys.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FileKeyStore {
    keys: HashMap<String, String>,
}

fn normalize_secret_id(secret_id: &str) -> Result<String, String> {
    let normalized = secret_id.trim().to_lowercase();
    if normalized.is_empty() {
        return Err("Secret id cannot be empty".to_string());
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Secret id contains unsupported characters".to_string());
    }
    Ok(normalized)
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
    let raw = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize key store: {}", e))?;
    fs::write(path, raw).map_err(|e| format!("Failed to write key store: {}", e))
}

fn try_store_in_keyring(secret_id: &str, secret_value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, secret_id)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry
        .set_password(secret_value)
        .map_err(|e| format!("Failed to store secret in keyring: {}", e))
}

fn try_read_from_keyring(secret_id: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, secret_id)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(format!("Failed to read key from keyring: {}", err)),
    }
}

fn try_delete_from_keyring(secret_id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, secret_id)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(format!("Failed to delete key from keyring: {}", err)),
    }
}

pub fn store_secret(app: &AppHandle, secret_id: &str, value: &str) -> Result<(), String> {
    let secret_id = normalize_secret_id(secret_id)?;
    let value = value.trim();
    if value.is_empty() {
        return Err("Secret value cannot be empty".to_string());
    }

    if let Err(err) = try_store_in_keyring(&secret_id, value) {
        warn!(
            "System keyring unavailable for secret '{}': {}. Using file fallback.",
            secret_id, err
        );
        let mut store = load_file_store(app)?;
        store.keys.insert(secret_id, value.to_string());
        return save_file_store(app, &store);
    }

    let mut store = load_file_store(app)?;
    store.keys.remove(&secret_id);
    save_file_store(app, &store)
}

pub fn read_secret(app: &AppHandle, secret_id: &str) -> Result<Option<String>, String> {
    let secret_id = normalize_secret_id(secret_id)?;
    match try_read_from_keyring(&secret_id) {
        Ok(Some(secret)) if !secret.trim().is_empty() => return Ok(Some(secret)),
        Ok(_) => {}
        Err(err) => {
            warn!(
                "System keyring read unavailable for secret '{}': {}. Using file fallback.",
                secret_id, err
            );
        }
    }

    let store = load_file_store(app)?;
    Ok(store
        .keys
        .get(&secret_id)
        .cloned()
        .filter(|value| !value.trim().is_empty()))
}

pub fn clear_secret(app: &AppHandle, secret_id: &str) -> Result<(), String> {
    let secret_id = normalize_secret_id(secret_id)?;
    if let Err(err) = try_delete_from_keyring(&secret_id) {
        warn!(
            "System keyring delete unavailable for secret '{}': {}. Cleaning file fallback.",
            secret_id, err
        );
    }

    let mut store = load_file_store(app)?;
    store.keys.remove(&secret_id);
    save_file_store(app, &store)
}
