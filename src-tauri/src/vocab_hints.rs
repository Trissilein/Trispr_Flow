//! Vocabulary hints for Whisper and the LLM refinement, built from the learned vocabulary.
//!
//! Sources: `vocab_terms` (terms to keep), `edit_substitutions` with `promoted` (heard -> intended
//! pairs with count/recency, so they can be ranked) and `postproc_custom_vocab` (hand-maintained
//! find-replace rules, ranked last).

use crate::state::{EditSubstitution, Settings};
use std::collections::{HashMap, HashSet};

/// Whisper keeps only the LAST 224 prompt tokens (whisper.cpp logs "will use only the last N
/// tokens"). Rare words cost 2-4 tokens each, so ~650 chars stay inside the window; the prompt is
/// ordered most important LAST so an overflow still drops the weakest terms first.
const WHISPER_PROMPT_MAX_CHARS: usize = 650;
/// LLM refinement: at most this many correction pairs (<= `REFINE_PAIRS_MAX_CHARS` chars), and
/// pairs plus preserved terms share `REFINE_TOTAL_MAX_CHARS`.
const REFINE_MAX_PAIRS: usize = 12;
const REFINE_PAIRS_MAX_CHARS: usize = 250;
const REFINE_TOTAL_MAX_CHARS: usize = 600;

/// Collapse all whitespace (incl. newlines/control gaps) so a term can never break the request body.
fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Append `item` to the comma list unless it would exceed `max_chars`. Returns whether it fit.
fn push_within(out: &mut String, item: &str, max_chars: usize) -> bool {
    let delim = if out.is_empty() { "" } else { ", " };
    if out.chars().count() + delim.len() + item.chars().count() > max_chars {
        return false;
    }
    out.push_str(delim);
    out.push_str(item);
    true
}

/// Terms ranked for a size-limited prompt: learned terms first (most often corrected, then most
/// recent), then terms without correction history (manual or learned before counts were kept),
/// newest first. Case-insensitive duplicates keep their first spelling.
pub(crate) fn ranked_terms(terms: &[String], subs: &[EditSubstitution]) -> Vec<String> {
    let mut meta: HashMap<String, (u32, u64)> = HashMap::new();
    for sub in subs.iter().filter(|sub| sub.promoted) {
        let entry = meta.entry(clean(&sub.to).to_lowercase()).or_insert((0, 0));
        entry.0 = entry.0.max(sub.count);
        entry.1 = entry.1.max(sub.last_seen_ms);
    }

    let mut seen = HashSet::new();
    let mut learned: Vec<(String, (u32, u64))> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    for term in terms {
        let term = clean(term);
        if term.is_empty() || !seen.insert(term.to_lowercase()) {
            continue;
        }
        match meta.get(&term.to_lowercase()) {
            Some(stats) => learned.push((term, *stats)),
            None => plain.push(term),
        }
    }
    learned.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    plain.reverse(); // appended in learning order -> newest first

    learned
        .into_iter()
        .map(|(term, _)| term)
        .chain(plain)
        .collect()
}

/// Heard -> intended pairs ranked by count, then recency; hand-maintained rules follow in a fixed
/// (alphabetical) order. A pair already present in another casing is listed once.
pub(crate) fn ranked_pairs(
    subs: &[EditSubstitution],
    rules: &HashMap<String, String>,
) -> Vec<(String, String)> {
    let mut learned: Vec<&EditSubstitution> = subs.iter().filter(|sub| sub.promoted).collect();
    learned.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.last_seen_ms.cmp(&a.last_seen_ms))
            .then_with(|| a.from.cmp(&b.from))
    });
    let mut manual: Vec<(&String, &String)> = rules.iter().collect();
    manual.sort();

    let candidates = learned
        .into_iter()
        .map(|sub| (&sub.from, &sub.to))
        .chain(manual);

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (from, to) in candidates {
        let (from, to) = (clean(from), clean(to));
        if from.is_empty() || to.is_empty() || from == to {
            continue;
        }
        if seen.insert((from.to_lowercase(), to.to_lowercase())) {
            out.push((from, to));
        }
    }
    out
}

/// Whisper initial prompt (CLI `--prompt` and server `prompt` field): the ranked terms,
/// comma-separated, within the token window; most important term last. `None` without terms.
pub(crate) fn whisper_prompt(settings: &Settings) -> Option<String> {
    let mut picked: Vec<String> = Vec::new();
    let mut len = 0usize;
    for term in ranked_terms(&settings.vocab_terms, &settings.edit_substitutions) {
        let add = term.chars().count() + if picked.is_empty() { 0 } else { 2 };
        if len + add > WHISPER_PROMPT_MAX_CHARS {
            continue; // skip, a shorter term further down may still fit
        }
        len += add;
        picked.push(term);
    }
    if picked.is_empty() {
        return None;
    }
    picked.reverse();
    Some(picked.join(", "))
}

/// Text appended to the LLM refinement system prompt: known corrections (heard -> intended) first,
/// then the remaining terms to keep verbatim. `None` when there is nothing to say.
pub(crate) fn refinement_suffix(settings: &Settings) -> Option<String> {
    let mut corrections = String::new();
    let mut targets: HashSet<String> = HashSet::new();
    if settings.postproc_custom_vocab_enabled {
        let mut used = 0usize;
        for (from, to) in ranked_pairs(
            &settings.edit_substitutions,
            &settings.postproc_custom_vocab,
        ) {
            if used >= REFINE_MAX_PAIRS {
                break;
            }
            if push_within(
                &mut corrections,
                &format!("{from} -> {to}"),
                REFINE_PAIRS_MAX_CHARS,
            ) {
                used += 1;
                targets.insert(to.to_lowercase());
            }
        }
    }

    // A term that is already a correction target is covered by its pair.
    let budget = REFINE_TOTAL_MAX_CHARS.saturating_sub(corrections.chars().count());
    let mut terms = String::new();
    for term in ranked_terms(&settings.vocab_terms, &settings.edit_substitutions) {
        if !targets.contains(&term.to_lowercase()) {
            push_within(&mut terms, &term, budget);
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if !corrections.is_empty() {
        parts.push(format!(
            "Known corrections (heard -> intended). Apply them when the heard form appears: {corrections}."
        ));
    }
    if !terms.is_empty() {
        parts.push(format!(
            "Known terms (proper nouns, acronyms, product names) — preserve these exactly as written, do not translate or normalize them: {terms}"
        ));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(
        from: &str,
        to: &str,
        count: u32,
        last_seen_ms: u64,
        promoted: bool,
    ) -> EditSubstitution {
        EditSubstitution {
            from: from.to_string(),
            to: to.to_string(),
            count,
            first_seen_ms: 1,
            last_seen_ms,
            promoted,
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn settings_with(terms: &[&str], subs: Vec<EditSubstitution>) -> Settings {
        let mut settings = Settings::default();
        settings.vocab_terms = strings(terms);
        settings.edit_substitutions = subs;
        settings.postproc_custom_vocab_enabled = true;
        settings
    }

    #[test]
    fn terms_rank_by_count_then_recency_then_plain_terms_newest_first() {
        let subs = vec![
            sub("Trispa", "Trispr", 5, 10, true),
            sub("Kodex", "Codex", 2, 90, true),
            sub("Gamma", "Gemma", 2, 50, true),
            sub("pending", "Ignored", 9, 99, false), // pending pairs do not rank anything
        ];
        let terms = strings(&[
            "Alpha", "Codex", "Gemma", "Trispr", "trispr", "Beta", "Ignored",
        ]);
        assert_eq!(
            ranked_terms(&terms, &subs),
            strings(&["Trispr", "Codex", "Gemma", "Ignored", "Beta", "Alpha"])
        );
    }

    #[test]
    fn whisper_prompt_skips_overlong_terms_instead_of_stopping_and_keeps_best_term_last() {
        let long = "x".repeat(WHISPER_PROMPT_MAX_CHARS - 5);
        let subs = vec![sub("Trispa", "Trispr", 4, 5, true)];
        // `long` is the newest plain term, so it ranks before Beta but cannot fit next to Trispr.
        let settings = settings_with(&["Beta", &long, "Trispr"], subs);
        let prompt = whisper_prompt(&settings).expect("prompt");
        // Trispr (learned, first) fits, `long` is skipped, Beta still fits: skip, not `break`.
        assert_eq!(prompt, "Beta, Trispr");
        assert!(prompt.chars().count() <= WHISPER_PROMPT_MAX_CHARS);
    }

    #[test]
    fn whisper_prompt_is_none_without_terms_and_strips_line_breaks() {
        assert_eq!(whisper_prompt(&Settings::default()), None);
        let settings = settings_with(&["Mem\r\nPalace", "  "], vec![]);
        assert_eq!(whisper_prompt(&settings).as_deref(), Some("Mem Palace"));
    }

    #[test]
    fn whisper_prompt_never_exceeds_budget_with_many_terms() {
        let terms: Vec<String> = (0..400).map(|i| format!("Term{i:03}Xy")).collect();
        let mut settings = Settings::default();
        settings.vocab_terms = terms;
        let prompt = whisper_prompt(&settings).expect("prompt");
        assert!(prompt.chars().count() <= WHISPER_PROMPT_MAX_CHARS);
        // newest terms (end of the list) are ranked first, so they are kept...
        assert!(prompt.contains("Term399Xy"));
        // ...and the oldest do not fit any more.
        assert!(!prompt.contains("Term000Xy"));
    }

    #[test]
    fn pairs_rank_learned_before_manual_rules_and_dedupe_casing_variants() {
        let subs = vec![
            sub("Kodex", "Codex", 2, 10, true),
            sub("Trispa", "Trispr", 3, 5, true),
            sub("open", "Pending", 8, 99, false),
        ];
        let mut rules = HashMap::new();
        rules.insert("Trispa".to_string(), "Trispr".to_string());
        rules.insert("trispa".to_string(), "Trispr".to_string()); // casing duplicate of a learned pair
        rules.insert("api".to_string(), "API".to_string());
        rules.insert("same".to_string(), "same".to_string()); // no-op
        let pairs = ranked_pairs(&subs, &rules);
        assert_eq!(
            pairs,
            vec![
                ("Trispa".to_string(), "Trispr".to_string()),
                ("Kodex".to_string(), "Codex".to_string()),
                ("api".to_string(), "API".to_string()),
            ]
        );
    }

    #[test]
    fn refinement_suffix_lists_pairs_first_then_terms_not_covered_by_a_pair() {
        let subs = vec![
            sub("Trispa", "Trispr", 3, 5, true),
            sub("Mem Palace", "MemPalace", 2, 7, true),
        ];
        let settings = settings_with(&["Trispr", "MemPalace", "Ollama"], subs);
        let suffix = refinement_suffix(&settings).expect("suffix");
        assert_eq!(
            suffix,
            "Known corrections (heard -> intended). Apply them when the heard form appears: \
             Trispa -> Trispr, Mem Palace -> MemPalace.\n\n\
             Known terms (proper nouns, acronyms, product names) — preserve these exactly as \
             written, do not translate or normalize them: Ollama"
        );
    }

    #[test]
    fn refinement_pairs_are_capped_in_count_and_characters() {
        let subs: Vec<EditSubstitution> = (0..30)
            .map(|i| {
                sub(
                    &format!("Heard{i:02}"),
                    &format!("Meant{i:02}"),
                    2,
                    i as u64,
                    true,
                )
            })
            .collect();
        let settings = settings_with(&[], subs);
        let suffix = refinement_suffix(&settings).expect("suffix");
        let list = suffix
            .split(": ")
            .nth(1)
            .expect("list")
            .trim_end_matches('.');
        assert!(list.matches(" -> ").count() <= REFINE_MAX_PAIRS);
        assert!(list.chars().count() <= REFINE_PAIRS_MAX_CHARS);
        // equal count -> most recently seen first
        assert!(list.starts_with("Heard29 -> Meant29"));
    }

    #[test]
    fn refinement_without_custom_vocab_enabled_sends_terms_only() {
        let subs = vec![sub("Trispa", "Trispr", 3, 5, true)];
        let mut settings = settings_with(&["Trispr"], subs);
        settings.postproc_custom_vocab_enabled = false;
        let suffix = refinement_suffix(&settings).expect("suffix");
        assert!(!suffix.contains("Known corrections"));
        assert!(suffix.ends_with("normalize them: Trispr"));
    }

    #[test]
    fn refinement_suffix_is_none_when_nothing_is_learned() {
        assert_eq!(refinement_suffix(&Settings::default()), None);
    }

    #[test]
    fn refinement_budget_is_shared_between_pairs_and_terms() {
        let subs = vec![sub("Trispa", "Trispr", 3, 5, true)];
        let terms: Vec<String> = (0..200).map(|i| format!("Term{i:03}Xy")).collect();
        let refs: Vec<&str> = terms.iter().map(String::as_str).collect();
        let settings = settings_with(&refs, subs);
        let suffix = refinement_suffix(&settings).expect("suffix");
        let corrections = "Trispa -> Trispr";
        let term_list = suffix.rsplit(": ").next().expect("terms");
        assert!(corrections.chars().count() + term_list.chars().count() <= REFINE_TOTAL_MAX_CHARS);
    }
}
