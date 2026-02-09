// Post-processing module for transcript enhancement
//
// This module provides text quality improvements through a multi-stage pipeline:
// 1. Rule-based enhancements (punctuation, capitalization, number normalization)
// 2. Custom vocabulary replacements
// 3. Optional LLM refinement via Claude API

use crate::state::Settings;
use std::collections::HashMap;
use tauri::AppHandle;

/// Main entry point for post-processing transcripts
///
/// Applies enhancements in sequence:
/// - Rule-based fixes (punctuation, capitalization, numbers)
/// - Custom vocabulary replacements
/// - Optional LLM refinement
///
/// Returns the processed text, or an error message if LLM refinement fails.
/// On error, the caller should fallback to the original text.
pub(crate) fn process_transcript(
    text: &str,
    settings: &Settings,
    _app: &AppHandle,
) -> Result<String, String> {
    let mut result = text.to_string();

    // Stage 1: Rule-based enhancements (sync, <5ms)
    if settings.postproc_punctuation_enabled {
        result = apply_punctuation(&result, &settings.postproc_language);
    }
    if settings.postproc_capitalization_enabled {
        result = apply_capitalization(&result, &settings.postproc_language);
    }
    if settings.postproc_numbers_enabled {
        result = normalize_numbers(&result, &settings.postproc_language);
    }

    // Stage 2: Custom Vocabulary (sync, <2ms)
    if settings.postproc_custom_vocab_enabled && !settings.postproc_custom_vocab.is_empty() {
        result = apply_custom_vocabulary(&result, &settings.postproc_custom_vocab);
    }

    // Stage 3: Optional LLM refinement (async, 300-500ms)
    // TODO: Implement in Phase 5
    // if settings.postproc_llm_enabled {
    //     result = refine_with_llm(&result, settings, app)?;
    // }

    Ok(result)
}

/// Apply punctuation rules based on language
///
/// English rules:
/// - Add period at end if missing
/// - Add commas before conjunctions (and, but, or)
/// - Detect questions and add question marks
///
/// German rules:
/// - Similar logic with German-specific conjunction rules
///
/// Multilingual mode ("multi"):
/// - Applies both English AND German rules simultaneously
/// - Ideal for code-switching and bilingual users
fn apply_punctuation(text: &str, lang: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();

    // English-specific rules
    if lang == "en" || lang == "multi" {
        // Add commas before common conjunctions (only if not already present)
        // Check for " and " not preceded by comma
        result = result.replace(", and ", " and "); // Remove existing to standardize
        result = result.replace(" and ", ", and ");

        result = result.replace(", but ", " but ");
        result = result.replace(" but ", ", but ");

        result = result.replace(", or ", " or ");
        result = result.replace(" or ", ", or ");

        // English question detection (case-insensitive)
        let lower_text = result.to_lowercase();
        let is_en_question = lower_text.starts_with("what ")
            || lower_text.starts_with("how ")
            || lower_text.starts_with("why ")
            || lower_text.starts_with("when ")
            || lower_text.starts_with("where ")
            || lower_text.starts_with("who ")
            || lower_text.starts_with("which ")
            || lower_text.starts_with("whose ")
            || lower_text.starts_with("can ")
            || lower_text.starts_with("could ")
            || lower_text.starts_with("would ")
            || lower_text.starts_with("should ")
            || lower_text.starts_with("is ")
            || lower_text.starts_with("are ")
            || lower_text.starts_with("do ")
            || lower_text.starts_with("does ")
            || lower_text.starts_with("did ");

        // For single-language mode, apply punctuation now
        if lang == "en" {
            if !result.ends_with('.') && !result.ends_with('!') && !result.ends_with('?') {
                if is_en_question {
                    result.push('?');
                } else {
                    result.push('.');
                }
            } else if is_en_question && result.ends_with('.') {
                result.pop();
                result.push('?');
            }
        }
    }

    // German-specific rules
    if lang == "de" || lang == "multi" {
        // German-specific rules (similar approach)
        result = result.replace(", und ", " und ");
        result = result.replace(" und ", ", und ");

        result = result.replace(", aber ", " aber ");
        result = result.replace(" aber ", ", aber ");

        result = result.replace(", oder ", " oder ");
        result = result.replace(" oder ", ", oder ");

        // German question detection (case-insensitive)
        let lower_text = result.to_lowercase();
        let is_de_question = lower_text.starts_with("was ")
            || lower_text.starts_with("wie ")
            || lower_text.starts_with("warum ")
            || lower_text.starts_with("wann ")
            || lower_text.starts_with("wo ")
            || lower_text.starts_with("wer ")
            || lower_text.starts_with("welch");

        // For single-language mode, apply punctuation now
        if lang == "de" {
            if !result.ends_with('.') && !result.ends_with('!') && !result.ends_with('?') {
                if is_de_question {
                    result.push('?');
                } else {
                    result.push('.');
                }
            } else if is_de_question && result.ends_with('.') {
                result.pop();
                result.push('?');
            }
        }
    }

    // Multi-language mode: Apply combined question detection
    if lang == "multi" {
        let lower_text = result.to_lowercase();

        // Check both English and German question patterns
        let is_en_question = lower_text.starts_with("what ")
            || lower_text.starts_with("how ")
            || lower_text.starts_with("why ")
            || lower_text.starts_with("when ")
            || lower_text.starts_with("where ")
            || lower_text.starts_with("who ")
            || lower_text.starts_with("which ")
            || lower_text.starts_with("whose ")
            || lower_text.starts_with("can ")
            || lower_text.starts_with("could ")
            || lower_text.starts_with("would ")
            || lower_text.starts_with("should ")
            || lower_text.starts_with("is ")
            || lower_text.starts_with("are ")
            || lower_text.starts_with("do ")
            || lower_text.starts_with("does ")
            || lower_text.starts_with("did ");

        let is_de_question = lower_text.starts_with("was ")
            || lower_text.starts_with("wie ")
            || lower_text.starts_with("warum ")
            || lower_text.starts_with("wann ")
            || lower_text.starts_with("wo ")
            || lower_text.starts_with("wer ")
            || lower_text.starts_with("welch");

        let is_question = is_en_question || is_de_question;

        if !result.ends_with('.') && !result.ends_with('!') && !result.ends_with('?') {
            if is_question {
                result.push('?');
            } else {
                result.push('.');
            }
        } else if is_question && result.ends_with('.') {
            result.pop();
            result.push('?');
        }
    }

    result
}

/// Apply capitalization rules based on language
///
/// Rules:
/// - Capitalize first letter
/// - Capitalize after sentence-ending punctuation (. ! ?)
/// - Language-specific rules (e.g., English "I" always capitalized)
///
/// Multilingual mode ("multi"):
/// - Applies both English AND German capitalization rules
fn apply_capitalization(text: &str, lang: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let mut capitalize_next = true;
    let mut prev_char = ' ';

    for ch in text.chars() {
        if capitalize_next && ch.is_alphabetic() {
            // Capitalize this letter
            for upper_ch in ch.to_uppercase() {
                result.push(upper_ch);
            }
            capitalize_next = false;
        } else {
            result.push(ch);
        }

        // Mark to capitalize after sentence-ending punctuation followed by space
        if (ch == '.' || ch == '!' || ch == '?') && !prev_char.is_numeric() {
            // Avoid capitalizing after decimals like "3.14"
            capitalize_next = true;
        }

        // If we just added a space after punctuation, keep capitalize_next = true
        // Otherwise, if we've seen non-space after punctuation, we've capitalized
        if ch == ' ' && (prev_char == '.' || prev_char == '!' || prev_char == '?') {
            capitalize_next = true;
        }

        prev_char = ch;
    }

    // English-specific: "I" always capitalized as standalone word
    if lang == "en" || lang == "multi" {
        // Replace " i " with " I " (word boundaries)
        result = result.replace(" i ", " I ");
        // Handle "i " at start
        if result.starts_with("i ") {
            result.replace_range(0..1, "I");
        }
        // Handle " i" at end
        if result.ends_with(" i") {
            let len = result.len();
            result.replace_range((len - 1)..len, "I");
        }
        // Handle "i" as only word
        if result == "i" {
            result = "I".to_string();
        }
    }

    // German-specific rules could go here in the future
    // (e.g., noun capitalization with NLP)
    // if lang == "de" || lang == "multi" {
    //     // Future: Capitalize German nouns
    // }

    result
}

/// Normalize number words to digits
///
/// Converts spelled-out numbers to digits:
/// - "one" → "1", "two" → "2", etc.
/// - Future: Date normalization ("twenty twenty six" → "2026")
/// - Future: Currency normalization ("fifty dollars" → "$50")
///
/// Multilingual mode ("multi"):
/// - Applies both English AND German number normalizations
fn normalize_numbers(text: &str, lang: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();

    // English number words (0-20 plus common tens)
    if lang == "en" || lang == "multi" {
        let number_words = [
            (" zero ", " 0 "),
            (" one ", " 1 "),
            (" two ", " 2 "),
            (" three ", " 3 "),
            (" four ", " 4 "),
            (" five ", " 5 "),
            (" six ", " 6 "),
            (" seven ", " 7 "),
            (" eight ", " 8 "),
            (" nine ", " 9 "),
            (" ten ", " 10 "),
            (" eleven ", " 11 "),
            (" twelve ", " 12 "),
            (" thirteen ", " 13 "),
            (" fourteen ", " 14 "),
            (" fifteen ", " 15 "),
            (" sixteen ", " 16 "),
            (" seventeen ", " 17 "),
            (" eighteen ", " 18 "),
            (" nineteen ", " 19 "),
            (" twenty ", " 20 "),
            (" thirty ", " 30 "),
            (" forty ", " 40 "),
            (" fifty ", " 50 "),
            (" sixty ", " 60 "),
            (" seventy ", " 70 "),
            (" eighty ", " 80 "),
            (" ninety ", " 90 "),
            (" hundred ", " 100 "),
            (" thousand ", " 1000 "),
        ];

        // Add spaces at start/end to ensure word boundaries
        let mut working_text = format!(" {} ", result);

        for (word, digit) in &number_words {
            working_text = working_text.replace(word, digit);
        }

        // Remove the added spaces
        result = working_text.trim().to_string();
    }

    // German number words (0-20)
    if lang == "de" || lang == "multi" {
        let number_words = [
            (" null ", " 0 "),
            (" eins ", " 1 "),
            (" zwei ", " 2 "),
            (" drei ", " 3 "),
            (" vier ", " 4 "),
            (" fünf ", " 5 "),
            (" sechs ", " 6 "),
            (" sieben ", " 7 "),
            (" acht ", " 8 "),
            (" neun ", " 9 "),
            (" zehn ", " 10 "),
            (" elf ", " 11 "),
            (" zwölf ", " 12 "),
            (" dreizehn ", " 13 "),
            (" vierzehn ", " 14 "),
            (" fünfzehn ", " 15 "),
            (" sechzehn ", " 16 "),
            (" siebzehn ", " 17 "),
            (" achtzehn ", " 18 "),
            (" neunzehn ", " 19 "),
            (" zwanzig ", " 20 "),
        ];

        let mut working_text = format!(" {} ", result);

        for (word, digit) in &number_words {
            working_text = working_text.replace(word, digit);
        }

        result = working_text.trim().to_string();
    }

    result
}

/// Apply custom vocabulary replacements with word boundary matching
///
/// Uses HashMap for case-sensitive replacements.
/// Word boundary matching prevents partial replacements
/// (e.g., "api" → "API" but "apikey" stays "apikey")
fn apply_custom_vocabulary(text: &str, vocab: &HashMap<String, String>) -> String {
    if text.is_empty() || vocab.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();

    // Apply each vocabulary replacement with word boundary matching
    for (original, replacement) in vocab {
        // Create regex pattern with word boundaries
        // Use regex::escape to safely handle special regex characters in user input
        let pattern = format!(r"\b{}\b", regex::escape(original));

        // Compile regex (in production, we might want to cache these)
        match regex::Regex::new(&pattern) {
            Ok(re) => {
                result = re.replace_all(&result, replacement.as_str()).to_string();
            }
            Err(e) => {
                // If regex compilation fails (shouldn't happen with escaped input),
                // log error and skip this replacement
                use tracing::warn;
                warn!(
                    "Failed to compile regex for custom vocabulary '{}': {}",
                    original, e
                );
            }
        }
    }

    result
}

/// Refine transcript using Claude API
///
/// Sends text to Claude with configurable prompt template.
/// Handles errors gracefully:
/// - Network timeout (10s)
/// - Invalid API key
/// - HTTP errors (401, 429, 500)
///
/// Returns error on failure - caller should fallback to original text.
#[allow(dead_code)]
async fn refine_with_llm(
    text: &str,
    settings: &Settings,
    _app: &AppHandle,
) -> Result<String, String> {
    // TODO: Implement in Phase 5
    // For now, check API key and return error if empty
    if settings.postproc_llm_api_key.is_empty() {
        return Err("LLM API key not configured".to_string());
    }

    // Placeholder - will implement Claude API client in Phase 5
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Punctuation Tests ==========

    #[test]
    fn test_punctuation_adds_period() {
        let input = "hello world";
        let output = apply_punctuation(input, "en");
        assert_eq!(output, "hello world.");
    }

    #[test]
    fn test_punctuation_question_detection() {
        let inputs = vec!["what is your name", "how are you", "why is this"];
        for input in inputs {
            let output = apply_punctuation(input, "en");
            assert!(output.ends_with('?'), "Expected '?' for: {}", input);
        }
    }

    #[test]
    fn test_punctuation_question_replaces_period() {
        let input = "what is your name.";
        let output = apply_punctuation(input, "en");
        assert_eq!(output, "what is your name?");
    }

    #[test]
    fn test_punctuation_adds_commas_before_conjunctions() {
        let input = "I like cats and dogs but not birds or fish";
        let output = apply_punctuation(input, "en");
        assert!(output.contains(", and "));
        assert!(output.contains(", but "));
        assert!(output.contains(", or "));
    }

    #[test]
    fn test_punctuation_preserves_existing() {
        let input = "hello world!";
        let output = apply_punctuation(input, "en");
        assert_eq!(output, "hello world!");
    }

    #[test]
    fn test_punctuation_empty_string() {
        let input = "";
        let output = apply_punctuation(input, "en");
        assert_eq!(output, "");
    }

    #[test]
    fn test_punctuation_german_conjunctions() {
        let input = "ich mag katzen und hunde aber nicht vögel oder fische";
        let output = apply_punctuation(input, "de");
        assert!(output.contains(", und "));
        assert!(output.contains(", aber "));
        assert!(output.contains(", oder "));
    }

    #[test]
    fn test_punctuation_german_question() {
        let input = "wie geht es dir";
        let output = apply_punctuation(input, "de");
        assert_eq!(output, "wie geht es dir?");
    }

    // ========== Capitalization Tests ==========

    #[test]
    fn test_capitalization_first_letter() {
        let input = "hello world";
        let output = apply_capitalization(input, "en");
        assert_eq!(output, "Hello world");
    }

    #[test]
    fn test_capitalization_after_period() {
        let input = "hello. world. test.";
        let output = apply_capitalization(input, "en");
        assert_eq!(output, "Hello. World. Test.");
    }

    #[test]
    fn test_capitalization_after_question_mark() {
        let input = "hello? world? test?";
        let output = apply_capitalization(input, "en");
        assert_eq!(output, "Hello? World? Test?");
    }

    #[test]
    fn test_capitalization_after_exclamation() {
        let input = "hello! world! test!";
        let output = apply_capitalization(input, "en");
        assert_eq!(output, "Hello! World! Test!");
    }

    #[test]
    fn test_capitalization_english_i() {
        let input = "i think i am happy";
        let output = apply_capitalization(input, "en");
        assert!(output.contains(" I "));
    }

    #[test]
    fn test_capitalization_english_i_at_start() {
        let input = "i am here";
        let output = apply_capitalization(input, "en");
        assert!(output.starts_with("I "));
    }

    #[test]
    fn test_capitalization_preserves_mid_sentence_case() {
        let input = "hello World test";
        let output = apply_capitalization(input, "en");
        // Should capitalize first, preserve "World", lowercase "test"
        assert_eq!(output, "Hello World test");
    }

    #[test]
    fn test_capitalization_empty_string() {
        let input = "";
        let output = apply_capitalization(input, "en");
        assert_eq!(output, "");
    }

    // ========== Number Normalization Tests ==========

    #[test]
    fn test_numbers_basic_digits() {
        let input = "I have three apples and five oranges";
        let output = normalize_numbers(input, "en");
        assert_eq!(output, "I have 3 apples and 5 oranges");
    }

    #[test]
    fn test_numbers_zero_to_ten() {
        let input = "zero one two three four five six seven eight nine ten";
        let output = normalize_numbers(input, "en");
        assert_eq!(output, "0 1 2 3 4 5 6 7 8 9 10");
    }

    #[test]
    fn test_numbers_teens() {
        let input = "eleven twelve thirteen fourteen fifteen";
        let output = normalize_numbers(input, "en");
        assert_eq!(output, "11 12 13 14 15");
    }

    #[test]
    fn test_numbers_tens() {
        let input = "twenty thirty forty fifty";
        let output = normalize_numbers(input, "en");
        assert_eq!(output, "20 30 40 50");
    }

    #[test]
    fn test_numbers_preserves_existing_digits() {
        let input = "I have 3 apples and 5 oranges";
        let output = normalize_numbers(input, "en");
        assert_eq!(output, "I have 3 apples and 5 oranges");
    }

    #[test]
    fn test_numbers_word_boundaries() {
        // "one" in "someone" should NOT be replaced
        let input = "someone has one apple";
        let output = normalize_numbers(input, "en");
        assert_eq!(output, "someone has 1 apple");
    }

    #[test]
    fn test_numbers_german() {
        let input = "ich habe drei äpfel und fünf orangen";
        let output = normalize_numbers(input, "de");
        assert_eq!(output, "ich habe 3 äpfel und 5 orangen");
    }

    #[test]
    fn test_numbers_empty_string() {
        let input = "";
        let output = normalize_numbers(input, "en");
        assert_eq!(output, "");
    }

    // ========== Integration Tests ==========

    #[test]
    fn test_full_pipeline_english() {
        let input = "hello world i have three apples";

        // Apply all transformations in sequence
        let result = apply_punctuation(input, "en");
        let result = apply_capitalization(&result, "en");
        let result = apply_numbers(&result, "en");

        assert_eq!(result, "Hello world I have 3 apples.");
    }

    #[test]
    fn test_full_pipeline_question() {
        let input = "what are you doing i have five cats";

        let result = apply_punctuation(input, "en");
        let result = apply_capitalization(&result, "en");
        let result = apply_numbers(&result, "en");

        assert_eq!(result, "What are you doing I have 5 cats?");
    }

    // Helper function alias for consistency
    fn apply_numbers(text: &str, lang: &str) -> String {
        normalize_numbers(text, lang)
    }

    // ========== Multilingual Mode Tests ==========

    #[test]
    fn test_multi_punctuation_en_conjunction() {
        let input = "I like cats and dogs";
        let output = apply_punctuation(input, "multi");
        assert!(output.contains(", and "));
    }

    #[test]
    fn test_multi_punctuation_de_conjunction() {
        let input = "ich mag katzen und hunde";
        let output = apply_punctuation(input, "multi");
        assert!(output.contains(", und "));
    }

    #[test]
    fn test_multi_punctuation_en_question() {
        let input = "what is your name";
        let output = apply_punctuation(input, "multi");
        assert!(output.ends_with('?'));
    }

    #[test]
    fn test_multi_punctuation_de_question() {
        let input = "wie geht es dir";
        let output = apply_punctuation(input, "multi");
        assert!(output.ends_with('?'));
    }

    #[test]
    fn test_multi_capitalization_en_i() {
        let input = "i think i am happy";
        let output = apply_capitalization(input, "multi");
        assert!(output.contains(" I "));
    }

    #[test]
    fn test_multi_numbers_en() {
        let input = "I have three apples";
        let output = normalize_numbers(input, "multi");
        assert_eq!(output, "I have 3 apples");
    }

    #[test]
    fn test_multi_numbers_de() {
        let input = "ich habe drei äpfel";
        let output = normalize_numbers(input, "multi");
        assert_eq!(output, "ich habe 3 äpfel");
    }

    #[test]
    fn test_multi_code_switching() {
        // Mixed English/German sentence
        let input = "I have drei apples and five äpfel";
        let output = normalize_numbers(input, "multi");
        assert_eq!(output, "I have 3 apples and 5 äpfel");
    }

    #[test]
    fn test_multi_full_pipeline_code_switching() {
        // Realistic code-switching scenario
        let input = "ich think we have three äpfel und five oranges";

        let result = apply_punctuation(input, "multi");
        let result = apply_capitalization(&result, "multi");
        let result = apply_numbers(&result, "multi");

        // Should have: capitalized first letter, commas before conjunctions, numbers converted
        assert!(result.starts_with("Ich") || result.starts_with("I"));
        assert!(result.contains(", und ") || result.contains(", and "));
        assert!(result.contains('3'));
        assert!(result.contains('5'));
        assert!(result.ends_with('.'));
    }
}
