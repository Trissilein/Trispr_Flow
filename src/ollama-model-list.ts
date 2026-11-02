/**
 * The model list of the Models section: one zebra list of collapsible rows.
 *
 * A collapsed row is compact (name, source, size, status chips, actions); a click on the row, or
 * Enter/Space on its toggle, opens a panel with what is known about the model. The panel's data
 * comes from `loadDetails` (the backend's `get_ollama_model_details`), asked for on the FIRST
 * expansion of a row only and kept per model afterwards. The caller decides what a row offers
 * (`actions`, `chips`, `hints`): this module only draws, keeps the open/closed state across
 * re-renders and keeps keyboard focus where it was.
 *
 * Every text is set with `textContent`: model names, descriptions and licenses come from the
 * network or from other people's model files.
 */
import { formatReleaseDate } from "./ollama-runtime-versions";

export type ModelListChip = {
  label: string;
  /** Full class list, e.g. `qwen-card-badge qwen-card-badge--active`. */
  className: string;
  title?: string;
};

export type ModelListAction = {
  /** `data-action` value the click handler dispatches on. */
  action: string;
  label: string;
  className: string;
  title?: string;
  disabled?: boolean;
  /** Extra `data-*` attributes (`target`, `version`, ...). `model` is always the row's name. */
  data?: Record<string, string>;
};

export type ModelListHint = { text: string; className: string };

export type ModelListLink = { label: string; url: string };

/** Facts the row already knows without asking anybody (inventory, catalog, runtime gate). */
export type ModelListInfo = {
  installed: boolean;
  description?: string;
  catalogNote?: string;
  /** First public release `YYYY-MM-DD`, with the Hugging Face repo that proves it. */
  released?: string;
  releasedFrom?: string;
  license?: string;
  /** `modified_at` of the installed model: when it was downloaded. */
  downloadedAt?: string;
  digest?: string;
  /** "6.6 GB": size on disk when installed, download size otherwise. */
  sizeText?: string;
  memoryText?: string;
  parameterSize?: string;
  quantization?: string;
  family?: string;
  /** "Needs Ollama 0.30.9 or newer (running 0.40.13)". */
  requiresText?: string;
  verifiedText?: string;
  links: ModelListLink[];
  /** The details are fetched again when this changes (a new build was pulled, the model was installed). */
  detailSignature: string;
};

export type ModelListRow = {
  /** Normalised name, unique in the list. */
  key: string;
  /** Exact pull reference. */
  name: string;
  title: string;
  /** `data-state`: active | installed | downloading | available | missing. */
  state: string;
  sourceLabel: string;
  sizeLabel: string;
  chips: ModelListChip[];
  actions: ModelListAction[];
  hints: ModelListHint[];
  progress?: { percent: number | null; text: string };
  info: ModelListInfo;
};

export type ModelListHooks = {
  onAction: (action: string, row: ModelListRow, data: DOMStringMap) => void;
  /** Asked for on the first expansion of a row. Without it the panel shows the known facts only. */
  loadDetails?: (name: string) => Promise<unknown>;
  /** Opens one of `info.links` outside the app. */
  openLink?: (url: string) => void;
  now?: () => number;
};

// ───────────────────────────── details coming from the backend ─────────────────────────────

export type ModelDetailsView = {
  local: {
    license?: string;
    capabilities: string[];
    family?: string;
    parameterSize?: string;
    quantization?: string;
    format?: string;
    contextLength?: number;
    architecture?: string;
    parameterCount?: number;
    baseModel?: string;
    requires?: string;
    modifiedAt?: string;
  } | null;
  localError?: string;
  remote: {
    source: "huggingface" | "ollama";
    releasedAt?: string;
    updatedAt?: string;
    downloads?: number;
    downloadsText?: string;
    likes?: number;
    license?: string;
    pipelineTag?: string;
    languages: string[];
    baseModel?: string;
    description?: string;
    gated?: boolean;
  } | null;
  remoteNote?: string;
  remoteCached: boolean;
  remoteStale: boolean;
  remoteFetchedAtMs?: number;
};

function text(value: unknown, max = 600): string | undefined {
  if (typeof value !== "string") return undefined;
  const flat = value.replace(/\s+/g, " ").trim();
  return flat ? flat.slice(0, max) : undefined;
}

function count(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : undefined;
}

function textList(value: unknown, maxItems: number, maxChars: number): string[] {
  if (!Array.isArray(value)) return [];
  const out: string[] = [];
  for (const item of value) {
    const entry = text(item, maxChars);
    if (entry && !out.includes(entry) && out.length < maxItems) out.push(entry);
  }
  return out;
}

/** The backend's answer, reduced to plain values; anything unexpected is simply missing. */
export function sanitizeModelDetails(raw: unknown): ModelDetailsView {
  const root = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  const localRaw = root.local && typeof root.local === "object" ? (root.local as Record<string, unknown>) : null;
  const remoteRaw = root.remote && typeof root.remote === "object" ? (root.remote as Record<string, unknown>) : null;
  const view: ModelDetailsView = {
    local: null,
    remote: null,
    remoteCached: root.remote_cached === true,
    remoteStale: root.remote_stale === true,
  };
  if (localRaw) {
    view.local = {
      license: text(localRaw.license, 80),
      capabilities: textList(localRaw.capabilities, 12, 24),
      family: text(localRaw.family, 40),
      parameterSize: text(localRaw.parameter_size, 40),
      quantization: text(localRaw.quantization_level, 40),
      format: text(localRaw.format, 40),
      contextLength: count(localRaw.context_length),
      architecture: text(localRaw.architecture, 40),
      parameterCount: count(localRaw.parameter_count),
      baseModel: text(localRaw.base_model, 120),
      requires: text(localRaw.requires, 32),
      modifiedAt: text(localRaw.modified_at, 40),
    };
  }
  const localError = text(root.local_error, 300);
  if (localError) view.localError = localError;
  if (remoteRaw) {
    view.remote = {
      source: remoteRaw.source === "huggingface" ? "huggingface" : "ollama",
      releasedAt: text(remoteRaw.released_at, 40),
      updatedAt: text(remoteRaw.updated_at, 40),
      downloads: count(remoteRaw.downloads),
      downloadsText: text(remoteRaw.downloads_text, 16),
      likes: count(remoteRaw.likes),
      license: text(remoteRaw.license, 80),
      pipelineTag: text(remoteRaw.pipeline_tag, 40),
      languages: textList(remoteRaw.languages, 8, 12),
      baseModel: text(remoteRaw.base_model, 120),
      description: text(remoteRaw.description, 500),
      gated: typeof remoteRaw.gated === "boolean" ? remoteRaw.gated : undefined,
    };
  }
  const note = text(root.remote_note, 300);
  if (note) view.remoteNote = note;
  const fetchedAt = count(root.remote_fetched_at_ms);
  if (fetchedAt !== undefined) view.remoteFetchedAtMs = fetchedAt;
  return view;
}

// ───────────────────────────── formatting ─────────────────────────────

const NUMBER_FORMAT = new Intl.NumberFormat("en-US");

export function formatCount(value: number): string {
  return NUMBER_FORMAT.format(Math.round(value));
}

/** 131072 -> "131,072 tokens (128K)". */
export function formatContextLength(tokens: number): string {
  const exact = `${formatCount(tokens)} tokens`;
  return tokens >= 1024 && tokens % 1024 === 0 ? `${exact} (${tokens / 1024}K)` : exact;
}

/** 7518069290 -> "7.5B". */
export function formatParameterCount(parameters: number): string {
  if (parameters >= 1e9) return `${(parameters / 1e9).toFixed(1)}B`;
  if (parameters >= 1e6) return `${Math.round(parameters / 1e6)}M`;
  return formatCount(parameters);
}

function formatDateTime(iso: string | undefined): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function isoDateOnly(date: string): string {
  // A date-only value is pinned to noon UTC so no timezone shows the previous day.
  return /^\d{4}-\d{2}-\d{2}$/.test(date) ? `${date}T12:00:00Z` : date;
}

// ───────────────────────────── state that outlives a render ─────────────────────────────

const expandedKeys = new Set<string>();
type DetailsEntry = {
  signature: string;
  state: "loading" | "ready" | "error";
  view?: ModelDetailsView;
  error?: string;
  at: number;
};
const detailsCache = new Map<string, DetailsEntry>();
/** A failed lookup is tried again on the next render of the open row after this long. */
const DETAILS_RETRY_MS = 30_000;
/** The details of one model are not asked for again within this time unless the model changed. */
const DETAILS_FRESH_MS = 10 * 60_000;
const MAX_CACHED_DETAILS = 64;

let current: { list: HTMLElement; rows: ModelListRow[]; hooks: ModelListHooks } | null = null;

/** Forgets which rows are open and every loaded detail (tests, and a clean start). */
export function resetModelListState(): void {
  expandedKeys.clear();
  detailsCache.clear();
  current = null;
}

export function isModelRowExpanded(key: string): boolean {
  return expandedKeys.has(key);
}

export function getCachedModelDetails(key: string): DetailsEntry | undefined {
  return detailsCache.get(key);
}

// ───────────────────────────── DOM helpers ─────────────────────────────

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  content?: string
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (content !== undefined) node.textContent = content;
  return node;
}

function panelId(key: string): string {
  let hash = 0;
  for (let i = 0; i < key.length; i++) hash = (hash * 31 + key.charCodeAt(i)) >>> 0;
  return `ollama-model-details-${hash.toString(36)}`;
}

export type ModelListFocusToken = { key: string; target: string } | null;

/** Where keyboard focus is inside the list: the row and which control of it. */
export function captureModelListFocus(host: ParentNode | null): ModelListFocusToken {
  const active = typeof document !== "undefined" ? document.activeElement : null;
  if (!host || !(active instanceof HTMLElement) || !host.contains(active)) return null;
  const row = active.closest<HTMLElement>(".ollama-model-row");
  const key = row?.dataset.key;
  if (!row || !key) return null;
  if (active.dataset.role === "toggle") return { key, target: "toggle" };
  const action = active.closest<HTMLElement>("[data-action]")?.dataset.action;
  if (action) return { key, target: `action:${action}` };
  const link = active.closest<HTMLElement>("[data-open-url]")?.dataset.openUrl;
  if (link) return { key, target: `link:${link}` };
  return { key, target: "toggle" };
}

export function restoreModelListFocus(host: ParentNode | null, token: ModelListFocusToken): void {
  if (!host || !token) return;
  const rows = Array.from(host.querySelectorAll<HTMLElement>(".ollama-model-row"));
  const row = rows.find((candidate) => candidate.dataset.key === token.key);
  if (!row) return;
  let target: HTMLElement | null = null;
  if (token.target.startsWith("action:")) {
    const action = token.target.slice("action:".length);
    target = Array.from(row.querySelectorAll<HTMLElement>("button[data-action]")).find(
      (button) => button.dataset.action === action && !(button as HTMLButtonElement).disabled
    ) ?? null;
  } else if (token.target.startsWith("link:")) {
    const url = token.target.slice("link:".length);
    target = Array.from(row.querySelectorAll<HTMLElement>("[data-open-url]")).find((link) => link.dataset.openUrl === url) ?? null;
  }
  // The action may be gone (it was just used): the toggle is the next best place.
  target ??= row.querySelector<HTMLElement>('[data-role="toggle"]');
  target?.focus({ preventScroll: true });
}

// ───────────────────────────── rendering ─────────────────────────────

function chipNode(chip: ModelListChip): HTMLElement {
  const node = el("span", chip.className, chip.label);
  if (chip.title) node.title = chip.title;
  return node;
}

function actionNode(row: ModelListRow, action: ModelListAction): HTMLButtonElement {
  const button = el("button", action.className, action.label);
  button.type = "button";
  button.dataset.action = action.action;
  button.dataset.model = row.name;
  for (const [name, value] of Object.entries(action.data ?? {})) button.dataset[name] = value;
  if (action.title) button.title = action.title;
  if (action.disabled) button.disabled = true;
  return button;
}

function progressNode(progress: NonNullable<ModelListRow["progress"]>): HTMLElement {
  const wrap = el("div", "qwen-card-progress");
  const bar = el("div", "qwen-card-progress-bar");
  const fill = el("div", "qwen-card-progress-fill");
  const percent = progress.percent == null ? 0 : Math.max(0, Math.min(100, progress.percent));
  fill.setAttribute("style", `width: ${percent}%`);
  bar.appendChild(fill);
  wrap.append(bar, el("span", "qwen-card-progress-text", progress.text));
  return wrap;
}

type Fact = { label: string; value: string | Node; title?: string };

function factNode(fact: Fact): HTMLElement {
  const wrap = el("div", "ollama-model-fact");
  wrap.append(el("dt", "ollama-model-fact-label", fact.label));
  const value = el("dd", "ollama-model-fact-value");
  if (typeof fact.value === "string") value.textContent = fact.value;
  else value.appendChild(fact.value);
  if (fact.title) value.title = fact.title;
  wrap.appendChild(value);
  return wrap;
}

function capabilityNode(capabilities: string[]): Node {
  const wrap = el("span", "ollama-model-caps");
  for (const capability of capabilities) wrap.appendChild(el("span", "ollama-model-cap", capability));
  return wrap;
}

/** The facts of one row: what the row knows, completed by the backend's details when they arrived. */
export function collectModelFacts(row: ModelListRow, entry: DetailsEntry | undefined): Fact[] {
  const info = row.info;
  const view = entry?.state === "ready" ? entry.view : undefined;
  const local = view?.local ?? null;
  const remote = view?.remote ?? null;
  const facts: Fact[] = [];

  // Released: only with evidence (the catalog's source, or the Hugging Face repository itself).
  if (info.released) {
    const shown = formatReleaseDate(isoDateOnly(info.released));
    if (shown) {
      facts.push({
        label: "Released",
        value: shown,
        title: info.releasedFrom ? `Source: huggingface.co/${info.releasedFrom} (repository created)` : undefined,
      });
    }
  } else if (remote?.source === "huggingface" && remote.releasedAt) {
    const shown = formatReleaseDate(remote.releasedAt);
    if (shown) facts.push({ label: "Released", value: shown, title: "The Hugging Face repository was created on this day." });
  }
  if (remote?.updatedAt) {
    const shown = formatReleaseDate(remote.updatedAt);
    if (shown) {
      facts.push({
        label: remote.source === "huggingface" ? "Updated on Hugging Face" : "Updated on ollama.com",
        value: shown,
      });
    }
  }
  if (info.installed) {
    const downloaded = formatDateTime(info.downloadedAt) || formatDateTime(local?.modifiedAt);
    if (downloaded) facts.push({ label: "Downloaded", value: downloaded });
  }
  if (info.sizeText) facts.push({ label: info.installed ? "Size on disk" : "Download size", value: info.sizeText });
  if (info.memoryText) facts.push({ label: "Memory", value: info.memoryText });
  if (info.digest) {
    const short = info.digest.replace(/^sha256:/i, "");
    facts.push({ label: "Digest", value: short.slice(0, 12), title: short });
  }

  const parameters =
    local?.parameterSize ?? info.parameterSize ?? (local?.parameterCount ? formatParameterCount(local.parameterCount) : undefined);
  if (parameters) {
    facts.push({
      label: "Parameters",
      value: local?.parameterCount && local.parameterSize ? `${parameters} (${formatCount(local.parameterCount)})` : parameters,
    });
  }
  const quantization = local?.quantization ?? info.quantization;
  if (quantization) facts.push({ label: "Quantization", value: quantization });
  if (local?.contextLength) facts.push({ label: "Context length", value: formatContextLength(local.contextLength) });
  const architecture = local?.architecture ?? local?.family ?? info.family;
  if (architecture) {
    facts.push({ label: "Architecture", value: local?.format ? `${architecture} (${local.format})` : architecture });
  }
  if (local && local.capabilities.length > 0) {
    facts.push({ label: "Capabilities", value: capabilityNode(local.capabilities) });
  }
  const license = local?.license ?? remote?.license ?? info.license;
  if (license) facts.push({ label: "License", value: license });
  const baseModel = local?.baseModel ?? remote?.baseModel;
  if (baseModel) facts.push({ label: "Base model", value: baseModel });
  if (info.requiresText) facts.push({ label: "Runtime", value: info.requiresText });
  else if (local?.requires) facts.push({ label: "Runtime", value: `Needs Ollama ${local.requires} or newer` });
  if (info.verifiedText) facts.push({ label: "Verification", value: info.verifiedText });

  if (remote) {
    const popularity: string[] = [];
    if (remote.downloads !== undefined) popularity.push(`${formatCount(remote.downloads)} downloads`);
    else if (remote.downloadsText) popularity.push(`${remote.downloadsText} downloads`);
    if (remote.likes !== undefined) popularity.push(`${formatCount(remote.likes)} likes`);
    if (popularity.length > 0) {
      facts.push({ label: remote.source === "huggingface" ? "Hugging Face" : "Ollama library", value: popularity.join(" · ") });
    }
    if (remote.pipelineTag) facts.push({ label: "Task", value: remote.pipelineTag });
    if (remote.languages.length > 0) facts.push({ label: "Languages", value: remote.languages.join(", ") });
    if (remote.gated) facts.push({ label: "Access", value: "Gated: needs an accepted license on Hugging Face" });
  }
  return facts;
}

function panelNode(row: ModelListRow, entry: DetailsEntry | undefined, hooks: ModelListHooks): HTMLElement {
  const panel = el("div", "ollama-model-panel");
  panel.id = panelId(row.key);
  panel.setAttribute("role", "region");
  panel.setAttribute("aria-label", `Details of ${row.title}`);

  const facts = el("dl", "ollama-model-facts");
  for (const fact of collectModelFacts(row, entry)) facts.appendChild(factNode(fact));
  if (facts.childElementCount > 0) panel.appendChild(facts);

  const view = entry?.state === "ready" ? entry.view : undefined;
  const paragraphs: string[] = [];
  if (row.info.description) paragraphs.push(row.info.description);
  if (row.info.catalogNote) paragraphs.push(row.info.catalogNote);
  const about = view?.remote?.description;
  if (about && !paragraphs.some((paragraph) => paragraph.includes(about) || about.includes(paragraph))) paragraphs.push(about);
  for (const paragraph of paragraphs) panel.appendChild(el("p", "qwen-card-desc ollama-model-about", paragraph));

  const links = row.info.links.filter((link) => /^https:\/\//.test(link.url));
  if (links.length > 0) {
    const bar = el("div", "ollama-model-links");
    for (const link of links) {
      const button = el("button", "ollama-model-link", link.label);
      button.type = "button";
      button.dataset.openUrl = link.url;
      button.title = link.url;
      if (!hooks.openLink) button.disabled = true;
      bar.appendChild(button);
    }
    panel.appendChild(bar);
  }

  // What could not be loaded is a quiet note, never an error.
  const notes: string[] = [];
  if (entry?.state === "loading") notes.push("Loading details…");
  if (entry?.state === "error") notes.push(`More details could not be loaded${entry.error ? ` (${entry.error})` : ""}. Shown: what is known locally.`);
  if (view?.localError && row.info.installed) notes.push(`Local details unavailable: ${view.localError}`);
  if (view?.remoteNote) notes.push(view.remoteNote);
  if (view?.remoteCached && !view.remoteStale && view.remoteFetchedAtMs) {
    const when = formatDateTime(new Date(view.remoteFetchedAtMs).toISOString());
    if (when) notes.push(`Online details saved ${when}.`);
  }
  for (const note of notes) {
    const line = el("div", "qwen-card-hint ollama-model-note", note);
    line.setAttribute("role", "status");
    panel.appendChild(line);
  }
  return panel;
}

function rowNode(row: ModelListRow, hooks: ModelListHooks, index: number): HTMLElement {
  const open = expandedKeys.has(row.key);
  const entry = open ? detailsCache.get(row.key) : undefined;

  const node = el("div", `qwen-model-card ollama-model-row ${index % 2 === 0 ? "is-odd" : "is-even"}`);
  node.dataset.model = row.name;
  node.dataset.key = row.key;
  node.dataset.state = row.state;
  node.dataset.expanded = open ? "true" : "false";
  node.setAttribute("role", "listitem");

  const head = el("div", "ollama-model-row-head");
  const main = el("div", "ollama-model-main");
  const toggle = el("button", "ollama-model-toggle");
  toggle.type = "button";
  toggle.dataset.role = "toggle";
  toggle.setAttribute("aria-expanded", open ? "true" : "false");
  if (open) toggle.setAttribute("aria-controls", panelId(row.key));
  toggle.title = open ? "Hide details" : "Show details";
  toggle.append(el("span", "ollama-model-chevron", "›"), el("span", "qwen-card-name ollama-model-name", row.title));
  const sub = el("div", "ollama-model-sub");
  sub.appendChild(el("code", "qwen-card-tag ollama-model-tag", row.name));
  const chips = el("span", "ollama-model-chips");
  for (const chip of row.chips) chips.appendChild(chipNode(chip));
  if (chips.childElementCount > 0) sub.appendChild(chips);
  main.append(toggle, sub);
  head.append(
    main,
    el("span", "ollama-model-source", row.sourceLabel),
    el("span", "qwen-card-size ollama-model-size", row.sizeLabel)
  );

  const actions = el("span", "ollama-model-actions");
  for (const action of row.actions) actions.appendChild(actionNode(row, action));
  head.appendChild(actions);
  node.appendChild(head);

  for (const hint of row.hints) node.appendChild(el("div", `qwen-card-hint ollama-model-hint ${hint.className}`.trim(), hint.text));
  if (row.progress) node.appendChild(progressNode(row.progress));
  if (open) node.appendChild(panelNode(row, entry, hooks));
  return node;
}

function startDetailsLoad(row: ModelListRow, hooks: ModelListHooks): void {
  const loader = hooks.loadDetails;
  if (!loader) return;
  const now = hooks.now ?? Date.now;
  const signature = row.info.detailSignature;
  const entry = detailsCache.get(row.key);
  if (entry && entry.signature === signature) {
    if (entry.state === "loading") return;
    if (entry.state === "ready" && now() - entry.at < DETAILS_FRESH_MS) return;
    if (entry.state === "error" && now() - entry.at < DETAILS_RETRY_MS) return;
  }
  if (detailsCache.size >= MAX_CACHED_DETAILS && !detailsCache.has(row.key)) {
    const oldest = detailsCache.keys().next().value;
    if (oldest !== undefined) detailsCache.delete(oldest);
  }
  const pending: DetailsEntry = { signature, state: "loading", at: now(), view: entry?.signature === signature ? entry.view : undefined };
  detailsCache.set(row.key, pending);
  const settle = (next: DetailsEntry) => {
    // A newer request for the same row (changed signature) owns the entry now.
    if (detailsCache.get(row.key) !== pending) return;
    detailsCache.set(row.key, next);
    if (current && current.list.isConnected) rebuild();
  };
  let promise: Promise<unknown>;
  try {
    promise = Promise.resolve(loader(row.name));
  } catch (error) {
    promise = Promise.reject(error);
  }
  promise.then(
    (raw) => settle({ signature, state: "ready", view: sanitizeModelDetails(raw), at: now() }),
    (error: unknown) =>
      settle({
        signature,
        state: "error",
        error: text(error instanceof Error ? error.message : String(error), 160),
        at: now(),
      })
  );
}

function rebuild(): void {
  if (!current) return;
  const { list, rows, hooks } = current;
  // Open rows ask for their details first (first expansion only loads), so the frame drawn next
  // already says "Loading details…".
  for (const row of rows) {
    if (expandedKeys.has(row.key)) startDetailsLoad(row, hooks);
  }
  const focus = captureModelListFocus(list);
  list.replaceChildren(...rows.map((row, index) => rowNode(row, hooks, index)));
  restoreModelListFocus(list, focus);
}

function toggleRow(key: string): void {
  if (expandedKeys.has(key)) expandedKeys.delete(key);
  else expandedKeys.add(key);
  rebuild();
}

/**
 * Draws the list into `container` (appends one `.ollama-model-rows`). The caller clears the container
 * before each call. Open rows stay open: their state lives in this module, not in the DOM.
 */
export function renderModelList(container: HTMLElement, rows: ModelListRow[], hooks: ModelListHooks): HTMLElement {
  const list = el("div", "qwen-model-cards ollama-model-rows");
  list.setAttribute("role", "list");
  list.setAttribute("aria-label", "Models");

  // A row that no longer exists should not stay open when it comes back (a deleted model).
  const keys = new Set(rows.map((row) => row.key));
  for (const key of Array.from(expandedKeys)) {
    if (!keys.has(key)) expandedKeys.delete(key);
  }

  list.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const actionButton = target.closest<HTMLButtonElement>("button[data-action]");
    const rowElement = target.closest<HTMLElement>(".ollama-model-row");
    const key = rowElement?.dataset.key;
    const row = key ? current?.rows.find((candidate) => candidate.key === key) : undefined;
    if (actionButton) {
      if (actionButton.disabled || !row || !current) return;
      current.hooks.onAction(actionButton.dataset.action ?? "", row, actionButton.dataset);
      return;
    }
    const link = target.closest<HTMLElement>("[data-open-url]");
    if (link) {
      const url = link.dataset.openUrl ?? "";
      if (url && current?.hooks.openLink) current.hooks.openLink(url);
      return;
    }
    // Anywhere on the head of a row (the toggle itself, a chip, the gap) opens or closes it;
    // the opened panel is for reading and selecting text.
    if (key && target.closest(".ollama-model-row-head")) toggleRow(key);
  });

  current = { list, rows, hooks };
  container.appendChild(list);
  rebuild();
  return list;
}
