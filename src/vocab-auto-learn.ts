/**
 * Vocabulary auto-learning with variant clustering.
 *
 * Observes final transcripts (refined if available, raw otherwise), extracts
 * tokens that look like proper nouns, acronyms, or project-specific terms,
 * clusters variants of the same word together (so "Trispa", "Trisper",
 * "TrisperFlow" all feed one counter for "Trispr"), and promotes clusters
 * into `settings.vocab_terms` once their summed count crosses the threshold.
 *
 * On promotion, variant spellings (count ≥ 2) are also written back as
 * `postproc_custom_vocab` find-replace rules so already-misheard tokens get
 * corrected on the next transcript — not just biased away from via Whisper
 * hints.
 *
 * Signal sources:
 *  - Structural: CamelCase / acronym / hyphen-compound-with-caps — these
 *    shapes are effectively never regular vocabulary. Promoted after 3
 *    sightings (summed across variants).
 *  - Plain capitalized: single-capital nouns mid-sentence, minus a small
 *    stopword list of common German/English generic nouns and frequent
 *    sentence-start words. Promoted after 8 sightings.
 *
 * Everything else (all-lowercase words, punctuation, numbers, sentence-start
 * capitals without mid-sentence confirmation) is ignored — no counter, no
 * state churn.
 */

import type { VocabTermCandidate } from "./types";
import { settings } from "./state";
import { persistSettings } from "./settings";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const PROMOTION_THRESHOLD_STRUCTURAL = 3;
const PROMOTION_THRESHOLD_PLAIN = 8;
const MAX_CANDIDATES = 500;
const MAX_TERMS = 200;

/** Minimum per-variant count required for it to become a rewrite rule on promotion. */
const REWRITE_MIN_VARIANT_COUNT = 2;

/** Similarity thresholds for variant clustering. */
const MIN_TOKEN_LEN_FOR_CLUSTER = 3;
const MIN_COMMON_PREFIX_HARD_CAP = 4;
const MAX_NORMALIZED_LEV_DISTANCE = 0.34;

const STOPWORDS = new Set<string>([
  "Also", "Ich", "Du", "Er", "Sie", "Wir", "Ihr", "Es", "Man",
  "Das", "Der", "Die", "Ein", "Eine", "Einen", "Einer", "Einem", "Eines",
  "Und", "Aber", "Oder", "Wenn", "Weil", "Denn", "Doch", "Noch", "Schon",
  "Ja", "Nein", "Ok", "Okay", "Hallo", "Tschüss", "Danke", "Bitte",
  "Vielleicht", "Eigentlich", "Wirklich", "Irgendwie", "Überhaupt",
  "Heute", "Gestern", "Morgen", "Jetzt", "Dann", "Hier", "Dort", "Da",
  "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag",
  "Januar", "Februar", "März", "April", "Mai", "Juni", "Juli",
  "August", "September", "Oktober", "November", "Dezember",
  "Ganze", "Sinn", "Dinge", "Ding", "Problem", "Fall", "Ende", "Anfang",
  "Mitte", "Beispiel", "Mal", "Konzept", "Variante", "Versionen", "Version",
  "Text", "Fehler", "Punkt", "Plan", "Session", "System", "Spiel",
  "Modell", "Modul", "Bilder", "Bild", "Moment", "Sache", "Sachen",
  "Informationen", "Information", "Screenshot", "Screenshots",
  "Sprachausgabe", "Wetter", "Wetterdaten", "Schritt", "Schritte", "Weise",
  "Job", "Jobs", "Welt", "Ort", "Richtung", "Minute", "Minuten",
  "Stunde", "Stunden", "Tag", "Jahr", "Haus", "Leute", "Mensch", "Person", "Leben",
  "Name", "Nummer", "Wort", "Worte", "Wörter", "Satz", "Sätze",
  "Arbeit", "Kollege", "Kollegen", "Freund", "Freunde", "Gruppe",
  "Stelle", "Stellen", "Lösung", "Lösungen", "Grund", "Gründe",
  "Teil", "Teile", "Frage", "Fragen", "Antwort", "Antworten",
  "Ansatz", "Ansätze", "Idee", "Ideen", "Aspekt", "Aspekte",
  "Ausgabe", "Ausgang", "Eingabe", "Eingang", "Art", "Seite", "Seiten",
  "Zeit", "Zeiten", "Stück", "Stücke", "Weg", "Wege",
  "Anforderung", "Anforderungen", "Änderung", "Änderungen",
  "Aufnahme", "Aufnahmen", "Anweisung", "Anweisungen",
  "Beschreibung", "Beschreibungen", "Aussage", "Aussagen",
  "Auswahl", "Analyse", "Analysen", "Agent", "Agenten", "Agents",
  "Alternative", "Alternativen", "Animation", "Anleitung",
  "Abschnitt", "Abschnitte", "Account", "Accounts",
  "Ahnung", "Architektur",
  "The", "A", "An", "I", "You", "He", "She", "We", "They", "It",
  "This", "That", "These", "Those",
  "And", "But", "Or", "Not", "If", "Then", "So", "Because",
  "Yes", "No", "Hello", "Thanks", "Please",
  "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
  "Actually", "Additional", "Another", "Storage", "Environment", "Review",
  "Add", "Subscription", "Wrapper", "Patch", "Germany", "Zero", "Hero",
  "NOT", "AND", "OR", "ONLY", "SAME", "NEVER", "ALWAYS", "IMPORTANT",
  "NOTE", "WARNING", "TODO", "TBD", "FIXME", "OK", "YES",
  "DO", "IS", "ARE", "BE", "AS", "AT", "IN", "ON", "OF", "TO", "FOR",
  "Do", "Is", "Are", "Be", "As", "At", "In", "On", "Of", "To", "For",
  "Can", "Will", "Would", "Should", "Could", "Must", "May", "Might",
  "Liste", "Listen", "Modelle", "Komplexität", "Interface", "Interfaces",
  "Hamburg", "Toggle", "Zugriff", "Datei", "Dateien", "Code", "Level",
  "Projekt", "Projekte", "Mode", "Modus", "Studio", "Request", "Requests",
  "Roadmap", "Balancing", "Punkte", "Upgrades", "Upgrade", "Constraint",
  "Constraints", "Haiku", "Punctuation", "Detect", "Wait",
]);

// ---------------------------------------------------------------------------
// Token classification
// ---------------------------------------------------------------------------

const EDGE_PUNCT_RE = /^[\p{P}\p{S}]+|[\p{P}\p{S}]+$/gu;
function stripEdgePunct(token: string): string {
  return token.replace(EDGE_PUNCT_RE, "");
}

function isCamelCase(t: string): boolean {
  if (!/[A-ZÄÖÜ]/.test(t) || !/[a-zäöüß]/.test(t)) return false;
  return /[a-zäöüß][A-ZÄÖÜ]/.test(t);
}

function isAcronym(t: string): boolean {
  return /^[A-ZÄÖÜ]{2,6}([0-9A-ZÄÖÜ]{0,3})?$/.test(t);
}

function isHyphenMixed(t: string): boolean {
  if (!/-/.test(t)) return false;
  const parts = t.split("-");
  if (parts.length < 2 || parts.some((p) => p.length === 0)) return false;
  if (!/[A-ZÄÖÜ]/.test(t)) return false;
  return parts.some((p) => /[A-ZÄÖÜ]/.test(p));
}

function isPlainCapitalized(t: string): boolean {
  return /^[A-ZÄÖÜ][a-zäöüß]+$/.test(t);
}

export type SeenKind = "structural" | "plain";

type Classification =
  | { kind: "reject" }
  | { kind: "structural"; canonical: string }
  | { kind: "plain"; canonical: string };

function classifyToken(raw: string, isSentenceStart: boolean): Classification {
  const clean = stripEdgePunct(raw);
  if (clean.length < 2) return { kind: "reject" };

  if (isCamelCase(clean) || isAcronym(clean) || isHyphenMixed(clean)) {
    if (STOPWORDS.has(clean)) return { kind: "reject" };
    return { kind: "structural", canonical: clean };
  }

  if (isPlainCapitalized(clean) && !isSentenceStart && !STOPWORDS.has(clean)) {
    return { kind: "plain", canonical: clean };
  }

  return { kind: "reject" };
}

// ---------------------------------------------------------------------------
// Similarity — variant clustering
// ---------------------------------------------------------------------------

/** Length of the longest common lowercase prefix. */
export function commonPrefixLen(a: string, b: string): number {
  const al = a.toLowerCase();
  const bl = b.toLowerCase();
  const max = Math.min(al.length, bl.length);
  let i = 0;
  while (i < max && al.charCodeAt(i) === bl.charCodeAt(i)) i += 1;
  return i;
}

/**
 * Classic Levenshtein with a two-row rolling buffer. Operates on the strings
 * as-is (caller is expected to lowercase both sides if case-insensitivity
 * is wanted).
 */
export function levenshtein(a: string, b: string): number {
  if (a === b) return 0;
  if (a.length === 0) return b.length;
  if (b.length === 0) return a.length;
  const m = a.length;
  const n = b.length;
  let prev = new Array<number>(n + 1);
  let curr = new Array<number>(n + 1);
  for (let j = 0; j <= n; j += 1) prev[j] = j;
  for (let i = 1; i <= m; i += 1) {
    curr[0] = i;
    const ca = a.charCodeAt(i - 1);
    for (let j = 1; j <= n; j += 1) {
      const cost = ca === b.charCodeAt(j - 1) ? 0 : 1;
      const del = prev[j] + 1;
      const ins = curr[j - 1] + 1;
      const sub = prev[j - 1] + cost;
      curr[j] = del < ins ? (del < sub ? del : sub) : ins < sub ? ins : sub;
    }
    [prev, curr] = [curr, prev];
  }
  return prev[n];
}

/**
 * Strip compound suffixes so variant heads can be compared against cluster
 * representatives. "Trispa-Flow" → "Trispa"; "TrisperFlow" → "Trisper".
 *
 * - Everything after the first "-" is dropped.
 * - A trailing CamelHump of ≤ 5 letters is dropped if what remains is still
 *   ≥ 4 letters (so "TrisperFlow" → "Trisper" but "XPBar" stays "XPBar").
 */
export function canonicalHead(token: string): string {
  let head = token;
  const hyphenIdx = head.indexOf("-");
  if (hyphenIdx > 0) head = head.slice(0, hyphenIdx);
  const camelMatch = /^([A-ZÄÖÜ][a-zäöüß]+)([A-ZÄÖÜ][A-Za-zÄÖÜäöüß]{0,4})$/.exec(head);
  if (camelMatch && camelMatch[1].length >= 4) {
    head = camelMatch[1];
  }
  return head;
}

/**
 * True if a and b look like spellings of the same word. Two-stage:
 *   1. Common prefix ≥ min(4, min_len − 1) — kills "Trispr" vs "Trippe".
 *   2. Normalized Levenshtein ≤ 0.34 — kills "TrisperFlow" vs "Flow".
 *
 * Both stages try the raw token AND its `canonicalHead`, so hyphen/compound
 * variants collapse onto the head word.
 */
export function isSimilar(a: string, b: string): boolean {
  if (a === b) return true;
  if (a.length < MIN_TOKEN_LEN_FOR_CLUSTER || b.length < MIN_TOKEN_LEN_FOR_CLUSTER) {
    return false;
  }
  const aHeads = new Set<string>([a, canonicalHead(a)]);
  const bHeads = new Set<string>([b, canonicalHead(b)]);
  for (const ah of aHeads) {
    for (const bh of bHeads) {
      if (pairIsSimilar(ah, bh)) return true;
    }
  }
  return false;
}

function pairIsSimilar(a: string, b: string): boolean {
  const minLen = Math.min(a.length, b.length);
  if (minLen < MIN_TOKEN_LEN_FOR_CLUSTER) return false;
  const prefixNeeded = Math.min(MIN_COMMON_PREFIX_HARD_CAP, minLen - 1);
  if (commonPrefixLen(a, b) < prefixNeeded) return false;
  const dist = levenshtein(a.toLowerCase(), b.toLowerCase());
  const normalized = dist / Math.max(a.length, b.length);
  return normalized <= MAX_NORMALIZED_LEV_DISTANCE;
}

/**
 * Elect the canonical form of a cluster:
 * 1. Most-frequent variant wins.
 * 2. Tie → longest.
 * 3. Tie → first inserted (iteration order of the variants object).
 */
export function pickCanonical(variants: Record<string, number>): string {
  const entries = Object.entries(variants);
  if (entries.length === 0) return "";
  let bestForm = entries[0][0];
  let bestCount = entries[0][1];
  for (let i = 1; i < entries.length; i += 1) {
    const [form, count] = entries[i];
    if (count > bestCount) {
      bestForm = form;
      bestCount = count;
      continue;
    }
    if (count === bestCount && form.length > bestForm.length) {
      bestForm = form;
      bestCount = count;
    }
    // Equal count and equal length → keep earlier (stable).
  }
  return bestForm;
}

// ---------------------------------------------------------------------------
// Per-transcript ingestion
// ---------------------------------------------------------------------------

function splitSentences(text: string): string[] {
  return text.split(/(?<=[.!?])[\s"'»”›]*\s+/).filter((s) => s.trim().length > 0);
}

function collectCandidates(text: string): Map<string, { canonical: string; kind: SeenKind }> {
  const out = new Map<string, { canonical: string; kind: SeenKind }>();
  for (const sentence of splitSentences(text)) {
    const tokens = sentence.split(/\s+/).filter((t) => t.length > 0);
    for (let i = 0; i < tokens.length; i += 1) {
      const res = classifyToken(tokens[i], i === 0);
      if (res.kind === "reject") continue;
      const key = res.canonical.toLowerCase();
      const prev = out.get(key);
      if (!prev || (prev.kind === "plain" && res.kind === "structural")) {
        out.set(key, { canonical: res.canonical, kind: res.kind });
      }
    }
  }
  return out;
}

/** Find an existing candidate whose canonical OR any recorded variant is similar. */
function findMatchingCandidate(
  token: string,
  candidates: VocabTermCandidate[],
): VocabTermCandidate | undefined {
  for (const c of candidates) {
    if (isSimilar(token, c.term)) return c;
    if (c.variants) {
      for (const v of Object.keys(c.variants)) {
        if (v === c.term) continue;
        if (isSimilar(token, v)) return c;
      }
    }
  }
  return undefined;
}

/** Find a promoted term similar to the token. Returns the exact string stored in vocab_terms. */
function findMatchingPromoted(token: string, terms: string[]): string | undefined {
  for (const t of terms) {
    if (isSimilar(token, t)) return t;
  }
  return undefined;
}

interface IngestResult {
  /** Rewrite pairs (from → to) to install into postproc_custom_vocab. */
  rewrites: Array<[string, string]>;
}

/**
 * Entry point called from the transcription event listeners. Updates the
 * candidate counters, merges variants into clusters, and promotes anything
 * whose summed count crosses the threshold. Fire-and-forget: persistSettings
 * handles its own errors. Auto-rewrites are installed asynchronously via a
 * dynamic import to avoid a circular dependency with event-listeners.ts.
 */
export function ingestTranscriptForAutoLearning(text: string): void {
  if (!settings) return;
  if (!text || text.trim().length === 0) return;

  const candidates = collectCandidates(text);
  if (candidates.size === 0) return;

  const result = applyCandidatesToState(candidates);
  void persistSettings();
  if (result.rewrites.length > 0) {
    void installAutoRewrites(result.rewrites);
  }
}

/**
 * Pure state-mutation step. Exposed for tests so they can drive the ingest
 * logic without needing the full `ingestTranscriptForAutoLearning` IO path.
 */
export function applyCandidatesToState(
  candidates: Map<string, { canonical: string; kind: SeenKind }>,
): IngestResult {
  if (!settings) return { rewrites: [] };

  const now = Date.now();
  const list: VocabTermCandidate[] = Array.isArray(settings.vocab_term_candidates)
    ? [...settings.vocab_term_candidates]
    : [];
  const termsList = Array.isArray(settings.vocab_terms) ? [...settings.vocab_terms] : [];
  const termsSet = new Set(termsList.map((t) => t.toLowerCase()));
  const rewrites: Array<[string, string]> = [];

  for (const { canonical } of candidates.values()) {
    // Already-promoted-cluster match? Record the variant as a rewrite and skip counter work.
    const promotedMatch = findMatchingPromoted(canonical, termsList);
    if (promotedMatch && promotedMatch !== canonical) {
      pushRewritePair(rewrites, canonical, promotedMatch);
      continue;
    }
    if (promotedMatch === canonical) {
      continue; // already the canonical form of a promoted cluster
    }

    // Existing candidate (exact term or variant) → merge.
    const existing = findMatchingCandidate(canonical, list);
    if (existing) {
      const variants = ensureVariants(existing);
      variants[canonical] = (variants[canonical] ?? 0) + 1;
      existing.count += 1;
      existing.last_seen_ms = now;
      existing.term = pickCanonical(variants);
    } else {
      const fresh: VocabTermCandidate = {
        term: canonical,
        count: 1,
        first_seen_ms: now,
        last_seen_ms: now,
        variants: { [canonical]: 1 },
      };
      list.push(fresh);
    }
  }

  // Promote anything that crossed its threshold.
  const survivors: VocabTermCandidate[] = [];
  for (const entry of list) {
    const effectiveKind = classifyCandidateKind(entry.term);
    const threshold =
      effectiveKind === "structural"
        ? PROMOTION_THRESHOLD_STRUCTURAL
        : PROMOTION_THRESHOLD_PLAIN;
    if (entry.count >= threshold && !termsSet.has(entry.term.toLowerCase())) {
      termsList.push(entry.term);
      termsSet.add(entry.term.toLowerCase());
      if (entry.variants) {
        for (const [variant, count] of Object.entries(entry.variants)) {
          if (variant === entry.term) continue;
          if (count < REWRITE_MIN_VARIANT_COUNT) continue;
          pushRewritePair(rewrites, variant, entry.term);
        }
      }
      continue; // drop promoted entry from candidates
    }
    survivors.push(entry);
  }

  // Cap sizes to keep settings JSON compact.
  let cappedList = survivors;
  if (survivors.length > MAX_CANDIDATES) {
    cappedList = survivors
      .slice()
      .sort((a, b) => b.last_seen_ms - a.last_seen_ms)
      .slice(0, MAX_CANDIDATES);
  }
  let cappedTerms = termsList;
  if (termsList.length > MAX_TERMS) {
    cappedTerms = termsList.slice(termsList.length - MAX_TERMS);
  }

  settings.vocab_term_candidates = cappedList;
  settings.vocab_terms = cappedTerms;

  return { rewrites };
}

function ensureVariants(entry: VocabTermCandidate): Record<string, number> {
  if (entry.variants && typeof entry.variants === "object") return entry.variants;
  // Legacy entry: interpret the existing term/count as a single-variant cluster.
  const seeded: Record<string, number> = { [entry.term]: entry.count };
  entry.variants = seeded;
  return seeded;
}

function classifyCandidateKind(term: string): SeenKind {
  if (isCamelCase(term) || isAcronym(term) || isHyphenMixed(term)) return "structural";
  return "plain";
}

/**
 * Install a from → to pair plus, if `from` starts uppercase, also its
 * lowercased form. Keeps `apply_custom_vocabulary` semantics case-sensitive
 * while covering the two forms that actually matter for transcripts.
 */
function pushRewritePair(
  out: Array<[string, string]>,
  from: string,
  to: string,
): void {
  if (from === to) return;
  out.push([from, to]);
  const lowered = from.toLowerCase();
  if (lowered !== from && lowered !== to.toLowerCase()) {
    out.push([lowered, to]);
  }
}

async function installAutoRewrites(pairs: Array<[string, string]>): Promise<void> {
  try {
    const { addVocabEntriesFromSuggestions } = await import("./event-listeners");
    await addVocabEntriesFromSuggestions(pairs);
  } catch (err) {
    console.warn("[vocab-auto-learn] failed to install auto-rewrite rules", err);
  }
}

// ---------------------------------------------------------------------------
// UI helpers
// ---------------------------------------------------------------------------

/** Remove a learned term (and any matching candidate). Used by the UI × button. */
export async function dismissLearnedTerm(term: string): Promise<void> {
  if (!settings) return;
  const key = term.toLowerCase();
  const nextTerms = (settings.vocab_terms ?? []).filter((t) => t.toLowerCase() !== key);
  const nextCandidates = (settings.vocab_term_candidates ?? []).filter(
    (c) => c.term.toLowerCase() !== key
  );
  const changed =
    nextTerms.length !== (settings.vocab_terms ?? []).length
    || nextCandidates.length !== (settings.vocab_term_candidates ?? []).length;
  if (!changed) return;
  settings.vocab_terms = nextTerms;
  settings.vocab_term_candidates = nextCandidates;
  await persistSettings();
}

/** Remove a single variant from a cluster candidate. If the cluster empties, drop it. */
export async function dismissCandidateVariant(
  clusterTerm: string,
  variant: string,
): Promise<void> {
  if (!settings) return;
  const list = settings.vocab_term_candidates ?? [];
  const clusterKey = clusterTerm.toLowerCase();
  const entry = list.find((c) => c.term.toLowerCase() === clusterKey);
  if (!entry) return;
  const variants = entry.variants ? { ...entry.variants } : {};
  const removedCount = variants[variant] ?? 0;
  if (removedCount === 0) return;
  delete variants[variant];
  const remaining = Object.keys(variants).length;
  if (remaining === 0) {
    settings.vocab_term_candidates = list.filter((c) => c !== entry);
  } else {
    entry.variants = variants;
    entry.count = Math.max(0, entry.count - removedCount);
    entry.term = pickCanonical(variants);
    settings.vocab_term_candidates = [...list];
  }
  await persistSettings();
}

/** Snapshot of what the auto-learner has produced, for UI display. */
export interface AutoLearnSnapshot {
  learned: string[];
  pendingCount: number;
}

export function getAutoLearnSnapshot(): AutoLearnSnapshot {
  const learned = Array.isArray(settings?.vocab_terms) ? [...settings!.vocab_terms] : [];
  const pending = Array.isArray(settings?.vocab_term_candidates)
    ? settings!.vocab_term_candidates.length
    : 0;
  return { learned, pendingCount: pending };
}

/** Expose threshold values for the UI (progress indicators on observing chips). */
export function promotionThresholdFor(candidate: VocabTermCandidate): number {
  return classifyCandidateKind(candidate.term) === "structural"
    ? PROMOTION_THRESHOLD_STRUCTURAL
    : PROMOTION_THRESHOLD_PLAIN;
}
