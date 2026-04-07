/**
 * Vocabulary candidate tracking for learned word corrections.
 *
 * Observes AI refinement diffs, detects repeated 1-to-1 word substitutions,
 * and surfaces them as vocabulary suggestions once a threshold is reached.
 *
 * Candidates are stored in localStorage under VOCAB_CANDIDATES_STORAGE_KEY.
 * They are never auto-added to postproc_custom_vocab — the user must confirm
 * each suggestion via the review dialog (unless vocab_auto_add is enabled).
 */

import type { VocabCandidate } from "./types";
import { buildRefinementWordDiff } from "./refinement-inspector";
import { settings } from "./state";

const VOCAB_CANDIDATES_STORAGE_KEY = "trispr_vocab_candidates_v1";
const MAX_CANDIDATES = 200;

/** Fallback threshold when settings are not yet loaded. */
export const VOCAB_SUGGESTION_THRESHOLD = 3;

/** Read the active suggestion threshold from settings, falling back to the constant. */
function getThreshold(): number {
  const t = settings?.vocab_suggestion_threshold;
  return typeof t === "number" && t >= 1 && t <= 10 ? t : VOCAB_SUGGESTION_THRESHOLD;
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

function loadCandidates(): VocabCandidate[] {
  try {
    const raw = window.localStorage.getItem(VOCAB_CANDIDATES_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (c): c is VocabCandidate =>
        c && typeof c.from === "string" && typeof c.to === "string" &&
        typeof c.count === "number" && typeof c.last_seen_ms === "number"
    );
  } catch {
    return [];
  }
}

function saveCandidates(candidates: VocabCandidate[]): void {
  try {
    // Drop oldest entries when cap is exceeded
    const capped =
      candidates.length > MAX_CANDIDATES
        ? candidates
            .slice()
            .sort((a, b) => b.last_seen_ms - a.last_seen_ms)
            .slice(0, MAX_CANDIDATES)
        : candidates;
    window.localStorage.setItem(VOCAB_CANDIDATES_STORAGE_KEY, JSON.stringify(capped));
  } catch (e) {
    console.warn("Failed to persist vocab candidates:", e);
  }
}

// ---------------------------------------------------------------------------
// Candidate extraction from diff
// ---------------------------------------------------------------------------

/**
 * Scans a word diff and extracts 1-to-1 substitution pairs:
 * a single removed token immediately followed by a single added token.
 * Runs of multiple removed/added tokens are skipped (they are rephrasing,
 * not simple corrections).
 */
export function extractSubstitutionsFromDiff(
  raw: string,
  refined: string
): Array<{ from: string; to: string }> {
  const diff = buildRefinementWordDiff(raw, refined);
  const pairs: Array<{ from: string; to: string }> = [];

  let i = 0;
  while (i < diff.length) {
    const token = diff[i];

    if (token.kind === "removed") {
      // Check for exactly one removed followed by exactly one added
      const next = diff[i + 1];
      const afterNext = diff[i + 2];
      if (
        next?.kind === "added" &&
        (afterNext === undefined || afterNext.kind !== "removed")
      ) {
        // Normalize: lowercase comparison key, but preserve original casing for display
        pairs.push({ from: token.token, to: next.token });
        i += 2;
        continue;
      }
      // Multi-token removal — skip until we leave the removed/added run
      while (i < diff.length && diff[i].kind !== "same") {
        i++;
      }
      continue;
    }

    if (token.kind === "added") {
      // Orphan added token (no preceding removed) — skip
      i++;
      continue;
    }

    i++;
  }

  return pairs;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Record substitutions observed in a refinement diff.
 * Increments counts for existing candidates, adds new ones.
 * Returns candidates that just crossed the suggestion threshold.
 *
 * Returns empty array if vocab learning is disabled in settings.
 */
export function recordRefinementDiff(
  raw: string,
  refined: string
): VocabCandidate[] {
  // Respect the learning toggle
  if (settings?.vocab_learning_enabled === false) return [];

  const pairs = extractSubstitutionsFromDiff(raw, refined);
  if (pairs.length === 0) return [];

  const threshold = getThreshold();
  const candidates = loadCandidates();
  const newlyThresholded: VocabCandidate[] = [];
  const now = Date.now();

  for (const pair of pairs) {
    const fromKey = pair.from.toLowerCase();
    const toKey = pair.to.toLowerCase();

    // Skip if the pair is just a casing difference (e.g. "API" → "api")
    if (fromKey === toKey) continue;

    const existing = candidates.find(
      (c) => c.from.toLowerCase() === fromKey && c.to.toLowerCase() === toKey
    );

    if (existing) {
      const prevCount = existing.count;
      existing.count += 1;
      existing.last_seen_ms = now;
      if (prevCount < threshold && existing.count >= threshold) {
        newlyThresholded.push(existing);
      }
    } else {
      const candidate: VocabCandidate = {
        from: pair.from,
        to: pair.to,
        count: 1,
        last_seen_ms: now,
      };
      candidates.push(candidate);
      if (threshold <= 1) {
        newlyThresholded.push(candidate);
      }
    }
  }

  saveCandidates(candidates);
  return newlyThresholded;
}

/** Returns all candidates that have reached the suggestion threshold. */
export function getSuggestedCandidates(): VocabCandidate[] {
  return loadCandidates().filter((c) => c.count >= getThreshold());
}

/** Returns all tracked candidates regardless of threshold (for status display). */
export function getAllCandidates(): VocabCandidate[] {
  return loadCandidates();
}

/** Returns the total number of pending suggestions. */
export function getPendingSuggestionCount(): number {
  return getSuggestedCandidates().length;
}

/**
 * Remove a candidate (after user confirms or dismisses it).
 * Pass the from/to pair to identify the entry.
 */
export function removeCandidateByPair(from: string, to: string): void {
  const candidates = loadCandidates().filter(
    (c) => !(c.from.toLowerCase() === from.toLowerCase() && c.to.toLowerCase() === to.toLowerCase())
  );
  saveCandidates(candidates);
}

/** Remove all candidates (e.g. after user dismisses all suggestions). */
export function clearAllCandidates(): void {
  try {
    window.localStorage.removeItem(VOCAB_CANDIDATES_STORAGE_KEY);
  } catch {
    // ignore
  }
}
