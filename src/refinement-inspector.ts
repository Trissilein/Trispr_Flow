import * as dom from "./dom-refs";
import type {
  HistoryEntry,
  TranscriptionRefinedEvent,
  TranscriptionRefinementFailedEvent,
  TranscriptionRefinementStartedEvent,
  TranscriptionResultEvent,
} from "./types";

export type InspectorStatus = "idle" | "refining" | "refined" | "error";

export type InspectorSnapshot = {
  jobId?: string;
  entryId?: string;
  source: string;
  raw: string;
  refined?: string;
  model?: string;
  executionTimeMs?: number;
  error?: string;
  status: InspectorStatus;
};

const snapshotsByEntryId = new Map<string, InspectorSnapshot>();
const snapshotsByJobId = new Map<string, InspectorSnapshot>();
let latest: InspectorSnapshot | null = null;
let focusedEntryId: string | null = null;
const REFINEMENT_SNAPSHOTS_STORAGE_KEY = "trispr_refinement_snapshots_v1";

type PersistedSnapshotState = {
  latest_entry_id?: string;
  snapshots: Array<InspectorSnapshot & { entryId: string }>;
};

function normalizePersistedStatus(value: unknown): InspectorStatus {
  if (value === "idle" || value === "refining" || value === "refined" || value === "error") {
    return value;
  }
  return "idle";
}

function snapshotFromHistoryEntry(entry: HistoryEntry): InspectorSnapshot | null {
  const refinement = entry.refinement;
  if (!refinement) return null;
  const status = normalizePersistedStatus(refinement.status);
  if (
    status === "idle"
    && !(refinement.raw ?? "").trim()
    && !(refinement.refined ?? "").trim()
    && !(refinement.error ?? "").trim()
  ) {
    return null;
  }
  const snapshot: InspectorSnapshot = {
    entryId: entry.id,
    jobId: (refinement.job_id ?? "").trim() || undefined,
    source: entry.source,
    raw: (refinement.raw ?? "").trim() ? refinement.raw : entry.text,
    refined: (refinement.refined ?? "").trim() ? refinement.refined : undefined,
    model: (refinement.model ?? "").trim() ? refinement.model : undefined,
    executionTimeMs:
      typeof refinement.execution_time_ms === "number"
      && Number.isFinite(refinement.execution_time_ms)
        ? refinement.execution_time_ms
        : undefined,
    error: (refinement.error ?? "").trim() ? refinement.error : undefined,
    status,
  };
  if (snapshot.status === "refining") {
    snapshot.status = "error";
    if (!snapshot.error) {
      snapshot.error = "Refinement reset (app restart)";
    }
  }
  return snapshot;
}

function persistSnapshotStateToStorage(): void {
  try {
    const snapshots = Array.from(snapshotsByEntryId.entries()).map(([entryId, snapshot]) => ({
      ...snapshot,
      entryId,
    }));
    if (snapshots.length === 0) {
      window.localStorage.removeItem(REFINEMENT_SNAPSHOTS_STORAGE_KEY);
      return;
    }
    const payload: PersistedSnapshotState = {
      latest_entry_id: latest?.entryId,
      snapshots,
    };
    window.localStorage.setItem(REFINEMENT_SNAPSHOTS_STORAGE_KEY, JSON.stringify(payload));
  } catch {
    // ignore localStorage failures
  }
}

function restoreSnapshotStateFromStorage(entryIds: Set<string>): void {
  snapshotsByEntryId.clear();
  snapshotsByJobId.clear();
  latest = null;
  focusedEntryId = null;

  try {
    const raw = window.localStorage.getItem(REFINEMENT_SNAPSHOTS_STORAGE_KEY);
    if (!raw) return;
    const parsed = JSON.parse(raw) as Partial<PersistedSnapshotState>;
    const records = Array.isArray(parsed.snapshots) ? parsed.snapshots : [];

    for (const record of records) {
      const entryId = typeof record.entryId === "string" ? record.entryId.trim() : "";
      if (!entryId || !entryIds.has(entryId)) continue;

      const status = normalizePersistedStatus(record.status);
      const snapshot: InspectorSnapshot = {
        entryId,
        jobId: typeof record.jobId === "string" && record.jobId.trim() ? record.jobId.trim() : undefined,
        source: typeof record.source === "string" && record.source.trim() ? record.source : "mic",
        raw: typeof record.raw === "string" ? record.raw : "",
        refined: typeof record.refined === "string" ? record.refined : undefined,
        model: typeof record.model === "string" ? record.model : undefined,
        executionTimeMs:
          typeof record.executionTimeMs === "number" && Number.isFinite(record.executionTimeMs)
            ? record.executionTimeMs
            : undefined,
        error: typeof record.error === "string" ? record.error : undefined,
        status: status === "refining" ? "error" : status,
      };
      if (status === "refining" && !snapshot.error) {
        snapshot.error = "Refinement reset (app restart)";
      }

      snapshotsByEntryId.set(entryId, snapshot);
      if (snapshot.jobId) {
        snapshotsByJobId.set(snapshot.jobId, { ...snapshot });
      }
    }

    const preferredLatestId =
      typeof parsed.latest_entry_id === "string" ? parsed.latest_entry_id.trim() : "";
    if (preferredLatestId && snapshotsByEntryId.has(preferredLatestId)) {
      latest = snapshotsByEntryId.get(preferredLatestId) ?? null;
    } else {
      for (const id of entryIds) {
        if (snapshotsByEntryId.has(id)) {
          latest = snapshotsByEntryId.get(id) ?? null;
          break;
        }
      }
    }
  } catch {
    snapshotsByEntryId.clear();
    snapshotsByJobId.clear();
    latest = null;
    focusedEntryId = null;
  }
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export type RefinementDiffKind = "same" | "added" | "removed";

export type RefinementDiffToken = {
  kind: RefinementDiffKind;
  token: string;
};

function tokenizeWords(text: string): string[] {
  return text
    .trim()
    .split(/\s+/)
    .filter((part) => part.length > 0);
}

export function buildRefinementWordDiff(raw: string, refined: string): RefinementDiffToken[] {
  const a = tokenizeWords(raw);
  const b = tokenizeWords(refined);

  if (a.length === 0 && b.length === 0) return [];
  if (a.length === 0) return b.map((token) => ({ kind: "added", token }));
  if (b.length === 0) return a.map((token) => ({ kind: "removed", token }));

  const rows = a.length + 1;
  const cols = b.length + 1;
  const dp: number[][] = Array.from({ length: rows }, () => Array<number>(cols).fill(0));

  for (let i = 1; i < rows; i += 1) {
    for (let j = 1; j < cols; j += 1) {
      if (a[i - 1] === b[j - 1]) {
        dp[i][j] = dp[i - 1][j - 1] + 1;
      } else {
        dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
      }
    }
  }

  const out: RefinementDiffToken[] = [];
  let i = a.length;
  let j = b.length;

  while (i > 0 && j > 0) {
    if (a[i - 1] === b[j - 1]) {
      out.push({ kind: "same", token: a[i - 1] });
      i -= 1;
      j -= 1;
    } else if (dp[i - 1][j] >= dp[i][j - 1]) {
      out.push({ kind: "removed", token: a[i - 1] });
      i -= 1;
    } else {
      out.push({ kind: "added", token: b[j - 1] });
      j -= 1;
    }
  }

  while (i > 0) {
    out.push({ kind: "removed", token: a[i - 1] });
    i -= 1;
  }
  while (j > 0) {
    out.push({ kind: "added", token: b[j - 1] });
    j -= 1;
  }

  return out.reverse();
}

function renderWordDiff(raw: string, refined: string): string {
  const diff = buildRefinementWordDiff(raw, refined);
  if (diff.length === 0) {
    return '<span class="refinement-diff-token refinement-diff-token--same">No text to diff.</span>';
  }

  return diff
    .map((part) => {
      const cls =
        part.kind === "added"
          ? "refinement-diff-token refinement-diff-token--added"
          : part.kind === "removed"
            ? "refinement-diff-token refinement-diff-token--removed"
            : "refinement-diff-token refinement-diff-token--same";
      return `<span class="${cls}">${escapeHtml(part.token)}</span>`;
    })
    .join(" ");
}

function snapshotFromLatestOrEvent(
  jobId: string | undefined,
  entryId: string | undefined,
  source: string
): InspectorSnapshot | null {
  if (jobId && snapshotsByJobId.has(jobId)) {
    return snapshotsByJobId.get(jobId) ?? null;
  }
  if (entryId && snapshotsByEntryId.has(entryId)) {
    return snapshotsByEntryId.get(entryId) ?? null;
  }
  if (!latest) return null;
  if (latest.jobId && jobId) {
    return latest.jobId === jobId ? latest : null;
  }
  if (latest.jobId && !jobId) {
    return null;
  }
  if (latest.entryId && entryId) {
    return latest.entryId === entryId ? latest : null;
  }
  if (latest.entryId && !entryId) {
    return null;
  }
  return latest.source === source ? latest : null;
}

function getDisplaySnapshot(): InspectorSnapshot | null {
  if (focusedEntryId) {
    return snapshotsByEntryId.get(focusedEntryId) ?? latest;
  }
  return latest;
}

function updateStatusBadge(status: InspectorStatus): void {
  if (!dom.refinementInspectorStatus) return;

  dom.refinementInspectorStatus.classList.remove(
    "model-status--available",
    "model-status--downloaded",
    "model-status--active",
    "is-error"
  );

  if (status === "refining") {
    dom.refinementInspectorStatus.classList.add("model-status--downloaded");
    dom.refinementInspectorStatus.textContent = "Refining";
    return;
  }

  if (status === "refined") {
    dom.refinementInspectorStatus.classList.add("model-status--active");
    dom.refinementInspectorStatus.textContent = "Refined";
    return;
  }

  if (status === "error") {
    dom.refinementInspectorStatus.classList.add("model-status--available", "is-error");
    dom.refinementInspectorStatus.textContent = "Failed";
    return;
  }

  dom.refinementInspectorStatus.classList.add("model-status--available");
  dom.refinementInspectorStatus.textContent = "Idle";
}

function renderLatestInspector(): void {
  if (
    !dom.refinementInspectorEmpty ||
    !dom.refinementInspectorContent ||
    !dom.refinementInspectorMeta ||
    !dom.refinementInspectorRaw ||
    !dom.refinementInspectorRefined ||
    !dom.refinementInspectorDiff ||
    !dom.refinementInspectorError
  ) {
    return;
  }

  const snapshot = getDisplaySnapshot();
  if (!snapshot) {
    dom.refinementInspectorEmpty.style.display = "block";
    dom.refinementInspectorContent.style.display = "none";
    return;
  }

  dom.refinementInspectorEmpty.style.display = "none";
  dom.refinementInspectorContent.style.display = "grid";

  updateStatusBadge(snapshot.status);

  const sourceLabel = snapshot.source === "output" ? "System audio" : snapshot.source;
  const modelPart = snapshot.model ? ` • ${snapshot.model}` : "";
  const timePart =
    typeof snapshot.executionTimeMs === "number" ? ` • ${snapshot.executionTimeMs} ms` : "";
  dom.refinementInspectorMeta.textContent = `${sourceLabel}${modelPart}${timePart}`;

  dom.refinementInspectorRaw.textContent = snapshot.raw || "—";
  dom.refinementInspectorRefined.textContent = snapshot.refined || "—";

  if (snapshot.refined && snapshot.raw) {
    dom.refinementInspectorDiff.innerHTML = renderWordDiff(snapshot.raw, snapshot.refined);
  } else {
    dom.refinementInspectorDiff.innerHTML =
      '<span class="refinement-diff-token refinement-diff-token--same">No refined output yet.</span>';
  }

  if (snapshot.status === "error" && snapshot.error) {
    dom.refinementInspectorError.style.display = "block";
    dom.refinementInspectorError.textContent = snapshot.error;
  } else {
    dom.refinementInspectorError.style.display = "none";
    dom.refinementInspectorError.textContent = "";
  }
}

function storeSnapshot(snapshot: InspectorSnapshot): void {
  latest = snapshot;
  if (snapshot.jobId) {
    snapshotsByJobId.set(snapshot.jobId, { ...snapshot });
  }
  if (snapshot.entryId) {
    snapshotsByEntryId.set(snapshot.entryId, { ...snapshot });
    persistSnapshotStateToStorage();
  }
}

export function getRefinementSnapshot(entryId: string): InspectorSnapshot | null {
  return snapshotsByEntryId.get(entryId) ?? null;
}

export function setInspectorFocus(entryId: string): void {
  focusedEntryId = entryId;
  const snapshot = snapshotsByEntryId.get(entryId);
  if (snapshot) {
    latest = snapshot;
  }
  if (dom.refinementInspector) {
    dom.refinementInspector.open = true;
  }
  renderLatestInspector();
}

export function clearInspectorFocus(): void {
  focusedEntryId = null;
  renderLatestInspector();
}

export function resetRefinementInspector(): void {
  latest = null;
  focusedEntryId = null;
  snapshotsByJobId.clear();
  snapshotsByEntryId.clear();
  persistSnapshotStateToStorage();
  renderLatestInspector();
}

export function restoreRefinementInspector(entries: HistoryEntry[]): void {
  restoreSnapshotStateFromStorage(new Set(entries.map((entry) => entry.id)));

  const sorted = [...entries].sort((a, b) => b.timestamp_ms - a.timestamp_ms);
  let latestFromHistory: InspectorSnapshot | null = null;
  for (const entry of sorted) {
    const snapshot = snapshotFromHistoryEntry(entry);
    if (!snapshot) continue;
    snapshotsByEntryId.set(entry.id, snapshot);
    if (snapshot.jobId) {
      snapshotsByJobId.set(snapshot.jobId, { ...snapshot });
    }
    if (!latestFromHistory) {
      latestFromHistory = snapshot;
    }
  }
  if (latestFromHistory) {
    latest = latestFromHistory;
  }

  persistSnapshotStateToStorage();
  renderLatestInspector();
}

export function handleTranscriptionResultForInspector(event: TranscriptionResultEvent): void {
  focusedEntryId = null;
  storeSnapshot({
    jobId: event.job_id,
    entryId: event.entry_id,
    source: event.source,
    raw: event.text,
    status: "idle",
  });
  renderLatestInspector();
}

export function handleRefinementStartedForInspector(
  event: TranscriptionRefinementStartedEvent
): void {
  const existing = snapshotFromLatestOrEvent(event.job_id, event.entry_id, event.source);
  const snapshot: InspectorSnapshot = {
    jobId: event.job_id,
    entryId: event.entry_id,
    source: event.source,
    raw: event.original || existing?.raw || "",
    refined: existing?.refined,
    model: existing?.model,
    executionTimeMs: existing?.executionTimeMs,
    status: "refining",
  };
  storeSnapshot(snapshot);
  renderLatestInspector();
}

export function handleRefinementSuccessForInspector(
  event: TranscriptionRefinedEvent
): void {
  const existing = snapshotFromLatestOrEvent(event.job_id, event.entry_id, event.source);
  const snapshot: InspectorSnapshot = {
    jobId: event.job_id,
    entryId: event.entry_id,
    source: event.source,
    raw: event.original || existing?.raw || "",
    refined: event.refined,
    model: event.model,
    executionTimeMs: event.execution_time_ms,
    status: "refined",
  };
  storeSnapshot(snapshot);
  renderLatestInspector();
}

export function handleRefinementFailureForInspector(
  event: TranscriptionRefinementFailedEvent
): void {
  const existing = snapshotFromLatestOrEvent(event.job_id, event.entry_id, event.source);
  const snapshot: InspectorSnapshot = {
    jobId: event.job_id,
    entryId: event.entry_id,
    source: event.source,
    raw: event.original || existing?.raw || "",
    refined: existing?.refined,
    model: existing?.model,
    executionTimeMs: existing?.executionTimeMs,
    error: event.error,
    status: "error",
  };
  storeSnapshot(snapshot);
  renderLatestInspector();
}

export function markAllPendingAsFailed(reason: string): void {
  let changed = false;
  for (const snapshot of snapshotsByEntryId.values()) {
    if (snapshot.status === "refining") {
      snapshot.status = "error";
      snapshot.error = `Refinement reset (${reason})`;
      changed = true;
    }
  }
  if (latest?.status === "refining") {
    latest.status = "error";
    latest.error = `Refinement reset (${reason})`;
    changed = true;
  }
  if (changed) {
    persistSnapshotStateToStorage();
    renderLatestInspector();
  }
}

export function renderRefinementInspector(): void {
  renderLatestInspector();
}
