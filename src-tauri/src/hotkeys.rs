use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub error: Option<String>,
    pub formatted: Option<String>,
}

/// Keys that can be registered without a modifier (media / volume keys).
/// Windows RegisterHotKey supports modifier-less registration for these.
const MODIFIER_FREE_KEYS: &[&str] = &[
    "MediaPlayPause",
    "MediaStop",
    "MediaTrackNext",
    "MediaTrackPrevious",
    "AudioVolumeUp",
    "AudioVolumeDown",
    "AudioVolumeMute",
];

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

    // Allow modifier-free registration for media/volume keys
    if parts.len() == 1 {
        let single = parts[0];
        if MODIFIER_FREE_KEYS
            .iter()
            .any(|k| k.eq_ignore_ascii_case(single))
        {
            return ValidationResult {
                valid: true,
                error: None,
                formatted: Some(single.to_string()),
            };
        }
        return ValidationResult {
            valid: false,
            error: Some(
                "Hotkey must include at least one modifier (e.g., Ctrl, Shift, Alt)".to_string(),
            ),
            formatted: None,
        };
    }

    // Valid modifiers
    let valid_modifiers = [
        "CommandOrControl",
        "CmdOrCtrl",
        "Command",
        "Cmd",
        "Control",
        "Ctrl",
        "Alt",
        "Option",
        "AltGr",
        "Shift",
        "Super",
        "Meta",
    ];

    // Validate each part except the last (which should be the key)
    let key_part = parts.last().unwrap();
    let modifier_parts = &parts[..parts.len() - 1];

    for modifier in modifier_parts {
        if !valid_modifiers
            .iter()
            .any(|m| m.eq_ignore_ascii_case(modifier))
        {
            return ValidationResult {
                valid: false,
                error: Some(format!(
                    "Invalid modifier: '{}'. Valid modifiers: Ctrl, Shift, Alt, Command, etc.",
                    modifier
                )),
                formatted: None,
            };
        }
    }

    // Validate key part — must not be empty and must not be a modifier itself
    if key_part.is_empty() {
        return ValidationResult {
            valid: false,
            error: Some("Missing key after modifiers".to_string()),
            formatted: None,
        };
    }
    if valid_modifiers
        .iter()
        .any(|m| m.eq_ignore_ascii_case(key_part))
    {
        return ValidationResult {
            valid: false,
            error: Some(
                "Hotkey needs a main key, not just modifiers. Example: Ctrl+Shift+Y".to_string(),
            ),
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

    let formatted_parts: Vec<String> = parts
        .iter()
        .map(|part| {
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
        })
        .collect();

    formatted_parts.join("+")
}

#[tauri::command]
pub(crate) fn validate_hotkey(key: String) -> ValidationResult {
    validate_hotkey_format(&key)
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
}
