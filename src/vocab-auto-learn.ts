/**
 * Edit-Delta Vocabulary Learning.
 *
 * Learns from what the user explicitly corrects: the diff between `pasted`
 * (refinement output pasted into the input field) and `submitted` (what the
 * user actually sends with Enter). A correction seen in 2 separate edits is
 * promoted: `to` becomes a vocabulary term (Whisper prompt + LLM refinement
 * hint), the pair itself is kept as a "heard -> intended" hint for the LLM, and
 * only a correction that looks like a mishearing (see `classifyCorrection`)
 * becomes a deterministic find-replace rule in `postproc_custom_vocab`.
 *
 * Rewordings (morgen -> heute) and grammar edits (den -> dem) are rejected
 * before they are even counted.
 */

import type { EditSubstitution } from "./types";
import { settings } from "./state";
import { persistSettings } from "./settings-persist";
import { showToast } from "./toast";

// 2, not 3: real-world data (6 weeks of usage) showed that requiring the
// exact same correction three times means practically nothing ever promotes —
// 40+ pending pairs, none past count 2.
export const PROMOTION_THRESHOLD = 2;

/**
 * Pending pairs not seen again within this window expire. A mishearing that
 * recurs shows up well inside 3 months of daily use; the 6-week data set above
 * produced mostly one-off pairs that would otherwise sit in the list forever.
 * Promoted pairs never expire by age: the only timestamp is "last corrected",
 * and a rule that works stops being corrected (the × in the Learned list
 * removes one).
 */
export const PENDING_TTL_MS = 90 * 24 * 60 * 60 * 1000;

// Hard caps so a long-lived profile cannot grow without bound.
const MAX_PENDING = 50;
const MAX_LEARNED = 100;
const MAX_TERMS = 200;

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
 * Hyphenated compounds with the same number of parts are compared part by part, so
 * WhatsApp-Tickets -> WhatsUp-Tickets teaches WhatsApp -> WhatsUp and counts together with
 * every other occurrence of the bare word. Anything else stays one pair.
 */
function splitCompound(from: string, to: string): SubstitutionPair[] {
  const fromParts = from.split("-");
  const toParts = to.split("-");
  if (fromParts.length < 2 || fromParts.length !== toParts.length) return [{ from, to }];
  const pairs: SubstitutionPair[] = [];
  fromParts.forEach((part, idx) => {
    if (part !== toParts[idx]) pairs.push({ from: part, to: toParts[idx] });
  });
  return pairs;
}

/**
 * Compute substitution pairs between pasted and submitted.
 *
 * 1. Tokenize both strings.
 * 2. Bail if identical, or > 50% of tokens are in the diff (complete rewrite).
 * 3. LCS DP to find the unchanged spine.
 * 4. Walk the edit script; a delete-run followed by an insert-run is a
 *    substitution: equal-length runs pair up token by token, a 2:1 / 1:2 run
 *    is one phrase pair ("Mem Palace" <-> "MemPalace").
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
    let candidates: Array<[string, string]>;
    if (dels.length === ins.length) {
      candidates = dels.map((d, p): [string, string] => [d, ins[p]]);
    } else if (dels.length + ins.length === 3 && dels.length > 0 && ins.length > 0) {
      candidates = [[dels.join(" "), ins.join(" ")]]; // 2:1 or 1:2 phrase
    } else {
      continue;
    }
    for (const [rawFrom, rawTo] of candidates) {
      const from = stripEdgePunct(rawFrom);
      const to = stripEdgePunct(rawTo);
      if (from === to) continue;
      for (const pair of splitCompound(from, to)) {
        if (isUselessToken(pair.from) || isUselessToken(pair.to)) continue;
        pairs.push(pair);
      }
    }
  }
  return pairs;
}

// ---------------------------------------------------------------------------
// Is this edit a mishearing correction? (similarity gate)
// ---------------------------------------------------------------------------

/**
 * - `rule`:    looks like a mishearing/spelling fix -> deterministic find-replace is safe
 * - `hint`:    plausible but loose -> only Whisper/LLM hints, no find-replace
 * - `casing`:  same letters, different case -> hints only (a rule would recase every common word)
 * - `reject`:  rewording or grammar edit -> not counted at all
 */
export type CorrectionKind = "rule" | "hint" | "casing" | "reject";

/**
 * Normalized Levenshtein distance (edits / longer length) on lowercased,
 * accent-folded, whitespace-free text. Measured: Trispa->Trispr 0.17,
 * Kodex->Codex 0.20, Gamma->Gemma 0.20, Wispr->Whisper 0.29 are rules (<= 0.34);
 * Kloud->Claude 0.50 is a hint (<= 0.50); treffen->sehen 0.57, gestern->heute
 * 0.57, Jammer->Gemma 0.67, morgen->heute 0.83 are rewordings or too far to
 * trust and are rejected. A find-replace rule also needs >= 5 letters (one
 * edit in a 3-4 letter word is too often a different word: Zug->Zeug).
 */
export const RULE_MAX_DISTANCE = 0.34;
export const HINT_MAX_DISTANCE = 0.5;
const RULE_MIN_LENGTH = 5;

/** German/English function words: edits between them are grammar, not terminology. */
const FUNCTION_WORDS = new Set(
  (
    "der die das den dem des ein eine einen einem einer eines und oder aber denn doch weil dass " +
    "wenn als wie was wer wen wem wann wo warum ich mich mir mein meine meinen meinem meiner " +
    "dich dir dein deine ihn ihm sein seine seinen seinem seiner sie ihr ihre ihren ihrem ihrer " +
    "wir uns unser unsere euch mit von vor für auf aus bei nach zum zur über unter durch gegen " +
    "ohne bis seit ist sind war waren bin bist hat haben hatte hatten wird werden wurde wurden " +
    "kann können konnte muss müssen soll sollen will wollen darf nicht kein keine keinen auch " +
    "noch schon nur sehr mal dann hier dort jetzt the and but for with that this these those " +
    "from into onto over not are was were has have had will would can could should may might " +
    "you your his her its our their them they she him who what when where why how"
  ).split(" "),
);

function foldForCompare(text: string): string {
  return text
    .toLowerCase()
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .replace(/\s+/g, "");
}

function levenshtein(a: string, b: string): number {
  const x = Array.from(a);
  const y = Array.from(b);
  let prev = Array.from({ length: y.length + 1 }, (_, idx) => idx);
  for (let i = 1; i <= x.length; i++) {
    const curr = [i];
    for (let j = 1; j <= y.length; j++) {
      curr[j] = Math.min(
        prev[j] + 1,
        curr[j - 1] + 1,
        prev[j - 1] + (x[i - 1] === y[j - 1] ? 0 : 1),
      );
    }
    prev = curr;
  }
  return prev[y.length];
}

function isAllFunctionWords(text: string): boolean {
  return text
    .toLowerCase()
    .split(/\s+/)
    .every((word) => FUNCTION_WORDS.has(word));
}

/** Typographic quotes/dashes folded to ASCII: editors and chat apps swap them silently. */
function normalizeTypography(text: string): string {
  return text
    .replace(/[‘’‛ʼ`´]/g, "'")
    .replace(/[“”„]/g, '"')
    .replace(/[‐‑‒–—]/g, "-");
}

export function classifyCorrection(from: string, to: string): CorrectionKind {
  if (isAllFunctionWords(from) || isAllFunctionWords(to)) return "reject";
  // Don't -> Don’t is a typography swap by the app, not a mishearing: never learn it.
  if (from !== to && normalizeTypography(from) === normalizeTypography(to)) return "reject";

  const a = foldForCompare(from);
  const b = foldForCompare(to);
  if (a === b) {
    const lowerFrom = from.toLowerCase();
    const lowerTo = to.toLowerCase();
    if (lowerFrom === lowerTo) return "casing";
    // Same letters, only the spacing differs (Mem Palace <-> MemPalace).
    if (lowerFrom.replace(/\s+/g, "") === lowerTo.replace(/\s+/g, "")) return "rule";
    return "hint"; // accents only (fur -> für): could be a different word
  }

  const length = Math.max(Array.from(a).length, Array.from(b).length);
  const distance = levenshtein(a, b) / length;
  if (distance <= RULE_MAX_DISTANCE && length >= RULE_MIN_LENGTH) return "rule";
  if (distance <= HINT_MAX_DISTANCE) return "hint";
  return "reject";
}

/**
 * Lowercase twin of a rule only for "special" spellings (CamelCase, digits):
 * those are not ordinary words. A plain capitalized `from` (Code, Gamma) may
 * be a common word whose lowercase form must keep working.
 */
function needsLowercaseRule(from: string): boolean {
  return from !== from.toLowerCase() && /[\p{Lu}\p{N}]/u.test(from.slice(1));
}

// ---------------------------------------------------------------------------
// One-time migration from the old heuristic system
// ---------------------------------------------------------------------------

/**
 * First run with the Edit-Delta system: only set the marker. The legacy
 * heuristic left nothing that tells its auto-generated rules apart from rules
 * the user wrote by hand (the flag defaults to false for every new profile
 * too), so nothing is deleted.
 */
export function runMigrationIfNeeded(): void {
  if (!settings) return;
  if (settings.edit_delta_migrated) return;
  settings.edit_delta_migrated = true;
}

// ---------------------------------------------------------------------------
// Ingestion entry point
// ---------------------------------------------------------------------------

function sameCorrection(s: EditSubstitution, from: string, to: string): boolean {
  return (
    s.from.toLowerCase() === from.toLowerCase() && s.to.toLowerCase() === to.toLowerCase()
  );
}

/** A single edit counts each distinct correction once, however often it occurs in the text. */
function uniquePairs(pairs: SubstitutionPair[]): SubstitutionPair[] {
  const seen = new Set<string>();
  return pairs.filter(({ from, to }) => {
    const key = `${from.toLowerCase()}\u0000${to.toLowerCase()}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** Drop pending pairs that were not seen again within PENDING_TTL_MS. Returns whether any went. */
function expirePending(now: number): boolean {
  if (!settings || !Array.isArray(settings.edit_substitutions)) return false;
  const kept = settings.edit_substitutions.filter(
    (s) => s.promoted || now - s.last_seen_ms <= PENDING_TTL_MS,
  );
  if (kept.length === settings.edit_substitutions.length) return false;
  settings.edit_substitutions = kept;
  return true;
}

/**
 * Called from the `enter_capture:edit_detected` event with both sides of the
 * user's edit. Accumulates substitution pairs (one count per edit and pair)
 * and promotes any that have reached the threshold.
 */
export function ingestEditDelta(pasted: string, submitted: string): void {
  if (!settings) return;
  runMigrationIfNeeded();

  const now = Date.now();
  const expired = expirePending(now);

  const pairs = uniquePairs(wordDiff(pasted, submitted)).filter(
    ({ from, to }) => classifyCorrection(from, to) !== "reject",
  );
  if (pairs.length === 0) {
    if (expired) void persistSettings();
    return;
  }

  const list: EditSubstitution[] = Array.isArray(settings.edit_substitutions)
    ? [...settings.edit_substitutions]
    : [];

  const toApply: Array<[string, string]> = [];
  const newlyLearned: Array<[string, string]> = [];

  for (const { from, to } of pairs) {
    // Case-insensitive match so casing variants of the same correction
    // ("Gamma→Gemma" vs "gamma→gemma") share one counter instead of each
    // stalling at count 1 forever.
    const existing = list.find((s) => sameCorrection(s, from, to));
    if (existing) {
      existing.count += 1;
      existing.last_seen_ms = now;
      if (existing.promoted) {
        // Seen again although learned: make sure this spelling has its rule too.
        toApply.push([from, to]);
      } else if (existing.count >= PROMOTION_THRESHOLD) {
        // Keep the freshest casing (the pair just observed).
        existing.from = from;
        existing.to = to;
        existing.promoted = true;
        toApply.push([from, to]);
        newlyLearned.push([from, to]);
      }
    } else {
      list.push({ from, to, count: 1, first_seen_ms: now, last_seen_ms: now, promoted: false });
    }
  }

  settings.edit_substitutions = list;
  if (toApply.length > 0) {
    applyPromotions(toApply, newlyLearned.length > 0);
  }
  enforceCaps();
  void persistSettings();

  if (toApply.length > 0) {
    refreshVocabUi();
  }
  if (newlyLearned.length > 0) {
    announceLearned(newlyLearned);
  }
}

function applyPromotions(pairs: Array<[string, string]>, enableRules: boolean): void {
  if (!settings) return;
  const vocab = { ...(settings.postproc_custom_vocab ?? {}) };
  const terms = Array.isArray(settings.vocab_terms) ? [...settings.vocab_terms] : [];
  const termsLower = new Set(terms.map((t) => t.toLowerCase()));
  const hasRule = (key: string) => Object.prototype.hasOwnProperty.call(vocab, key);

  for (const [from, to] of pairs) {
    if (classifyCorrection(from, to) === "rule") {
      // A rule that already maps this word elsewhere belongs to the user: keep it.
      if (!hasRule(from)) vocab[from] = to;
      const lowered = from.toLowerCase();
      if (needsLowercaseRule(from) && !hasRule(lowered)) vocab[lowered] = to;
    }
    if (!termsLower.has(to.toLowerCase())) {
      terms.push(to);
      termsLower.add(to.toLowerCase());
    }
  }

  settings.postproc_custom_vocab = vocab;
  // The LLM correction hints and the find-replace rules are both gated by this switch. Only a
  // newly learned pair switches it on; a pair seen again must not undo the user turning it off.
  if (enableRules) settings.postproc_custom_vocab_enabled = true;
  settings.vocab_terms = terms;
}

function refreshVocabUi(): void {
  void import("./settings/vocabulary.settings").then(({ renderLearnedVocabChips }) => {
    renderLearnedVocabChips();
  });
}

/** Toast with Undo so a wrong promotion never happens silently. */
function announceLearned(pairs: Array<[string, string]>): void {
  const shown = pairs.slice(0, 3).map(([from, to]) => `${from} → ${to}`);
  const more = pairs.length > shown.length ? ` (+${pairs.length - shown.length} more)` : "";
  showToast({
    type: "success",
    title: "Vocabulary learned",
    message: `${shown.join(", ")}${more}`,
    duration: 8000,
    actionLabel: "Undo",
    onAction: async () => {
      for (const [from, to] of pairs) {
        await dismissLearnedPair(from, to);
      }
      refreshVocabUi();
    },
  });
}

// ---------------------------------------------------------------------------
// Caps
// ---------------------------------------------------------------------------

function enforceCaps(): void {
  if (!settings) return;
  const subs = Array.isArray(settings.edit_substitutions) ? settings.edit_substitutions : [];
  const newestFirst = (a: EditSubstitution, b: EditSubstitution) => b.last_seen_ms - a.last_seen_ms;
  const pending = subs.filter((s) => !s.promoted).sort(newestFirst);
  const learned = subs.filter((s) => s.promoted).sort(newestFirst);

  const keptLearned = learned.slice(0, MAX_LEARNED);
  settings.edit_substitutions = [...pending.slice(0, MAX_PENDING), ...keptLearned];
  for (const dropped of learned.slice(MAX_LEARNED)) {
    removeRulesFor(dropped.from, dropped.to);
    if (!keptLearned.some((s) => s.to.toLowerCase() === dropped.to.toLowerCase())) {
      removeTerm(dropped.to);
    }
  }

  if (Array.isArray(settings.vocab_terms) && settings.vocab_terms.length > MAX_TERMS) {
    settings.vocab_terms = settings.vocab_terms.slice(-MAX_TERMS); // oldest first -> drop the oldest
  }
}

// ---------------------------------------------------------------------------
// UI helpers
// ---------------------------------------------------------------------------

function removeTerm(term: string): void {
  if (!settings) return;
  const key = term.toLowerCase();
  settings.vocab_terms = (settings.vocab_terms ?? []).filter((t) => t.toLowerCase() !== key);
}

/** Remove find-replace rules `from -> to` (any casing of `from`). */
function removeRulesFor(from: string, to: string): void {
  if (!settings?.postproc_custom_vocab) return;
  const next: Record<string, string> = {};
  for (const [key, value] of Object.entries(settings.postproc_custom_vocab)) {
    const isThisRule =
      key.toLowerCase() === from.toLowerCase() && value.toLowerCase() === to.toLowerCase();
    if (!isThisRule) next[key] = value;
  }
  settings.postproc_custom_vocab = next;
}

/**
 * Remove a learned term from vocab_terms together with every correction that
 * points at it: promoted pairs and find-replace rules with this `to`.
 * Called by the UI × button on a term chip.
 */
export async function dismissLearnedTerm(term: string): Promise<void> {
  if (!settings) return;
  const key = term.toLowerCase();
  const termsBefore = (settings.vocab_terms ?? []).length;
  const subsBefore = (settings.edit_substitutions ?? []).length;
  const rulesBefore = Object.keys(settings.postproc_custom_vocab ?? {}).length;

  removeTerm(term);
  settings.edit_substitutions = (settings.edit_substitutions ?? []).filter(
    (s) => !(s.promoted && s.to.toLowerCase() === key),
  );
  settings.postproc_custom_vocab = Object.fromEntries(
    Object.entries(settings.postproc_custom_vocab ?? {}).filter(
      ([, value]) => value.toLowerCase() !== key,
    ),
  );

  const unchanged =
    (settings.vocab_terms ?? []).length === termsBefore &&
    (settings.edit_substitutions ?? []).length === subsBefore &&
    Object.keys(settings.postproc_custom_vocab).length === rulesBefore;
  if (unchanged) return;
  await persistSettings();
}

/**
 * Remove one learned pair: the pair itself, its find-replace rules, and the
 * term `to` unless another learned pair still points at it. Called by the UI ×
 * button on a learned pair chip and by the Undo action of the "learned" toast.
 */
export async function dismissLearnedPair(from: string, to: string): Promise<void> {
  if (!settings) return;
  const before = (settings.edit_substitutions ?? []).length;
  settings.edit_substitutions = (settings.edit_substitutions ?? []).filter(
    (s) => !(s.promoted && sameCorrection(s, from, to)),
  );
  if ((settings.edit_substitutions ?? []).length === before) return;

  removeRulesFor(from, to);
  const stillUsed = settings.edit_substitutions.some(
    (s) => s.promoted && s.to.toLowerCase() === to.toLowerCase(),
  );
  if (!stillUsed) removeTerm(to);
  await persistSettings();
}

/** Discard a pending substitution before it reaches the promotion threshold. */
export async function dismissPendingSubstitution(from: string, to: string): Promise<void> {
  if (!settings) return;
  const before = (settings.edit_substitutions ?? []).length;
  settings.edit_substitutions = (settings.edit_substitutions ?? []).filter(
    (s) => s.promoted || !(s.from === from && s.to === to),
  );
  if ((settings.edit_substitutions ?? []).length === before) return;
  await persistSettings();
}

/** Startup housekeeping: expire stale pending pairs and persist if any went. */
export async function pruneStalePending(): Promise<void> {
  if (!settings) return;
  if (!expirePending(Date.now())) return;
  await persistSettings();
  refreshVocabUi();
}

export interface AutoLearnSnapshot {
  learned: string[];
  pendingCount: number;
}

export function getAutoLearnSnapshot(): AutoLearnSnapshot {
  const learned = Array.isArray(settings?.vocab_terms) ? [...settings!.vocab_terms] : [];
  const pending = Array.isArray(settings?.edit_substitutions)
    ? settings!.edit_substitutions.filter((s) => !s.promoted).length
    : 0;
  return { learned, pendingCount: pending };
}
