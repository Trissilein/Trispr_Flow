use crate::modules::TaskCaptureSettings;

pub const TASK_CAPTURE_KEYWORDS: &[&str] = &[
    "erinnere mich",
    "trag ein",
    "trag auf liste",
    "auf meine liste",
    "auf die agenda",
    "auf meine todo liste",
    "add to list",
    "add to my agenda",
    "put on my todo",
    "remind me",
];
pub const TASK_CAPTURE_FILLERS: &[&str] = &["daran", "dass", "an"];
pub const TASK_CAPTURE_REFINEMENT_PROMPT: &str = "Du bist ein Task-Formatierer. Formuliere den folgenden Sprachtext als klaren, konkreten Task in einem Satz. Antworte NUR mit dem formatierten Task, nichts anderes.";

pub fn find_matching_route<'a>(
    command_text: &str,
    settings: &'a TaskCaptureSettings,
) -> Option<&'a crate::modules::TaskCaptureRoute> {
    let lowered = command_text.to_lowercase();
    settings.routes.iter().find(|route| {
        route.keywords.iter().any(|kw| {
            let kw_lower = kw.to_lowercase();
            match settings.match_mode.as_str() {
                "exact" => {
                    let words: Vec<&str> = lowered.split_whitespace().collect();
                    let kw_words: Vec<&str> = kw_lower.split_whitespace().collect();
                    words
                        .windows(kw_words.len())
                        .any(|w| w == kw_words.as_slice())
                }
                _ => lowered.contains(&kw_lower),
            }
        })
    })
}

pub fn extract_task_text(command_text: &str) -> String {
    let trimmed = command_text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let lowered = trimmed.to_lowercase();
    let mut remainder = TASK_CAPTURE_KEYWORDS
        .iter()
        .filter_map(|keyword| lowered.find(keyword).map(|idx| (idx, *keyword)))
        .min_by_key(|(idx, _)| *idx)
        .map(|(idx, keyword)| &trimmed[idx + keyword.len()..])
        .unwrap_or(trimmed)
        .trim_start_matches(|ch: char| {
            ch.is_whitespace() || matches!(ch, ',' | ':' | ';' | '.' | '!' | '?' | '-')
        })
        .trim();

    loop {
        let Some(first_word) = remainder.split_whitespace().next() else {
            break;
        };
        let normalized = crate::normalize_assistant_action_text(first_word);
        if !TASK_CAPTURE_FILLERS.contains(&normalized.as_str()) {
            break;
        }
        remainder = remainder[first_word.len()..].trim_start_matches(|ch: char| {
            ch.is_whitespace() || matches!(ch, ',' | ':' | ';' | '.' | '!' | '?' | '-')
        });
    }

    remainder
        .trim()
        .trim_matches(|ch: char| {
            ch.is_whitespace() || matches!(ch, ',' | ':' | ';' | '.' | '!' | '?')
        })
        .to_string()
}

pub fn refine_task_text(
    app: &tauri::AppHandle,
    settings: &crate::state::Settings,
    raw_text: &str,
    custom_prompt: Option<&str>,
) -> String {
    let fallback = raw_text.trim().to_string();
    if fallback.is_empty() || !settings.ai_fallback.enabled {
        return fallback;
    }
    if !settings.workflow_agent.online_enabled
        && !crate::ai_provider_is_local(&settings.ai_fallback.provider)
    {
        return fallback;
    }

    let setup = match crate::prepare_refinement(app, settings) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!("reminder_capture refinement unavailable: {}", error);
            return fallback;
        }
    };

    let mut options = setup.options.clone();
    options.max_tokens = options.max_tokens.clamp(64, 192);
    options.custom_prompt = Some(
        custom_prompt
            .unwrap_or(TASK_CAPTURE_REFINEMENT_PROMPT)
            .to_string(),
    );
    options.prompt_profile = "custom".to_string();
    options.enforce_language_guard = false;

    match setup
        .provider
        .refine_transcript(&fallback, &setup.model, &options, &setup.api_key)
    {
        Ok(result) => {
            let refined = result.text.trim();
            if refined.is_empty() {
                fallback
            } else {
                refined.to_string()
            }
        }
        Err(error) => {
            tracing::warn!("reminder_capture refinement failed: {}", error);
            fallback
        }
    }
}

pub fn post_task_to_endpoint(text: &str, endpoint: &str) -> Result<(), String> {
    tracing::info!("[task_capture] POST to endpoint: {}", endpoint);
    match ureq::post(endpoint)
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(10))
        .send_json(serde_json::json!({ "text": text }))
    {
        Ok(response) => {
            tracing::info!("[task_capture] POST success: status {}", response.status());
            Ok(())
        }
        Err(ureq::Error::Status(code, response)) => {
            let err = crate::format_ureq_status_error("Task capture POST", code, response);
            tracing::error!("[task_capture] POST failed: {}", err);
            Err(err)
        }
        Err(ureq::Error::Transport(transport)) => {
            let err = format!("Task capture POST failed for {}: {}", endpoint, transport);
            tracing::error!("[task_capture] {}", err);
            Err(err)
        }
    }
}

pub fn task_capture_enabled(settings: &crate::state::Settings) -> bool {
    settings
        .module_settings
        .enabled_modules
        .contains("task_capture")
}

#[cfg(test)]
mod tests {
    use super::extract_task_text;

    #[test]
    fn extracts_reminder_text_after_keyword_and_fillers() {
        assert_eq!(
            extract_task_text("Trispr erinnere mich daran Fred morgen anzurufen"),
            "Fred morgen anzurufen"
        );
    }

    #[test]
    fn extracts_reminder_text_for_german_phrase() {
        assert_eq!(
            extract_task_text("Trispr trag ein Milch und Brot kaufen"),
            "Milch und Brot kaufen"
        );
    }

    #[test]
    fn extracts_reminder_text_for_english_phrase() {
        assert_eq!(
            extract_task_text("Trispr remind me to send invoice"),
            "to send invoice"
        );
    }

    #[test]
    fn extracts_task_from_add_to_my_agenda() {
        let result = extract_task_text("Trispr add to my agenda review the PR");
        assert!(!result.is_empty());
        assert!(result.contains("review"));
    }
}
