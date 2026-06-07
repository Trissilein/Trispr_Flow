/**
 * LLM-assisted periodic cleanup of the vocab edit_substitutions list.
 *
 * Runs at most once every CLEANUP_INTERVAL_MS (4 days) at app start.
 * Uses whatever OLLAMA model is currently loaded in memory (api/ps),
 * falling back to the configured postproc_llm_model.
 *
 * Two-phase approach:
 *   1. Pre-filter: deterministic rules catch obvious junk (URLs, sentence
 *      boundary artifacts, contradictory pairs) without an LLM call.
 *   2. LLM pass: classifies the rest as "keep" or "delete".
 *      Default on any error or ambiguity: keep.
 */

import type { EditSubstitution } from "./types";
import { settings } from "./state";
import { persistSettings } from "./settings-persist";
import { renderLearnedVocabChips } from "./settings/vocabulary.settings";

const CLEANUP_INTERVAL_MS = 4 * 24 * 60 * 60 * 1000;
const MIN_ENTRIES_TO_CLEAN = 5;
const BATCH_SIZE = 20;
const OLLAMA_TIMEOUT_MS = 60_000;

// ---------------------------------------------------------------------------
// OLLAMA helpers
// ---------------------------------------------------------------------------

function resolveEndpoint(): string {
  return settings?.providers?.ollama?.endpoint?.trim() || "http://localhost:11434";
}

async function getLoadedModel(endpoint: string): Promise<string | null> {
  try {
    const resp = await fetch(`${endpoint}/api/ps`, {
      signal: AbortSignal.timeout(5_000),
    });
    if (!resp.ok) return null;
    const json = (await resp.json()) as { models?: Array<{ name: string }> };
    const first = json.models?.[0]?.name;
    return first ?? null;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Phase 1 — deterministic pre-filter (no LLM)
// ---------------------------------------------------------------------------

function preFilter(subs: EditSubstitution[]): {
  keep: EditSubstitution[];
  deleted: EditSubstitution[];
} {
  // Build index for contradictory-pair detection
  const index = new Set(subs.map((s) => `${s.from.toLowerCase()}|||${s.to.toLowerCase()}`));

  const keep: EditSubstitution[] = [];
  const deleted: EditSubstitution[] = [];

  for (const sub of subs) {
    // URL in target
    if (/https?:\/\//.test(sub.to)) {
      deleted.push(sub);
      continue;
    }
    // Sentence boundary artifact: punctuation immediately followed by capital letter
    if (/[.!?][A-ZÄÖÜ]/.test(sub.to)) {
      deleted.push(sub);
      continue;
    }
    // Contradictory pair: A→B and B→A both exist
    const forward = `${sub.from.toLowerCase()}|||${sub.to.toLowerCase()}`;
    const reverse = `${sub.to.toLowerCase()}|||${sub.from.toLowerCase()}`;
    if (index.has(forward) && index.has(reverse)) {
      deleted.push(sub);
      continue;
    }
    keep.push(sub);
  }

  return { keep, deleted };
}

// ---------------------------------------------------------------------------
// Phase 2 — LLM classification
// ---------------------------------------------------------------------------

function buildPrompt(batch: EditSubstitution[]): string {
  const lines = batch
    .map((s, i) => `${i + 1}. "${s.from}" → "${s.to}" (seen ${s.count}×)`)
    .join("\n");

  return `You review a vocabulary correction list for a speech-to-text dictation app.
Each entry: what the speech recognizer said → what the user corrected it to (seen N times).
The app auto-applies corrections that repeat often enough.

Classify each as "keep" or "delete":
- keep: plausible recurring correction (proper noun, brand, technical term, jargon, foreign word)
- delete: junk — common grammar/function words (das/den/die/ich/und/eine/muss/macht/per), \
single-use style changes, implausible substitutions, or anything where the target looks wrong

When uncertain: keep.

Reply ONLY with a JSON array, no markdown, no explanation:
[{"i":1,"action":"keep"},{"i":2,"action":"delete"},...]

Entries:
${lines}`;
}

function stripThinking(raw: string): string {
  return raw.replace(/<think>[\s\S]*?<\/think>/gi, "").trim();
}

async function classifyBatch(
  batch: EditSubstitution[],
  model: string,
  endpoint: string,
): Promise<Map<number, "keep" | "delete">> {
  // Default: keep everything (safe)
  const result = new Map<number, "keep" | "delete">();
  batch.forEach((_, i) => result.set(i + 1, "keep"));

  try {
    const resp = await fetch(`${endpoint}/api/chat`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model,
        stream: false,
        options: { temperature: 0 },
        messages: [{ role: "user", content: buildPrompt(batch) }],
      }),
      signal: AbortSignal.timeout(OLLAMA_TIMEOUT_MS),
    });
    if (!resp.ok) return result;

    const json = (await resp.json()) as {
      message?: { content?: string };
      response?: string;
    };
    const raw = json.message?.content ?? json.response ?? "";
    const cleaned = stripThinking(raw);

    // LLMs sometimes wrap JSON in markdown fences — extract the array
    const match = cleaned.match(/\[[\s\S]*\]/);
    if (!match) return result;

    const decisions = JSON.parse(match[0]) as Array<{ i: number; action: string }>;
    for (const d of decisions) {
      if (
        typeof d.i === "number" &&
        d.i >= 1 &&
        d.i <= batch.length &&
        (d.action === "keep" || d.action === "delete")
      ) {
        result.set(d.i, d.action as "keep" | "delete");
      }
    }
  } catch {
    // Network error, timeout, parse failure → keep-all default stays
  }

  return result;
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

export async function runVocabCleanup(): Promise<void> {
  if (!settings) return;

  const subs = Array.isArray(settings.edit_substitutions)
    ? [...settings.edit_substitutions]
    : [];
  if (subs.length < MIN_ENTRIES_TO_CLEAN) return;

  const endpoint = resolveEndpoint();
  const model =
    (await getLoadedModel(endpoint)) || settings.postproc_llm_model?.trim() || "";
  if (!model) return;

  // Phase 1
  const { keep: afterPreFilter, deleted: preDeleted } = preFilter(subs);

  // Phase 2 — batch through LLM
  const llmDeleted: EditSubstitution[] = [];
  const remaining: EditSubstitution[] = [];

  for (let i = 0; i < afterPreFilter.length; i += BATCH_SIZE) {
    const batch = afterPreFilter.slice(i, i + BATCH_SIZE);
    const decisions = await classifyBatch(batch, model, endpoint);
    for (let j = 0; j < batch.length; j++) {
      if (decisions.get(j + 1) === "delete") {
        llmDeleted.push(batch[j]);
      } else {
        remaining.push(batch[j]);
      }
    }
  }

  settings.edit_substitutions = remaining;
  settings.last_vocab_cleanup_ms = Date.now();
  await persistSettings();

  if (preDeleted.length + llmDeleted.length > 0) {
    renderLearnedVocabChips();
  }
}

export function scheduleVocabCleanupIfNeeded(): void {
  if (!settings) return;
  const last = settings.last_vocab_cleanup_ms ?? 0;
  if (Date.now() - last < CLEANUP_INTERVAL_MS) return;
  void runVocabCleanup();
}
