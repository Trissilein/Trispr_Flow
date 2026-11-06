// Troubleshooting panel of the setup wizard's engine step: what went wrong, what to do, and the
// way out when the app cannot download the engine itself (download it in the browser, then pick
// the file). `onboarding-wizard.ts` owns the setup flow; this file owns the panel.

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  WHICH_ONE_DO_I_NEED,
  buildDiagnosticsText,
  orderPackages,
  type ManualInfo,
  type ManualPackage,
  type RuntimeRecommendation,
  type RuntimeSetupError,
  type RuntimeVariant,
  type SetupDiagnostics,
} from "./onboarding-wizard-logic";

export interface TroubleshootHandlers {
  /** Try the download of the engine chosen above again. */
  onRetry: () => void;
  /** Install the zip the user picked (the wizard imports it and runs the self-test). */
  onImport: (variant: RuntimeVariant, path: string) => void;
}

export interface TroubleshootView {
  error: RuntimeSetupError;
  /** The engine that failed; goes into the diagnostics. */
  variant: RuntimeVariant | null;
  /** The package listed first. */
  preferred: RuntimeVariant | null;
  recommendation: RuntimeRecommendation | null;
  /** Something that happened before the failure, e.g. "CUDA does not work on this PC: ...". */
  context?: string;
}

let handlers: TroubleshootHandlers | null = null;
let view: TroubleshootView | null = null;
let manualInfo: ManualInfo | null = null;
let bound = false;

function byId<T extends HTMLElement = HTMLElement>(id: string): T | null {
  return document.getElementById(id) as T | null;
}

function setText(id: string, text: string): void {
  const el = byId(id);
  if (el) el.textContent = text;
}

function say(text: string): void {
  setText("wizard-troubleshoot-note", text);
}

async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export function bindTroubleshootPanel(next: TroubleshootHandlers): void {
  handlers = next;
  if (bound) return;
  bound = true;
  byId("wizard-troubleshoot-retry")?.addEventListener("click", () => handlers?.onRetry());
  byId("wizard-troubleshoot-copy-diagnostics")?.addEventListener("click", () => void copyDiagnostics());
  setText("wizard-troubleshoot-which", WHICH_ONE_DO_I_NEED);
}

export function hideTroubleshoot(): void {
  const panel = byId("wizard-troubleshoot");
  if (panel) panel.hidden = true;
  view = null;
}

export function showTroubleshoot(next: TroubleshootView): void {
  view = next;
  setText("wizard-troubleshoot-cause", next.error.message);
  setText("wizard-troubleshoot-hint", next.error.hint);
  const context = byId("wizard-troubleshoot-context");
  if (context) {
    context.textContent = next.context ?? "";
    context.hidden = !next.context;
  }
  say("");
  const diagnostics = byId<HTMLTextAreaElement>("wizard-troubleshoot-diagnostics");
  if (diagnostics) diagnostics.hidden = true;
  void renderPackages();
  const panel = byId("wizard-troubleshoot");
  if (panel) panel.hidden = false;
}

async function loadManualInfo(): Promise<ManualInfo | null> {
  if (manualInfo) return manualInfo;
  try {
    manualInfo = await invoke<ManualInfo>("get_whisper_runtime_manual_info");
  } catch (err) {
    console.warn("Could not read the manual download info", err);
  }
  return manualInfo;
}

async function renderPackages(): Promise<void> {
  const container = byId("wizard-troubleshoot-packages");
  if (!container) return;
  const info = await loadManualInfo();
  container.replaceChildren();
  if (!info) {
    const missing = document.createElement("p");
    missing.className = "field-hint";
    missing.textContent = "The package list could not be read. Restart Trispr Flow and try again.";
    container.append(missing);
    return;
  }
  const preferred = view?.preferred ?? view?.recommendation?.recommended ?? null;
  for (const pkg of orderPackages(info.variants, preferred)) {
    container.append(packageCard(pkg, pkg.variant === (preferred ?? "vulkan")));
  }
  const page = document.createElement("p");
  page.className = "field-hint";
  page.textContent = `All files: ${info.release_page_url}`;
  container.append(page);
}

function button(label: string, onClick: () => void, className = "hotkey-record-btn"): HTMLButtonElement {
  const el = document.createElement("button");
  el.type = "button";
  el.className = className;
  el.textContent = label;
  el.addEventListener("click", onClick);
  return el;
}

function packageCard(pkg: ManualPackage, recommended: boolean): HTMLElement {
  const card = document.createElement("div");
  card.className = "wizard-package";
  card.dataset.variant = pkg.variant;

  const head = document.createElement("div");
  head.className = "wizard-package-head";
  const title = document.createElement("strong");
  title.textContent = `${pkg.label} package`;
  head.append(title);
  if (recommended) {
    const badge = document.createElement("span");
    badge.className = "wizard-package-badge";
    badge.textContent = "Recommended for this PC";
    head.append(badge);
  }

  const gpu = document.createElement("div");
  gpu.className = "field-hint";
  gpu.textContent = pkg.which_gpu_hint;

  const file = document.createElement("div");
  file.className = "field-hint";
  file.textContent = `${pkg.asset} — ${pkg.size_mb} MB`;

  const hash = document.createElement("div");
  hash.className = "wizard-package-hash";
  const label = document.createElement("span");
  label.className = "field-hint";
  label.textContent = "SHA-256";
  const code = document.createElement("code");
  code.className = "wizard-package-sha";
  code.textContent = pkg.sha256;
  hash.append(
    label,
    code,
    button("Copy", () => {
      void copyToClipboard(pkg.sha256).then((ok) => say(ok ? "SHA-256 copied." : "Could not copy. Select the text and copy it by hand."));
    })
  );

  const actions = document.createElement("div");
  actions.className = "wizard-package-actions";
  actions.append(
    button("Open download page", () => void openDownloadPage(pkg.variant)),
    button("Use downloaded file…", () => void pickFile(pkg), "btn btn-primary")
  );

  card.append(head, gpu, file, hash, actions);
  return card;
}

async function openDownloadPage(variant: RuntimeVariant): Promise<void> {
  try {
    await invoke("open_runtime_download_page", { variant });
    say("Your browser opens the download. When it has finished, click \"Use downloaded file…\" and pick the .zip.");
  } catch (err) {
    console.warn("Could not open the download page", err);
    say("Could not open the browser. Copy the file name and SHA-256 and find the file on the release page by hand.");
  }
}

async function pickFile(pkg: ManualPackage): Promise<void> {
  let selected: string | string[] | null = null;
  try {
    selected = await open({
      directory: false,
      multiple: false,
      title: `Choose the ${pkg.label} package (${pkg.asset})`,
      filters: [{ name: "Speech engine package (.zip)", extensions: ["zip"] }],
    });
  } catch (err) {
    console.warn("File dialog failed", err);
    say("The file dialog could not be opened.");
    return;
  }
  if (typeof selected !== "string" || !selected) return;
  handlers?.onImport(pkg.variant, selected);
}

async function copyDiagnostics(): Promise<void> {
  let diagnostics: SetupDiagnostics | null = null;
  try {
    diagnostics = await invoke<SetupDiagnostics>("get_whisper_runtime_diagnostics");
  } catch (err) {
    console.warn("Could not read the diagnostics", err);
  }
  const text = buildDiagnosticsText({
    diagnostics,
    variant: view?.variant ?? null,
    error: view?.error ?? null,
    recommendation: view?.recommendation ?? null,
  });
  if (await copyToClipboard(text)) {
    say("Diagnostics copied. Paste them into your message.");
    return;
  }
  // No clipboard access: show the text so it can be selected by hand.
  const box = byId<HTMLTextAreaElement>("wizard-troubleshoot-diagnostics");
  if (box) {
    box.value = text;
    box.hidden = false;
    box.select();
  }
  say("Could not copy automatically. Select the text below and copy it.");
}
