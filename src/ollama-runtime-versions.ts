import type { ZebraAction, ZebraChip, ZebraColumn, ZebraRow } from "./ollama-zebra-list";
import type { OllamaRuntimeVersionInfo } from "./types";

/** The pinned, recommended runtime (same value as `DEFAULT_RUNTIME_VERSION` in ollama_runtime.rs). */
export const DEFAULT_RUNTIME_VERSION = "0.40.13";

/** Targets older releases wrote as their default: not a user choice, so they move to the recommended one. */
export const LEGACY_RUNTIME_TARGET_VERSIONS: readonly string[] = ["0.20.2"];

/** Trimmed target version, with a legacy default replaced by the recommended runtime (never a downgrade offer). */
export function normalizeRuntimeTargetVersion(value: string | null | undefined): string {
  const target = (value ?? "").trim();
  return LEGACY_RUNTIME_TARGET_VERSIONS.includes(target) ? DEFAULT_RUNTIME_VERSION : target;
}

/** Fields of `OllamaSettings` that decide what the runtime target means. */
export type RuntimeTargetFields = {
  runtime_source?: string;
  runtime_version?: string;
  runtime_target_version: string;
  /** The user picked the target in the version list (see `runtime_target_explicit` in Rust). */
  runtime_target_explicit?: boolean;
};

/**
 * A target nobody chose that is older than the runtime in use moves up to that runtime, so the UI
 * never offers a downgrade the user did not ask for. Twin of `heal_stale_runtime_target` in
 * `ai_fallback/models.rs` (managed runtimes only). True when the target changed.
 */
export function healStaleRuntimeTarget(
  ollama: RuntimeTargetFields,
  runningVersion?: string | null
): boolean {
  if (ollama.runtime_target_explicit || ollama.runtime_source !== "per_user_zip") return false;
  const running = (runningVersion || ollama.runtime_version || "").trim().replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+$/.test(running)) return false;
  const order = compareVersions(ollama.runtime_target_version, running);
  if (order === null || order >= 0) return false;
  ollama.runtime_target_version = running;
  return true;
}

/**
 * Brings the target into shape: a legacy default (unless picked), an empty value (follows the
 * installed runtime, else the recommended one) and a stale implicit target (see above).
 */
export function syncRuntimeTarget(ollama: RuntimeTargetFields, runningVersion?: string | null): void {
  const trimmed = (ollama.runtime_target_version ?? "").trim();
  if (!trimmed) ollama.runtime_target_explicit = false;
  ollama.runtime_target_version = ollama.runtime_target_explicit
    ? trimmed
    : normalizeRuntimeTargetVersion(trimmed);
  if (!ollama.runtime_target_version) {
    ollama.runtime_target_version = (ollama.runtime_version ?? "").trim() || DEFAULT_RUNTIME_VERSION;
  }
  healStaleRuntimeTarget(ollama, runningVersion);
}

/** "18 Oct 2026" from an ISO timestamp; empty string when missing or invalid. */
export function formatReleaseDate(iso?: string | null): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}

/** Decimal GB/MB like the registries show them (not GiB). */
export function formatDownloadSize(bytes?: number | null): string {
  if (!bytes || bytes <= 0) return "";
  const gb = bytes / 1e9;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${Math.round(bytes / 1e6)} MB`;
}

export type RuntimeVersionActionId = "install" | "switch" | "delete";

/** Dispatched on the list host when a row button is used: `detail` is `{ version, action }`. */
export const RUNTIME_VERSION_ACTION_EVENT = "runtime-version-action";

export type RuntimeVersionRowsInput = {
  catalog: readonly OllamaRuntimeVersionInfo[];
  /** Version the runtime reports right now ("" / "unknown" / null: none). */
  runningVersion: string | null | undefined;
  /** `runtime_target_version`: what the main button above the list applies. */
  targetVersion: string;
  /** `runtime_version` of the settings: the binary the app last started. */
  recordedVersion?: string | null;
  /** A runtime operation runs: every action stays visible but inert. */
  busy: boolean;
};

const RUNTIME_ZEBRA_BUSY_TITLE = "Another runtime operation is running.";

export const RUNTIME_VERSION_COLUMNS: ZebraColumn[] = [
  { id: "version", label: "Version", width: "9rem" },
  { id: "released", label: "Published", width: "7.5rem" },
  { id: "size", label: "Size", width: "5rem", align: "end" },
  { id: "status", label: "Status" },
];

function cleanVersion(version: string | null | undefined): string {
  const text = (version ?? "").trim().replace(/^v/, "");
  return text === "unknown" ? "" : text;
}

function compareVersionsDesc(left: string, right: string): number {
  const order = compareVersions(left, right);
  if (order !== null) return -order;
  return right.localeCompare(left, undefined, { numeric: true });
}

/**
 * Rows of the runtime version list (Zebra list, see `ollama-zebra-list.ts`): the running version
 * first, then newest first. Per row only what makes sense:
 * Install (not installed, verified download), Switch to (installed, not running), Delete
 * (installed, not running, not the selected target nor the recorded binary); the running row
 * carries just the "Running" marker. The running and the selected version are listed even when the
 * catalog does not know them, so the list never hides what the main button talks about.
 */
export function buildRuntimeVersionRows(input: RuntimeVersionRowsInput): ZebraRow[] {
  const running = cleanVersion(input.runningVersion);
  const target = cleanVersion(input.targetVersion);
  const recorded = cleanVersion(input.recordedVersion);

  const pool = new Map<string, OllamaRuntimeVersionInfo>();
  for (const entry of input.catalog) {
    const version = cleanVersion(entry.version);
    if (version && !pool.has(version)) pool.set(version, { ...entry, version });
  }
  const addUnknown = (version: string, installed: boolean) => {
    if (!version || pool.has(version)) return;
    pool.set(version, {
      version,
      source: "online",
      selected: version === target,
      installed,
      recommended: version === DEFAULT_RUNTIME_VERSION,
      prerelease: /(?:-rc|-alpha|-beta)/i.test(version),
      installable: false,
      installable_reason: installed
        ? null
        : "Not in the verified runtime catalog. Use \"Get versions\" to check it.",
    });
  };
  addUnknown(running, true);
  addUnknown(target, false);

  const entries = Array.from(pool.values()).sort((a, b) => {
    if (a.version === running) return -1;
    if (b.version === running) return 1;
    return compareVersionsDesc(a.version, b.version);
  });

  return entries.map((entry): ZebraRow => {
    const version = entry.version;
    const isRunning = version === running;
    const isTarget = version === target;
    const installed = entry.installed || isRunning;

    const chips: ZebraChip[] = [];
    if (installed && !isRunning) chips.push({ label: "Installed", tone: "neutral" });
    if (entry.recommended) chips.push({ label: "Recommended", tone: "accent", title: "Pinned and checked by this app" });
    if (entry.latest) chips.push({ label: "Latest", tone: "ok", title: "Newest stable release found" });
    if (entry.prerelease) chips.push({ label: "Prerelease", tone: "warn" });
    if (isTarget && !isRunning) {
      chips.push({ label: "Target", tone: "info", title: "Selected: the main button above applies it" });
    }

    const actions: ZebraAction[] = [];
    let marker: ZebraChip | undefined;
    const busyTitle = input.busy ? RUNTIME_ZEBRA_BUSY_TITLE : undefined;
    if (isRunning) {
      marker = { label: "Running", tone: "ok" };
    } else if (installed) {
      actions.push({
        id: "switch",
        label: "Switch to",
        title: busyTitle ?? `Start ${version} (already on disk, no download)`,
        tone: "primary",
        disabled: input.busy,
      });
      if (!isTarget && version !== recorded) {
        actions.push({
          id: "delete",
          label: "Delete",
          title: busyTitle ?? `Delete the installed runtime ${version}. It can be installed again later.`,
          tone: "danger",
          disabled: input.busy,
        });
      }
    } else if (entry.installable) {
      const size = formatDownloadSize(entry.size_bytes);
      actions.push({
        id: "install",
        label: "Install",
        title: busyTitle ?? `Download${size ? ` (${size})` : ""}, verify and install ${version}, then start it`,
        tone: "primary",
        disabled: input.busy,
      });
    } else {
      marker = {
        label: "Not installable",
        tone: "neutral",
        title: entry.installable_reason?.trim() || "No verified download for this version.",
      };
    }

    return {
      id: version,
      label: version,
      cells: {
        version,
        released: formatReleaseDate(entry.released_at) || "—",
        size: formatDownloadSize(entry.size_bytes) || "—",
        status: chips,
      },
      actions,
      marker,
      current: isRunning,
    };
  });
}

/** Numeric x.y.z of a version string ("v0.40.13", "0.40.7-rc1" count as 0.40.13 / 0.40.7); null otherwise. */
export function parseVersionTriple(version: string | null | undefined): [number, number, number] | null {
  const match = /^v?(\d+)\.(\d+)\.(\d+)(?:[-+][0-9A-Za-z.-]+)?$/.exec((version ?? "").trim());
  if (!match) return null;
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

/** -1 / 0 / 1, or null when either side is not a strict x.y.z version. */
export function compareVersions(left: string | null | undefined, right: string | null | undefined): number | null {
  const a = parseVersionTriple(left);
  const b = parseVersionTriple(right);
  if (!a || !b) return null;
  for (let i = 0; i < 3; i += 1) {
    if (a[i] !== b[i]) return a[i] > b[i] ? 1 : -1;
  }
  return 0;
}

/**
 * Runtime that satisfies "needs Ollama >= requires": only verified entries (verified download or
 * already on disk), no prereleases. The recommended/pinned entry wins (smallest one if there are
 * several), otherwise the newest. Null when the catalog has nothing that new.
 */
export function pickRuntimeVersionForRequirement(
  catalog: readonly OllamaRuntimeVersionInfo[],
  requires: string | null | undefined
): OllamaRuntimeVersionInfo | null {
  const candidates = catalog
    .filter((entry) => (entry.installable || entry.installed) && !entry.prerelease)
    .filter((entry) => (requires ? (compareVersions(entry.version, requires) ?? -1) >= 0 : true));
  if (candidates.length === 0) return null;
  const byVersion = (a: OllamaRuntimeVersionInfo, b: OllamaRuntimeVersionInfo) =>
    compareVersions(a.version, b.version) ?? 0;
  const preferred = candidates.filter((entry) => entry.recommended || entry.source === "pinned");
  if (preferred.length > 0) return [...preferred].sort(byVersion)[0];
  return [...candidates].sort(byVersion)[candidates.length - 1];
}
