import type { TranscriptSessionCandidate } from "./types";

/**
 * Score difference threshold below which two candidates are considered ambiguous.
 * If top-2 candidates differ by less than this value, user should select manually.
 */
export const DISAMBIGUATION_SCORE_THRESHOLD = 0.1;

/**
 * Allowed target language values for the workflow agent execution plan.
 * Must match AgentTargetLanguage union in types.ts and backend ALLOWED_LANGUAGES.
 */
export const ALLOWED_TARGET_LANGUAGES: readonly string[] = [
  "source",
  "en",
  "de",
  "fr",
  "es",
  "it",
  "pt",
  "nl",
  "pl",
  "ru",
  "ja",
  "ko",
  "zh",
  "ar",
  "tr",
  "hi",
];

/**
 * Returns true when the top-2 candidates are too similar to auto-select safely.
 * Requires the user to manually review and select.
 */
export function isAmbiguousSelection(
  candidates: TranscriptSessionCandidate[],
  threshold = DISAMBIGUATION_SCORE_THRESHOLD
): boolean {
  if (candidates.length < 2) return false;
  return candidates[0].score - candidates[1].score < threshold;
}

/**
 * Returns true when the given language string is a valid target language.
 */
export function isValidTargetLanguage(lang: string | null | undefined): boolean {
  if (!lang) return false;
  return ALLOWED_TARGET_LANGUAGES.includes(lang.trim().toLowerCase());
}
