/**
 * Edit-Delta Vocabulary Learning.
 *
 * Learns from what the user explicitly corrects: the diff between `pasted`
 * (refinement output pasted into the input field) and `submitted` (what the
 * user actually sends with Enter). Substitutions that repeat 3× are promoted
 * to `postproc_custom_vocab` (find-replace before the next Whisper/LLM pass)
 * and to `vocab_terms` (Whisper hint + LLM refinement context).
 *
 * No heuristics, no STOPWORDS, no Levenshtein clustering. Every signal is an
 * explicit user correction, so false positives are structurally impossible.
 */

import type { EditSubstitution } from "./types";
import { settings } from "./state";
import { persistSettings } from "./settings";

const PROMOTION_THRESHOLD = 3;

// ---------------------------------------------------------------------------
// Word-level diff
// ---------------------------------------------------------------------------

interface SubstitutionPair {
  from: string;
  to: string;
}

function tokenize(text: string): string[] {
  return text.split(/\s+/).filter((t) => t.length > 0);
}

const EDGE_PUNCT_RE = /^[\p{P}\p{S}]+|[\p{P}\p{S}]+$/gu;
const NUMERIC_RE = /^[\d.,]+$/;

/**
 * Length of LCS between two token sequences. Plain DP, only the length is
 * needed for window scoring — backtracking is reserved for the main wordDiff.
 */
function lcsLength(a: string[], b: string[]): number {
  const m = a.length;
  const n = b.length;
  if (m === 0 || n === 0) return 0;
  let prev = new Array<number>(n + 1).fill(0);
  let curr = new Array<number>(n + 1).fill(0);
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      curr[j] = a[i - 1] === b[j - 1] ? prev[j - 1] + 1 : Math.max(prev[j], curr[j - 1]);
    }
    [prev, curr] = [curr, prev];
    curr.fill(0);
  }
  return prev[n];
}

/**
 * When submitted is much larger than pasted (typical for UIAutomation reads
 * out of Monaco / VS-Code-style editors that return the entire document),
 * shrink submitted to the best-matching window so the 50% rewrite heuristic
 * does not discard a real edit signal.
 *
 * Strategy:
 * 1. If submitted is at most 2x pasted in token count, return as-is.
 * 2. If pasted appears as an exact contiguous subsequence in submitted, the
 *    user did not edit — return submitted as-is so wordDiff sees identical
 *    content within the matching span.
 * 3. Otherwise slide a window of size ~1.5x pasted across submitted and pick
 *    the window with the highest LCS against pasted.
 */
function findEditWindow(pastedTokens: string[], submittedTokens: string[]): string[] {
  const m = pastedTokens.length;
  const n = submittedTokens.length;
  if (m === 0 || n === 0) return submittedTokens;
  if (n <= m * 2) return submittedTokens;

  for (let i = 0; i + m <= n; i++) {
    let match = true;
    for (let j = 0; j < m; j++) {
      if (submittedTokens[i + j] !== pastedTokens[j]) {
        match = false;
        break;
      }
    }
    if (match) return submittedTokens;
  }

  const windowSize = Math.min(n, Math.max(m + 4, Math.ceil(m * 1.5)));
  const step = Math.max(1, Math.floor(windowSize / 4));
  let bestStart = 0;
  let bestScore = -1;
  for (let start = 0; start + windowSize <= n; start += step) {
    const window = submittedTokens.slice(start, start + windowSize);
    const score = lcsLength(pastedTokens, window);
    if (score > bestScore) {
      bestScore = score;
      bestStart = start;
    }
  }

  return submittedTokens.slice(bestStart, bestStart + windowSize);
}

function stripEdgePunct(t: string): string {
  return t.replace(EDGE_PUNCT_RE, "");
}

function isUselessToken(t: string): boolean {
  if (t.length < 3) return true;
  if (NUMERIC_RE.test(t)) return true;
  if (/^[\p{P}\p{S}]+$/u.test(t)) return true;
  return false;
}

/**
 * Compute 1-to-1 substitution pairs between pasted and submitted.
 *
 * 1. Tokenize both strings.
 * 2. Bail if identical, or > 50% of tokens are in the diff (complete rewrite).
 * 3. LCS DP to find the unchanged spine.
 * 4. Walk the edit script; consecutive equal-length delete+insert runs are
 *    1:1 substitution candidates.
 * 5. Filter out tokens that are too short, purely numeric, or punctuation.
 */
export function wordDiff(pasted: string, submitted: string): SubstitutionPair[] {
  if (pasted === submitted) return [];
  const pa = tokenize(pasted);
  const suRaw = tokenize(submitted);
  if (pa.length === 0 || suRaw.length === 0) return [];

  const su = findEditWindow(pa, suRaw);

  const m = pa.length;
  const n = su.length;

  // Full DP for LCS + backtracking.
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array<number>(n + 1).fill(0));
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      dp[i][j] =
        pa[i - 1] === su[j - 1]
          ? dp[i - 1][j - 1] + 1
          : Math.max(dp[i - 1][j], dp[i][j - 1]);
    }
  }

  const lcs = dp[m][n];
  const changed = m + n - 2 * lcs;
  if (changed > (m + n) * 0.5) return [];

  // Backtrack to produce the edit script.
  type Op = { type: "equal" | "delete" | "insert"; token: string };
  const ops: Op[] = [];
  let i = m;
  let j = n;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && pa[i - 1] === su[j - 1]) {
      ops.push({ type: "equal", token: pa[i - 1] });
      i--;
      j--;
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      ops.push({ type: "insert", token: su[j - 1] });
      j--;
    } else {
      ops.push({ type: "delete", token: pa[i - 1] });
      i--;
    }
  }
  ops.reverse();

  // Pair consecutive delete-runs with immediately-following insert-runs.
  const pairs: SubstitutionPair[] = [];
  let k = 0;
  while (k < ops.length) {
    if (ops[k].type !== "delete") {
      k++;
      continue;
    }
    const dels: string[] = [];
    while (k < ops.length && ops[k].type === "delete") {
      dels.push(ops[k].token);
      k++;
    }
    const ins: string[] = [];
    while (k < ops.length && ops[k].type === "insert") {
      ins.push(ops[k].token);
      k++;
    }
    if (dels.length !== ins.length) continue; // not 1:1
    for (let p = 0; p < dels.length; p++) {
      const from = stripEdgePunct(dels[p]);
      const to = stripEdgePunct(ins[p]);
      if (from === to) continue;
      if (isUselessToken(from) || isUselessToken(to)) continue;
      pairs.push({ from, to });
    }
  }
  return pairs;
}

// ---------------------------------------------------------------------------
// One-time migration from the old heuristic system
// ---------------------------------------------------------------------------

/**
 * On first run with the new Edit-Delta system: clear the legacy heuristic
 * data (postproc_custom_vocab contained wrong auto-generated entries) and set
 * the migration flag so this never runs again.
 */
export function runMigrationIfNeeded(): void {
  if (!settings) return;
  if (settings.edit_delta_migrated) return;
  settings.postproc_custom_vocab = {};
  // vocab_term_candidates may exist in old settings.json — drop it.
  (settings as unknown as Record<string, unknown>)["vocab_term_candidates"] = undefined;
  settings.edit_substitutions = [];
  settings.edit_delta_migrated = true;
}

// ---------------------------------------------------------------------------
// Ingestion entry point
// ---------------------------------------------------------------------------

/**
 * Called from the `enter_capture:edit_detected` event with both sides of the
 * user's edit. Accumulates substitution pairs and promotes any that have
 * reached the threshold.
 */
export function ingestEditDelta(pasted: string, submitted: string): void {
  if (!settings) return;
  runMigrationIfNeeded();

  const pairs = wordDiff(pasted, submitted);
  if (pairs.length === 0) return;

  const now = Date.now();
  const list: EditSubstitution[] = Array.isArray(settings.edit_substitutions)
    ? [...settings.edit_substitutions]
    : [];

  const promoted: Array<[string, string]> = [];

  for (const { from, to } of pairs) {
    const existing = list.find((s) => s.from === from && s.to === to);
    if (existing) {
      existing.count += 1;
      existing.last_seen_ms = now;
      if (existing.count >= PROMOTION_THRESHOLD) {
        promoted.push([from, to]);
      }
    } else {
      list.push({ from, to, count: 1, first_seen_ms: now, last_seen_ms: now });
    }
  }

  settings.edit_substitutions = list.filter(
    (s) => !promoted.some(([f, t]) => s.from === f && s.to === t),
  );

  if (promoted.length > 0) {
    applyPromotions(promoted);
  }

  void persistSettings();
}

function applyPromotions(pairs: Array<[string, string]>): void {
  if (!settings) return;
  const vocab = { ...(settings.postproc_custom_vocab ?? {}) };
  const terms = Array.isArray(settings.vocab_terms) ? [...settings.vocab_terms] : [];
  const termsLower = new Set(terms.map((t) => t.toLowerCase()));

  for (const [from, to] of pairs) {
    vocab[from] = to;
    const lowered = from.toLowerCase();
    if (lowered !== from) vocab[lowered] = to;
    if (!termsLower.has(to.toLowerCase())) {
      terms.push(to);
      termsLower.add(to.toLowerCase());
    }
  }

  settings.postproc_custom_vocab = vocab;
  settings.postproc_custom_vocab_enabled = true;
  settings.vocab_terms = terms;

  void import("./event-listeners")
    .then(({ renderLearnedVocabChips }) => {
      renderLearnedVocabChips();
    })
    .catch(() => {});
}

// ---------------------------------------------------------------------------
// UI helpers
// ---------------------------------------------------------------------------

/** Remove a learned term from vocab_terms. Called by the UI × button. */
export async function dismissLearnedTerm(term: string): Promise<void> {
  if (!settings) return;
  const key = term.toLowerCase();
  const next = (settings.vocab_terms ?? []).filter((t) => t.toLowerCase() !== key);
  if (next.length === (settings.vocab_terms ?? []).length) return;
  settings.vocab_terms = next;
  await persistSettings();
}

/** Discard a pending substitution before it reaches the promotion threshold. */
export async function dismissPendingSubstitution(from: string, to: string): Promise<void> {
  if (!settings) return;
  const before = (settings.edit_substitutions ?? []).length;
  settings.edit_substitutions = (settings.edit_substitutions ?? []).filter(
    (s) => !(s.from === from && s.to === to),
  );
  if ((settings.edit_substitutions ?? []).length === before) return;
  await persistSettings();
}

export interface AutoLearnSnapshot {
  learned: string[];
  pendingCount: number;
}

export function getAutoLearnSnapshot(): AutoLearnSnapshot {
  const learned = Array.isArray(settings?.vocab_terms) ? [...settings!.vocab_terms] : [];
  const pending = Array.isArray(settings?.edit_substitutions)
    ? settings!.edit_substitutions.length
    : 0;
  return { learned, pendingCount: pending };
}
