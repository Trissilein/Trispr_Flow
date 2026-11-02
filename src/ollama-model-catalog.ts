import catalogJson from "./model-catalog.json";
import { normalizeModelTag } from "./ollama-tag-utils";

export type ModelCatalogSource = "ollama" | "huggingface";
export type ModelCatalogStatus = "recommended" | "supported" | "legacy";

/**
 * Proof that a model produced usable refinement output on a real runtime (written by hand after
 * a functional test run, never by the size verifier). `latency_p50_ms` is the median response time.
 */
export type ModelFunctionalProof = {
  verified_at: string;
  runtime: string;
  latency_p50_ms: number;
  note?: string;
};

/**
 * One curated, pullable model. `name` is the exact pull reference (`gemma4:e4b` or
 * `hf.co/org/repo:Quant`, case preserved). `size_gb` is decimal GB, summed over all registry
 * layers; `verified_at` (YYYY-MM-DD) is written by `scripts/verify-model-catalog.mjs --update`.
 */
export type ModelCatalogEntry = {
  name: string;
  label: string;
  size_gb: number;
  vram_gb?: number;
  profile: string;
  description: string;
  source: ModelCatalogSource;
  family: string;
  status: ModelCatalogStatus;
  verified_at?: string;
  note?: string;
  functional?: ModelFunctionalProof;
  /**
   * First public release of the weights (YYYY-MM-DD), taken from when the Hugging Face repository
   * `released_from` was created. Only kept together with its `released_from` source, never alone.
   */
  released?: string;
  /** Hugging Face repo (`org/repo`) that proves `released` and `license`. */
  released_from?: string;
  /** Short license name (`apache-2.0`), from the license tag of `released_from`. */
  license?: string;
};

const SOURCES: ReadonlySet<string> = new Set(["ollama", "huggingface"]);
const STATUSES: ReadonlySet<string> = new Set(["recommended", "supported", "legacy"]);
const HF_PREFIX = "hf.co/";
const DATE_ONLY = /^\d{4}-\d{2}-\d{2}$/;
const HF_REPO_ID = /^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/;
const RUNTIME_VERSION = /^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;

export function isHuggingFaceModelRef(name: string | null | undefined): boolean {
  return normalizeModelTag(name).startsWith(HF_PREFIX);
}

function nonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

/** A malformed proof is dropped (no "Verified working" chip), the entry itself stays usable. */
export function parseFunctionalProof(raw: unknown): ModelFunctionalProof | undefined {
  if (!raw || typeof raw !== "object") return undefined;
  const proof = raw as Record<string, unknown>;
  const { verified_at: verifiedAt, runtime, latency_p50_ms: latency, note } = proof;
  if (typeof verifiedAt !== "string" || !DATE_ONLY.test(verifiedAt)) return undefined;
  if (typeof runtime !== "string" || !RUNTIME_VERSION.test(runtime.trim())) return undefined;
  if (typeof latency !== "number" || !Number.isFinite(latency) || latency <= 0) return undefined;
  const parsed: ModelFunctionalProof = {
    verified_at: verifiedAt,
    runtime: runtime.trim().replace(/^v/i, ""),
    latency_p50_ms: Math.round(latency),
  };
  if (nonEmptyString(note)) parsed.note = note.trim();
  return parsed;
}

function parseEntry(raw: unknown): ModelCatalogEntry | null {
  if (!raw || typeof raw !== "object") return null;
  const entry = raw as Record<string, unknown>;
  const { name, label, profile, description, family, source, status, size_gb: sizeGb } = entry;
  if (
    !nonEmptyString(name) ||
    !nonEmptyString(label) ||
    !nonEmptyString(description) ||
    !nonEmptyString(source) ||
    !nonEmptyString(status) ||
    typeof sizeGb !== "number" ||
    !Number.isFinite(sizeGb) ||
    sizeGb <= 0 ||
    !SOURCES.has(source) ||
    !STATUSES.has(status)
  ) {
    return null;
  }
  // A Hugging Face source must be an hf.co ref and vice versa, otherwise the pull path is wrong.
  if ((source === "huggingface") !== isHuggingFaceModelRef(name)) return null;

  const parsed: ModelCatalogEntry = {
    name: name.trim(),
    label: label.trim(),
    size_gb: sizeGb,
    profile: typeof profile === "string" ? profile : "",
    description: description.trim(),
    source: source as ModelCatalogSource,
    family: typeof family === "string" ? family : "",
    status: status as ModelCatalogStatus,
  };
  const vram = entry.vram_gb;
  if (typeof vram === "number" && Number.isFinite(vram) && vram > 0) parsed.vram_gb = vram;
  if (typeof entry.verified_at === "string" && DATE_ONLY.test(entry.verified_at)) {
    parsed.verified_at = entry.verified_at;
  }
  if (nonEmptyString(entry.note)) parsed.note = entry.note.trim();
  const functional = parseFunctionalProof(entry.functional);
  if (functional) parsed.functional = functional;
  // A release date without its source is dropped: no invented dates.
  const releasedFrom = typeof entry.released_from === "string" ? entry.released_from.trim() : "";
  if (HF_REPO_ID.test(releasedFrom)) {
    parsed.released_from = releasedFrom;
    if (typeof entry.released === "string" && DATE_ONLY.test(entry.released)) parsed.released = entry.released;
    if (nonEmptyString(entry.license) && entry.license.trim().length <= 60) parsed.license = entry.license.trim();
  }
  return parsed;
}

/** Keeps valid entries only; duplicates (case-insensitive on the pull ref) keep the first. */
export function parseModelCatalog(raw: unknown): ModelCatalogEntry[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const entries: ModelCatalogEntry[] = [];
  for (const item of raw) {
    const entry = parseEntry(item);
    if (!entry) continue;
    const key = normalizeModelTag(entry.name);
    if (seen.has(key)) continue;
    seen.add(key);
    entries.push(entry);
  }
  return entries;
}

export const MODEL_CATALOG: ModelCatalogEntry[] = parseModelCatalog(catalogJson);

export function catalogSourceLabel(source: ModelCatalogSource): string {
  return source === "huggingface" ? "Hugging Face" : "Ollama library";
}
