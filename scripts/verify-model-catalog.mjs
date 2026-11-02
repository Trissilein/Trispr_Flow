#!/usr/bin/env node
// Verifies src/model-catalog.json against the live registries.
//
//   node scripts/verify-model-catalog.mjs            report only
//   node scripts/verify-model-catalog.mjs --update   write size_gb + verified_at for resolved refs
//   --catalog <file>                                 check another catalog file (default: src/model-catalog.json)
//
// Per ref: GET the manifest (Ollama registry or Hugging Face OCI endpoint), sum all layer sizes,
// compare with size_gb (decimal GB, like the registries show). Exit code 1 if any ref cannot be
// resolved. Size drift alone is reported but does not fail the run (run --update to fix it).
// Only small manifest requests are made - no model data is downloaded.

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const DEFAULT_CATALOG_PATH = resolve(dirname(fileURLToPath(import.meta.url)), "../src/model-catalog.json");
const MANIFEST_ACCEPT = "application/vnd.docker.distribution.manifest.v2+json";
const REQUEST_TIMEOUT_MS = 15_000;
/** Registry sizes and the catalog are both rounded; ignore differences below this many GB. */
export const DRIFT_TOLERANCE_GB = 0.05;
const KEY_ORDER = [
  "name",
  "label",
  "source",
  "family",
  "status",
  "profile",
  "size_gb",
  "vram_gb",
  "verified_at",
  "released",
  "released_from",
  "license",
  "description",
  "note",
];

/** `gemma4:e4b`, `user/model:tag`, `registry.ollama.ai/library/x:tag`, `hf.co/org/repo:Quant`. */
export function parseModelRef(name) {
  const trimmed = String(name ?? "").trim();
  const hf = trimmed.match(/^(?:hf\.co|huggingface\.co)\/(.+)$/i);
  const rest = hf ? hf[1] : trimmed.replace(/^registry\.ollama\.ai\//i, "");
  const colon = rest.lastIndexOf(":");
  const pathPart = colon === -1 ? rest : rest.slice(0, colon);
  const tag = colon === -1 ? "latest" : rest.slice(colon + 1);
  const segments = pathPart.split("/");
  const validSegment = (s) => /^[A-Za-z0-9._-]+$/.test(s) && s !== "." && s !== "..";
  if (!validSegment(tag) || !segments.every(validSegment)) {
    throw new Error(`invalid model ref '${trimmed}'`);
  }
  if (hf) {
    if (segments.length !== 2) throw new Error(`Hugging Face refs need org/repo: '${trimmed}'`);
    return { registry: "huggingface", path: segments.join("/"), tag };
  }
  let path;
  if (segments.length === 1) path = `library/${segments[0]}`;
  else if (segments.length === 2) path = segments.join("/");
  else if (segments.length === 3 && segments[0] === "library") path = segments.slice(1).join("/");
  else throw new Error(`unsupported Ollama ref '${trimmed}'`);
  return { registry: "ollama", path, tag };
}

export function manifestUrl(ref) {
  const host = ref.registry === "huggingface" ? "https://huggingface.co" : "https://registry.ollama.ai";
  return `${host}/v2/${ref.path}/manifests/${ref.tag}`;
}

export function sumLayerBytes(manifest) {
  if (!manifest || !Array.isArray(manifest.layers)) return 0;
  return manifest.layers.reduce((sum, layer) => sum + (Number.isFinite(layer?.size) ? layer.size : 0), 0);
}

/** 6_583_658_527 -> 6.58 */
export function bytesToGb(bytes) {
  return Math.round((bytes / 1e9) * 100) / 100;
}

export function hasDrift(catalogGb, registryGb) {
  return !(Math.abs(catalogGb - registryGb) <= DRIFT_TOLERANCE_GB);
}

/** Hugging Face API URL that proves `released` (repo creation) and `license` (license tag). */
export function evidenceUrl(repo) {
  return `https://huggingface.co/api/models/${repo}?expand%5B%5D=createdAt&expand%5B%5D=tags`;
}

/** What the Hugging Face answer says: `{ released: "YYYY-MM-DD" | null, license: string | null }`. */
export function readEvidence(model) {
  const created = typeof model?.createdAt === "string" ? model.createdAt.slice(0, 10) : null;
  const tag = Array.isArray(model?.tags) ? model.tags.find((t) => typeof t === "string" && t.startsWith("license:")) : null;
  return {
    released: created && /^\d{4}-\d{2}-\d{2}$/.test(created) ? created : null,
    license: tag ? tag.slice("license:".length) : null,
  };
}

/** Differences between an entry's claims and the evidence; empty when they agree (or nothing is claimed). */
export function evidenceDrift(entry, evidence) {
  const problems = [];
  if (entry.released && entry.released !== evidence.released) {
    problems.push(`released ${entry.released} != ${evidence.released ?? "?"}`);
  }
  if (entry.license && entry.license !== evidence.license) {
    problems.push(`license ${entry.license} != ${evidence.license ?? "?"}`);
  }
  return problems;
}

export async function fetchEvidence(repo) {
  const response = await fetch(evidenceUrl(repo), {
    headers: { Accept: "application/json", "User-Agent": "TrisprFlow/verify-model-catalog" },
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  return readEvidence(await response.json());
}

export async function fetchManifest(ref) {
  const response = await fetch(manifestUrl(ref), {
    headers: { Accept: MANIFEST_ACCEPT, "User-Agent": "TrisprFlow/verify-model-catalog" },
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  const body = Buffer.from(await response.arrayBuffer());
  const manifest = JSON.parse(body.toString("utf8"));
  return {
    manifest,
    digest: createHash("sha256").update(body).digest("hex"),
    bytes: sumLayerBytes(manifest),
  };
}

function orderedEntry(entry) {
  const known = KEY_ORDER.filter((key) => key in entry).map((key) => [key, entry[key]]);
  const extra = Object.entries(entry).filter(([key]) => !KEY_ORDER.includes(key));
  return Object.fromEntries([...known, ...extra]);
}

function pad(value, width) {
  return String(value).padEnd(width, " ");
}

async function main() {
  const update = process.argv.includes("--update");
  const catalogFlag = process.argv.indexOf("--catalog");
  const catalogPath = catalogFlag === -1 ? DEFAULT_CATALOG_PATH : process.argv[catalogFlag + 1];
  const catalog = JSON.parse(readFileSync(catalogPath, "utf8"));
  const today = new Date().toISOString().slice(0, 10);
  let unresolved = 0;
  let drifted = 0;
  let evidenceProblems = 0;

  const rows = [];
  for (const entry of catalog) {
    let ref;
    try {
      ref = parseModelRef(entry.name);
    } catch (error) {
      rows.push({ entry, error: error.message });
      unresolved += 1;
      continue;
    }
    try {
      const { digest, bytes } = await fetchManifest(ref);
      if (bytes <= 0) throw new Error("manifest has no layers");
      const registryGb = bytesToGb(bytes);
      const drift = hasDrift(entry.size_gb, registryGb);
      if (drift) drifted += 1;
      const row = { entry, digest, registryGb, drift };
      if (entry.released_from) {
        try {
          row.evidence = await fetchEvidence(entry.released_from);
          row.evidenceProblems = evidenceDrift(entry, row.evidence);
          if (row.evidenceProblems.length > 0) evidenceProblems += 1;
        } catch (error) {
          row.evidenceProblems = [`source ${entry.released_from} unreachable: ${error.message}`];
          evidenceProblems += 1;
        }
      }
      rows.push(row);
    } catch (error) {
      unresolved += 1;
      rows.push({ entry, error: error.message });
    }
  }

  const nameWidth = Math.max(...rows.map((row) => row.entry.name.length), 4);
  console.log(`${pad("ref", nameWidth)}  ${pad("status", 10)}  ${pad("catalog", 8)}  ${pad("registry", 8)}  manifest`);
  for (const row of rows) {
    if (row.error) {
      console.log(`${pad(row.entry.name, nameWidth)}  ${pad("UNRESOLVED", 10)}  ${pad(row.entry.size_gb + " GB", 8)}  ${pad("-", 8)}  ${row.error}`);
      continue;
    }
    console.log(
      `${pad(row.entry.name, nameWidth)}  ${pad(row.drift ? "DRIFT" : "ok", 10)}  ${pad(row.entry.size_gb + " GB", 8)}  ${pad(row.registryGb + " GB", 8)}  ${row.digest.slice(0, 12)}`,
    );
  }
  for (const row of rows) {
    for (const problem of row.evidenceProblems ?? []) console.log(`EVIDENCE ${row.entry.name}: ${problem}`);
  }
  console.log(
    `\n${rows.length - unresolved}/${rows.length} resolved, ${drifted} with size drift (> ${DRIFT_TOLERANCE_GB} GB), ${evidenceProblems} with release/license evidence problems, ${unresolved} unresolved.`,
  );

  if (update) {
    const next = rows.map((row) => {
      if (row.error) return row.entry;
      const fixed = { ...row.entry, size_gb: row.registryGb, verified_at: today };
      // Evidence the source confirms is written back; a claim that cannot be checked stays as it is.
      if (row.evidence?.released && fixed.released) fixed.released = row.evidence.released;
      if (row.evidence?.license && fixed.license) fixed.license = row.evidence.license;
      return orderedEntry(fixed);
    });
    writeFileSync(catalogPath, `${JSON.stringify(next, null, 2)}\n`, "utf8");
    console.log(`Updated size_gb + verified_at (${today}) for ${rows.length - unresolved} entries in ${catalogPath}.`);
  } else if (drifted > 0) {
    console.log("Run with --update to write the registry sizes back.");
  }

  process.exit(unresolved > 0 || (evidenceProblems > 0 && !update) ? 1 : 0);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error);
    process.exit(2);
  });
}
