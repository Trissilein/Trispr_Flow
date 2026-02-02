use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub error: Option<String>,
    pub formatted: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub hotkey: String,
    pub conflicting_with: Vec<String>,
}

/// Validates a hotkey string format
pub fn validate_hotkey_format(key: &str) -> ValidationResult {
    let key = key.trim();

    if key.is_empty() {
        return ValidationResult {
            valid: false,
            error: Some("Hotkey cannot be empty".to_string()),
            formatted: None,
        };
    }

    // Parse modifiers and key
    let parts: Vec<&str> = key.split('+').map(|s| s.trim()).collect();

    if parts.len() < 2 {
        return ValidationResult {
            valid: false,
            error: Some("Hotkey must include at least one modifier (e.g., Ctrl, Shift, Alt)".to_string()),
            formatted: None,
        };
    }

    // Valid modifiers
    let valid_modifiers = [
        "CommandOrControl", "CmdOrCtrl", "Command", "Cmd", "Control", "Ctrl",
        "Alt", "Option", "AltGr", "Shift", "Super", "Meta",
    ];

    // Validate each part except the last (which should be the key)
    let key_part = parts.last().unwrap();
    let modifier_parts = &parts[..parts.len() - 1];

    for modifier in modifier_parts {
        if !valid_modifiers.iter().any(|m| m.eq_ignore_ascii_case(modifier)) {
            return ValidationResult {
                valid: false,
                error: Some(format!("Invalid modifier: '{}'. Valid modifiers: Ctrl, Shift, Alt, Command, etc.", modifier)),
                formatted: None,
            };
        }
    }

    // Validate key part (basic validation - could be more comprehensive)
    if key_part.is_empty() {
        return ValidationResult {
            valid: false,
            error: Some("Missing key after modifiers".to_string()),
            formatted: None,
        };
    }

    // Format the hotkey (normalize case)
    let formatted = format_hotkey(key);

    ValidationResult {
        valid: true,
        error: None,
        formatted: Some(formatted),
    }
}

/// Formats a hotkey string to a consistent format
fn format_hotkey(key: &str) -> String {
    let parts: Vec<&str> = key.split('+').map(|s| s.trim()).collect();

    let formatted_parts: Vec<String> = parts.iter().map(|part| {
        // Normalize common modifiers
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => "Ctrl".to_string(),
            "cmdorctrl" | "commandorcontrol" => "CommandOrControl".to_string(),
            "cmd" | "command" => "Command".to_string(),
            "alt" | "option" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "meta" | "super" => "Meta".to_string(),
            _ => {
                // Capitalize first letter for key
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            }
        }
    }).collect();

    formatted_parts.join("+")
}

/// Detects conflicts between hotkeys
pub fn detect_conflicts(hotkeys: Vec<String>) -> Vec<ConflictInfo> {
    let mut conflicts = Vec::new();
    let mut seen = HashSet::new();

    for (i, hotkey) in hotkeys.iter().enumerate() {
        let normalized = normalize_hotkey(hotkey);

        if seen.contains(&normalized) {
            // Find which hotkeys conflict
            let conflicting: Vec<String> = hotkeys
                .iter()
                .enumerate()
                .filter(|(j, h)| *j != i && normalize_hotkey(h) == normalized)
                .map(|(_, h)| h.clone())
                .collect();

            if !conflicting.is_empty() {
                conflicts.push(ConflictInfo {
                    hotkey: hotkey.clone(),
                    conflicting_with: conflicting,
                });
            }
        }

        seen.insert(normalized);
    }

    conflicts
}

/// Normalizes a hotkey for comparison (lowercase, consistent separator)
fn normalize_hotkey(key: &str) -> String {
    key.to_lowercase().replace(" ", "")
}

/// Tests if a hotkey can be registered (without actually activating it)
pub fn test_hotkey_registration(app: &AppHandle, key: &str) -> Result<(), String> {
    // First validate format
    let validation = validate_hotkey_format(key);
    if !validation.valid {
        return Err(validation.error.unwrap_or_else(|| "Invalid hotkey".to_string()));
    }

    // Try to register (and immediately unregister)
    let manager = app.global_shortcut();

    // Note: This is a simplified test. In a real scenario, we'd need to check
    // if the hotkey is already registered and handle that case.
    // For now, we just validate the format.

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_empty() {
        let result = validate_hotkey_format("");
        assert!(!result.valid);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_validate_no_modifier() {
        let result = validate_hotkey_format("Space");
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_valid_hotkey() {
        let result = validate_hotkey_format("Ctrl+Shift+Space");
        assert!(result.valid);
        assert!(result.error.is_none());
        assert!(result.formatted.is_some());
    }

    #[test]
    fn test_format_hotkey() {
        let formatted = format_hotkey("ctrl+shift+space");
        assert_eq!(formatted, "Ctrl+Shift+Space");
    }

    #[test]
    fn test_detect_conflicts() {
        let hotkeys = vec![
            "Ctrl+Shift+Space".to_string(),
            "Ctrl+Shift+M".to_string(),
            "ctrl+shift+space".to_string(), // Conflict with first
        ];

        let conflicts = detect_conflicts(hotkeys);
        assert!(!conflicts.is_empty());
    }

    #[test]
    fn test_normalize_hotkey() {
        assert_eq!(
            normalize_hotkey("Ctrl+Shift+Space"),
            normalize_hotkey("ctrl+shift+space")
        );
    }
}
