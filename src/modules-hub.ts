import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import * as dom from "./dom-refs";
import { ASSISTANT_CORE_MODULE_ID, ASSISTANT_PRESENCE_MODULE_ID, isAssistantCoreAvailable, setSettings, settings } from "./state";
import { renderSettings } from "./settings";
import type { Settings } from "./types";
import { showToast } from "./toast";
import { openGddFlow } from "./gdd-flow";
import { openMainTab } from "./wiring/app-chrome.wire";
import { focusWorkflowAgentConsole, syncWorkflowAgentConsoleState } from "./workflow-agent-console";
import { focusVoiceOutputConsole, syncVoiceOutputConsoleState } from "./voice-output-console";
import { focusFirstElement } from "./modal-focus";
import type { ModuleDescriptor, ModuleHealthStatus, ModuleUpdateInfo } from "./types";

let initialized = false;
let moduleSnapshot: ModuleDescriptor[] = [];
let moduleHealthSnapshot = new Map<string, ModuleHealthStatus>();

/// An on-demand module discovered in the remote modules-index.json.
interface AvailableModule {
  id: string;
  kind: string;
  name: string;
  version: string;
  size: number;
  installed: boolean;
  installed_version: string | null;
  update_available: boolean;
}

interface ModuleDownloadProgress {
  module_id: string;
  stage: string; // "downloading" | "verifying" | "installing" | "complete" | "error"
  message: string;
  downloaded: number | null;
  total: number | null;
  percent: number | null;
}

// Modules offered by the remote index, keyed by id. Empty when offline or the
// index is unreachable — the Hub still renders the registry view.
let availableSnapshot = new Map<string, AvailableModule>();
// Modules with an in-flight download, keyed by id.
let downloadProgress = new Map<string, ModuleDownloadProgress>();

function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return "";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  if (mb >= 1) return `${Math.round(mb)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function moduleStateLabel(moduleState: ModuleDescriptor["state"]): string {
  if (moduleState === "active") return "Active";
  if (moduleState === "installed") return "Installed";
  if (moduleState === "enabled") return "Enabled";
  if (moduleState === "not_installed") return "Not installed";
  return "Error";
}

function moduleHealthLabel(healthState: ModuleHealthStatus["state"]): string {
  if (healthState === "needs_setup") return "Needs setup";
  if (healthState === "fallback_active") return "Fallback";
  if (healthState === "local_warning") return "Warning";
  if (healthState === "release_blocker") return "Error";
  if (healthState === "degraded") return "Warning";
  if (healthState === "error") return "Error";
  return "Ok";
}

function moduleCardHealth(moduleInfo: ModuleDescriptor): ModuleHealthStatus | undefined {
  return moduleHealthSnapshot.get(moduleInfo.id);
}

type PermissionCopy = {
  label: string;
  detail: string;
};

function permissionCopy(permission: string): PermissionCopy {
  if (permission === "screen_capture") {
    return {
      label: "Screen capture",
      detail: "Low-FPS context frames stay in RAM and are not written to disk.",
    };
  }
  if (permission === "audio_output") {
    return {
      label: "Audio output",
      detail: "Allows spoken replies via TTS playback only (no microphone capture).",
    };
  }
  if (permission === "filesystem_history") {
    return {
      label: "Transcript history",
      detail: "Allows module access to existing local transcript entries.",
    };
  }
  if (permission === "filesystem_exports") {
    return {
      label: "Export files",
      detail: "Allows module access to generated local export files.",
    };
  }
  if (permission === "network_confluence") {
    return {
      label: "Confluence network",
      detail: "Allows calls to configured Confluence Cloud endpoints.",
    };
  }
  if (permission === "keyring_access") {
    return {
      label: "Credential storage",
      detail: "Allows secure credential read/write via keyring storage.",
    };
  }
  return {
    label: permission,
    detail: "Permission required by this module.",
  };
}

function moduleScopeHint(moduleId: string): string {
  if (moduleId === "input_vision") {
    return "Privacy scope: in-memory vision context only; no automatic image file persistence.";
  }
  if (moduleId === "output_voice_tts") {
    return "Privacy scope: TTS playback only; no microphone recording.";
  }
  return "";
}

function consentFeedback(moduleInfo: ModuleDescriptor, missing: string[]): string {
  if (!missing.length) return "";
  if (moduleInfo.id === "input_vision") {
    return "Screen capture consent missing. Vision context remains disabled until granted.";
  }
  if (moduleInfo.id === "output_voice_tts") {
    return "Audio output consent missing. Spoken replies remain disabled until granted.";
  }
  const labels = missing.map((permission) => permissionCopy(permission).label).join(", ");
  return `Consent required: ${labels}`;
}

function consentDialogMessage(moduleInfo: ModuleDescriptor, missing: string[]): string {
  const details = missing.map((permission) => {
    const copy = permissionCopy(permission);
    return `- ${copy.label}: ${copy.detail}`;
  });
  const scopeHint = moduleScopeHint(moduleInfo.id);
  const lines = [`Enable module "${moduleInfo.name}"?`, "", "This grants:", ...details];
  if (scopeHint) {
    lines.push("", scopeHint);
  }
  lines.push("", "Continue?");
  return lines.join("\n");
}

function moduleGuide(moduleId: string): { description: string; usage: string } {
  if (moduleId === "gdd") {
    return {
      description: "Builds structured Game Design Documents from transcript sessions.",
      usage: "Use: Open GDD Flow, pick a session/preset, then generate and publish.",
    };
  }
  if (moduleId === "integrations_confluence") {
    return {
      description: "Handles Confluence Cloud auth, routing, and page create/update calls.",
      usage: "Use: Configure Confluence connection in GDD Flow before publishing.",
    };
  }
  if (moduleId === ASSISTANT_CORE_MODULE_ID || moduleId === "workflow_agent") {
    return {
      description: "Desktop assistant runtime for conversation, direct actions, and confirmable GDD plans.",
      usage: "Use: Enable Assistant mode, open Assistant Debug, then parse commands or voice actions.",
    };
  }
  if (moduleId === ASSISTANT_PRESENCE_MODULE_ID) {
    return {
      description: "Floating assistant presence window with live transcript, state, and action feedback.",
      usage: "Use: Keep Assistant mode active to auto-show the Presence window as the primary assistant surface.",
    };
  }
  if (moduleId === "analysis") {
    return {
      description: "Runs analysis workflows on transcript history and exportable data.",
      usage: "Use: Enable module, then launch Analysis Flow from this page.",
    };
  }
  if (moduleId === "ai_refinement") {
    return {
      description: "Adds optional local AI transcript refinement and prompt/runtime controls.",
      usage: "Use: Enable module, then open the AI Refinement tab to configure provider and prompts.",
    };
  }
  if (moduleId === "task_capture") {
    return {
      description: "Voice-activated task capture with AI refinement and Confluence agenda posting.",
      usage: "Say 'Trispr erinnere mich...' or 'Trispr remind me...' to capture tasks.",
    };
  }
  if (moduleId === "input_vision") {
    return {
      description: "Adds low-FPS screen context capture for agent-driven workflows.",
      usage: "Use: Enable and start the vision stream when screen context is needed.",
    };
  }
  if (moduleId === "output_voice_tts") {
    return {
      description: "Adds spoken agent replies via Windows-native or local TTS output.",
      usage: "Use: Enable, select provider/voice, then run a TTS test.",
    };
  }
  return {
    description: "Managed module with isolated lifecycle, health, and permissions.",
    usage: "Use: Enable it, then launch the related flow from this modules page.",
  };
}

function missingConsents(moduleInfo: ModuleDescriptor): string[] {
  if (moduleInfo.state === "not_installed") return [];
  const consented = settings?.module_settings?.consented_permissions?.[moduleInfo.id] || [];
  return moduleInfo.permissions.filter((permission) => !consented.includes(permission));
}

function moduleStateKey(moduleInfo: ModuleDescriptor): string {
  if (moduleInfo.core) return "core";
  if (moduleInfo.state === "active") return "active";
  if (moduleInfo.state === "enabled" || moduleInfo.state === "installed") return "inactive";
  return "unavailable";
}

function moduleStatusClass(moduleInfo: ModuleDescriptor): string {
  const health = moduleCardHealth(moduleInfo);
  if (health && health.state !== "ok") {
    if (health.state === "release_blocker" || health.state === "error") {
      return "model-status is-error";
    }
    return "model-status model-status--warning";
  }
  if (moduleInfo.core) return "model-status active";
  if (moduleInfo.state === "active") return "model-status downloaded";
  if (moduleInfo.state === "enabled" || moduleInfo.state === "installed")
    return "model-status model-status--installed";
  if (moduleInfo.state === "error") return "model-status is-error";
  return "model-status available";
}

function moduleStatusLabel(moduleInfo: ModuleDescriptor): string {
  const health = moduleCardHealth(moduleInfo);
  if (health && health.state !== "ok") return moduleHealthLabel(health.state);
  return moduleInfo.core ? "Core" : moduleStateLabel(moduleInfo.state);
}

function openModuleConfig(moduleId: string): void {
  const moduleInfo = moduleSnapshot.find((m) => m.id === moduleId);
  if (!moduleInfo) return;

  if (moduleId === "output_voice_tts") {
    const enabled = settings?.module_settings?.enabled_modules?.includes("output_voice_tts") ?? false;
    if (!enabled) {
      showToast({
        type: "info",
        title: "Enable module first",
        message: "Enable Voice Output to open its configuration tab.",
        duration: 3200,
      });
      return;
    }
    openMainTab("voice-output");
    focusVoiceOutputConsole();
    return;
  }

  if (moduleId === "ai_refinement") {
    const enabled = settings?.module_settings?.enabled_modules?.includes("ai_refinement") ?? false;
    if (!enabled) {
      showToast({
        type: "info",
        title: "Enable module first",
        message: "Enable AI Refinement to open its configuration tab.",
        duration: 3200,
      });
      return;
    }
    openMainTab("ai-refinement");
    return;
  }

  if (moduleId === "task_capture") {
    const enabled = settings?.module_settings?.enabled_modules?.includes("task_capture") ?? false;
    if (!enabled) {
      showToast({
        type: "info",
        title: "Enable module first",
        message: "Enable Task Capture to open its configuration tab.",
        duration: 3200,
      });
      return;
    }
    openMainTab("task-capture");
    return;
  }

  if (moduleId === ASSISTANT_CORE_MODULE_ID || moduleId === "workflow_agent") {
    const enabled = isAssistantCoreAvailable();
    if (!enabled) {
      showToast({
        type: "info",
        title: "Enable module first",
        message: "Enable Assistant Core to open its debug tab.",
        duration: 3200,
      });
      return;
    }
    openMainTab("agent");
    focusWorkflowAgentConsole();
    return;
  }

  if (moduleId === ASSISTANT_PRESENCE_MODULE_ID) {
    void invoke("show_assistant_presence_window").catch((error) => {
      showToast({
        type: "warning",
        title: "Presence unavailable",
        message: String(error),
        duration: 3200,
      });
    });
    return;
  }

  if (!dom.moduleConfigModal) return;

  const guide = moduleGuide(moduleId);
  const missing = missingConsents(moduleInfo);
  const scopeHint = moduleScopeHint(moduleId);
  const feedbackParts: string[] = [];
  const consentMessage = consentFeedback(moduleInfo, missing);
  if (consentMessage) feedbackParts.push(consentMessage);
  if (moduleInfo.last_error) feedbackParts.push(moduleInfo.last_error);

  if (dom.moduleConfigModalName)
    dom.moduleConfigModalName.textContent = moduleInfo.name;
  if (dom.moduleConfigModalMeta)
    dom.moduleConfigModalMeta.textContent = `ID: ${moduleInfo.id} · v${moduleInfo.version} · ${moduleStateLabel(moduleInfo.state)}`;
  if (dom.moduleConfigModalDesc)
    dom.moduleConfigModalDesc.textContent = guide.description;
  if (dom.moduleConfigModalUsage)
    dom.moduleConfigModalUsage.textContent = scopeHint
      ? `${guide.usage} ${scopeHint}`
      : guide.usage;
  if (dom.moduleConfigModalDeps)
    dom.moduleConfigModalDeps.textContent = moduleInfo.dependencies.length
      ? `Deps: ${moduleInfo.dependencies.join(", ")}`
      : "";
  if (dom.moduleConfigModalFeedback)
    dom.moduleConfigModalFeedback.textContent = feedbackParts.join(" · ") || "Ready";
  dom.moduleConfigModal.removeAttribute("hidden");
  focusFirstElement(dom.moduleConfigModal);
}

function closeModuleConfig(): void {
  dom.moduleConfigModal?.setAttribute("hidden", "");
}

function cardActions(moduleInfo: ModuleDescriptor): string {
  if (moduleInfo.core) {
    return `<button class="hotkey-record-btn" disabled>Core (always on)</button>
    <button class="hotkey-record-btn" data-module-action="health" data-module-id="${moduleInfo.id}">Health</button>
    <button class="hotkey-record-btn" data-module-action="updates" data-module-id="${moduleInfo.id}">Check updates</button>`;
  }

  const canEnable = moduleInfo.toggleable && (moduleInfo.state === "installed" || moduleInfo.state === "error");
  const canDisable = moduleInfo.toggleable && (moduleInfo.state === "active" || moduleInfo.state === "enabled");
  const canInstall = moduleInfo.toggleable && moduleInfo.bundled && moduleInfo.state === "not_installed";

  // On-demand module: present in the remote index, not bundled into the binary.
  const available = availableSnapshot.get(moduleInfo.id);
  const downloadable = !!available;
  const progress = downloadProgress.get(moduleInfo.id);

  const id = moduleInfo.id;
  let primary: string;
  if (progress) {
    primary = `<button class="hotkey-record-btn" disabled>${escapeHtml(downloadStageLabel(progress))}</button>`;
  } else if (canEnable) {
    primary = `<button class="hotkey-record-btn" data-module-action="enable" data-module-id="${id}">Enable</button>`;
  } else if (canDisable) {
    primary = `<button class="hotkey-record-btn" data-module-action="disable" data-module-id="${id}">Disable</button>`;
  } else if (moduleInfo.state === "not_installed" && downloadable) {
    const sizeLabel = formatBytes(available!.size);
    primary = `<button class="hotkey-record-btn" data-module-action="download" data-module-id="${id}">Download${sizeLabel ? ` (${sizeLabel})` : ""}</button>`;
  } else if (canInstall) {
    primary = `<button class="hotkey-record-btn" data-module-action="install" data-module-id="${id}">Install</button>`;
  } else {
    primary = `<button class="hotkey-record-btn" data-module-action="install" data-module-id="${id}" disabled>Install</button>`;
  }

  // Secondary affordances for installed on-demand modules.
  const update = downloadable && available!.update_available && moduleInfo.state !== "not_installed" && !progress
    ? `<button class="hotkey-record-btn" data-module-action="download" data-module-id="${id}">Update to v${escapeHtml(available!.version)}</button>`
    : "";
  const uninstall = downloadable && moduleInfo.state !== "not_installed" && !progress
    ? `<button class="ghost-btn" data-module-action="uninstall" data-module-id="${id}">Uninstall</button>`
    : "";

  return `${primary}
    ${update}
    ${uninstall}
    <button class="hotkey-record-btn" data-module-action="health" data-module-id="${id}">Health</button>
    <button class="hotkey-record-btn" data-module-action="updates" data-module-id="${id}">Check updates</button>`;
}

function downloadStageLabel(progress: ModuleDownloadProgress): string {
  switch (progress.stage) {
    case "verifying":
      return "Verifying…";
    case "installing":
      return "Installing…";
    case "complete":
      return "Done";
    case "error":
      return "Failed";
    default:
      return progress.percent != null ? `Downloading ${progress.percent}%` : "Downloading…";
  }
}

type ModuleGroupKey = "active" | "installed" | "available" | "core";

const MODULE_GROUP_ORDER: ModuleGroupKey[] = ["active", "installed", "available", "core"];

const MODULE_GROUP_LABEL: Record<ModuleGroupKey, string> = {
  active: "Active",
  installed: "Installed · inactive",
  available: "Available to add",
  core: "Core · always on",
};

function moduleGroupKey(moduleInfo: ModuleDescriptor): ModuleGroupKey {
  if (moduleInfo.core) return "core";
  if (moduleInfo.state === "active") return "active";
  if (moduleInfo.state === "not_installed") return "available";
  return "installed"; // installed, enabled, or error — present but not running
}

function renderModuleRow(moduleInfo: ModuleDescriptor): string {
  const summary = `Deps ${moduleInfo.dependencies.length} · Perms ${moduleInfo.permissions.length}`;
  const summaryTitle = escapeHtml(`Dependencies: ${moduleInfo.dependencies.join(", ") || "none"}\nPermissions: ${moduleInfo.permissions.join(", ") || "none"}`);
  const guide = moduleGuide(moduleInfo.id);
  const scopeHint = moduleScopeHint(moduleInfo.id);
  const descText = scopeHint ? `${guide.description} ${guide.usage} ${scopeHint}` : `${guide.description} ${guide.usage}`;
  const health = moduleCardHealth(moduleInfo);
  const healthIsSetupWarning = health
    ? ["needs_setup", "fallback_active", "local_warning", "degraded"].includes(health.state)
    : false;
  const missing = missingConsents(moduleInfo);
  const consentMessage = consentFeedback(moduleInfo, missing);
  const feedbackParts: string[] = [];
  if (consentMessage) {
    feedbackParts.push(consentMessage);
  }
  if (moduleInfo.last_error) {
    feedbackParts.push(moduleInfo.last_error);
  }
  // Only surface feedback when there is something actionable; "Ready" was noise.
  const feedbackText = feedbackParts.join(" · ");
  const consentDetails = missing
    .map((permission) => {
      const copy = permissionCopy(permission);
      return `${copy.label}: ${copy.detail}`;
    })
    .join("\n");
  const feedbackTitle = escapeHtml([feedbackParts.join("\n"), consentDetails].filter(Boolean).join("\n"));
  const feedbackClass = moduleInfo.last_error && !healthIsSetupWarning
    ? "module-row-feedback is-error"
    : consentMessage
      ? "module-row-feedback is-warning"
      : "module-row-feedback is-ok";
  const feedback = feedbackText
    ? `<span class="${feedbackClass}" title="${feedbackTitle}">${escapeHtml(feedbackText)}</span>`
    : "";
  const launch = moduleInfo.id === "gdd"
    ? `<button class="ghost-btn" data-module-action="launch-gdd" data-module-id="gdd">Open GDD Flow</button>`
    : moduleInfo.id === "analysis"
      ? `<button class="ghost-btn" data-module-action="launch-analysis" data-module-id="analysis">Open Analysis Flow</button>`
      : moduleInfo.core
        ? ""
        : `<button class="ghost-btn" data-module-action="open-config" data-module-id="${moduleInfo.id}">Configure</button>`;

  return `<div class="module-row" data-module-card="${moduleInfo.id}" data-module-state="${moduleStateKey(moduleInfo)}">
        <div class="module-row-main">
          <div class="module-row-title">
            <span class="model-name" title="${escapeHtml(descText)}">${escapeHtml(moduleInfo.name)}</span>
            <span class="${moduleStatusClass(moduleInfo)}">${moduleStatusLabel(moduleInfo)}</span>
          </div>
          <div class="module-row-meta">
            <span class="model-meta" title="${summaryTitle}">ID: <code>${escapeHtml(moduleInfo.id)}</code> · v${escapeHtml(moduleInfo.version)} · ${summary}</span>
            ${feedback}
          </div>
        </div>
        <div class="module-row-actions">${cardActions(moduleInfo)} ${launch}</div>
      </div>`;
}

function renderModulesList(modules: ModuleDescriptor[]): void {
  if (!dom.modulesList) return;
  if (modules.length === 0) {
    dom.modulesList.innerHTML = `<div class="field-hint">No modules registered.</div>`;
    return;
  }

  const grouped = new Map<ModuleGroupKey, ModuleDescriptor[]>();
  for (const moduleInfo of modules) {
    const key = moduleGroupKey(moduleInfo);
    const bucket = grouped.get(key);
    if (bucket) {
      bucket.push(moduleInfo);
    } else {
      grouped.set(key, [moduleInfo]);
    }
  }

  dom.modulesList.innerHTML = MODULE_GROUP_ORDER.map((key) => {
    const bucket = grouped.get(key);
    if (!bucket || bucket.length === 0) return "";
    const rows = [...bucket]
      .sort((a, b) => a.name.localeCompare(b.name))
      .map(renderModuleRow)
      .join("\n");
    return `<section class="modules-group" data-module-group="${key}">
        <h3 class="modules-group-label">${MODULE_GROUP_LABEL[key]} <span class="modules-group-count">${bucket.length}</span></h3>
        <div class="modules-group-body">${rows}</div>
      </section>`;
  })
    .filter(Boolean)
    .join("\n");
}

async function refreshModuleState(): Promise<void> {
  const modules = await invoke<ModuleDescriptor[]>("list_modules");
  try {
    const health = await invoke<ModuleHealthStatus[]>("get_module_health");
    moduleHealthSnapshot = new Map(
      Array.isArray(health) ? health.map((entry) => [entry.module_id, entry]) : []
    );
  } catch {
    moduleHealthSnapshot = new Map();
  }
  moduleSnapshot = modules;
  renderModulesList(modules);
  syncWorkflowAgentConsoleState();
  syncVoiceOutputConsoleState();
  if (dom.modulesStatus) {
    const active = modules.filter((moduleInfo) => moduleInfo.state === "active").length;
    const pendingConsents = modules.filter((moduleInfo) => missingConsents(moduleInfo).length > 0)
      .length;
    dom.modulesStatus.textContent = pendingConsents > 0
      ? `${active}/${modules.length} active · ${pendingConsents} consent pending`
      : `${active}/${modules.length} active`;
  }

  // Annotate with remote index data (download size, updates) in the background
  // so an unreachable index never blocks the registry view.
  void refreshAvailableModules();
}

/// Fetch the remote module index and re-render so downloadable modules gain a
/// Download/Update/Uninstall affordance. Best-effort: failures (offline,
/// unreachable index) leave the registry view intact.
async function refreshAvailableModules(): Promise<void> {
  try {
    const available = await invoke<AvailableModule[]>("list_available_modules");
    availableSnapshot = new Map(
      Array.isArray(available) ? available.map((entry) => [entry.id, entry]) : []
    );
    renderModulesList(moduleSnapshot);
  } catch {
    // Index unreachable — keep whatever we had; the registry view still works.
  }
}

async function refreshSettingsAndRender(): Promise<void> {
  // Re-fetch settings from backend so module changes are reflected in the frontend
  // state before renderSettings() checks isAssistantCoreAvailable() etc.
  try {
    const fresh = await invoke<Settings>("get_settings");
    setSettings(fresh);
  } catch {
    // non-fatal — render with whatever we have
  }
  renderSettings();
}

async function handleEnable(moduleId: string): Promise<void> {
  const moduleInfo = moduleSnapshot.find((candidate) => candidate.id === moduleId);
  if (!moduleInfo) return;
  if (!moduleInfo.toggleable) {
    showToast({
      type: "info",
      title: "Core module",
      message: `${moduleInfo.name} is always active and cannot be toggled.`,
      duration: 3200,
    });
    return;
  }

  const missing = missingConsents(moduleInfo);
  const grants = [...missing];
  if (missing.length > 0) {
    const confirmed = window.confirm(consentDialogMessage(moduleInfo, missing));
    if (!confirmed) return;
  }

  try {
    const result = await invoke<{ restart_required?: boolean; message?: string }>("enable_module", {
      moduleId,
      grantPermissions: grants,
    });
    await refreshModuleState();
    await refreshSettingsAndRender();
    showToast({
      type: "success",
      title: "Module enabled",
      message: result?.message || `${moduleInfo.name} is now active.`,
      duration: 3200,
    });
    if (result?.restart_required) {
      showToast({
        type: "warning",
        title: "Restart recommended",
        message: `${moduleInfo.name} requires app restart for full activation.`,
        duration: 4500,
      });
    }
  } catch (error) {
    showToast({
      type: "error",
      title: "Enable failed",
      message: String(error),
      duration: 5000,
    });
    await refreshModuleState();
  }
}

async function handleInstall(moduleId: string): Promise<void> {
  const moduleInfo = moduleSnapshot.find((candidate) => candidate.id === moduleId);
  if (!moduleInfo) return;
  try {
    const result = await invoke<{ installed: boolean; target_dir: string }>("install_bundled_module_package", {
      moduleId,
    });
    await refreshModuleState();
    showToast({
      type: "success",
      title: result.installed ? "Module installed" : "Module already installed",
      message: `${moduleInfo.name} package is available locally.`,
      duration: 3600,
    });
  } catch (error) {
    showToast({
      type: "error",
      title: "Install failed",
      message: String(error),
      duration: 5200,
    });
    await refreshModuleState();
  }
}

async function handleDisable(moduleId: string): Promise<void> {
  const moduleInfo = moduleSnapshot.find((candidate) => candidate.id === moduleId);
  if (moduleInfo && !moduleInfo.toggleable) {
    showToast({
      type: "info",
      title: "Core module",
      message: `${moduleInfo.name} is always active and cannot be toggled.`,
      duration: 3200,
    });
    return;
  }
  try {
    await invoke("disable_module", { moduleId });
    await refreshModuleState();
    await refreshSettingsAndRender();
    showToast({
      type: "success",
      title: "Module disabled",
      message: "Execution stopped. Module data was kept.",
      duration: 3200,
    });
  } catch (error) {
    showToast({
      type: "error",
      title: "Disable failed",
      message: String(error),
      duration: 5000,
    });
  }
}

async function handleHealth(moduleId: string): Promise<void> {
  try {
    const health = await invoke<ModuleHealthStatus[]>("get_module_health", { moduleId });
    const entry = health[0];
    if (!entry) return;
    const toastType = entry.state === "ok"
      ? "success"
      : entry.state === "release_blocker" || entry.state === "error"
        ? "error"
        : "warning";
    showToast({
      type: toastType,
      title: `${moduleId} health: ${entry.state}`,
      message: entry.detail,
      duration: 4500,
    });
  } catch (error) {
    showToast({
      type: "error",
      title: "Health check failed",
      message: String(error),
      duration: 5000,
    });
  }
}

async function handleUpdates(moduleId: string): Promise<void> {
  try {
    const updates = await invoke<ModuleUpdateInfo[]>("check_module_updates", { moduleId });
    const update = updates[0];
    if (!update) return;
    showToast({
      type: update.update_available ? "warning" : "info",
      title: update.update_available ? "Update available" : "No updates",
      message: `${moduleId}: ${update.current_version} -> ${update.latest_version}`,
      duration: 4200,
    });
  } catch (error) {
    showToast({
      type: "error",
      title: "Update check failed",
      message: String(error),
      duration: 5000,
    });
  }
}

async function handleDownload(moduleId: string): Promise<void> {
  const moduleInfo = moduleSnapshot.find((candidate) => candidate.id === moduleId);
  const name = moduleInfo?.name || moduleId;
  // Seed an in-progress state so the row immediately shows feedback even before
  // the first progress event arrives.
  downloadProgress.set(moduleId, {
    module_id: moduleId,
    stage: "downloading",
    message: "Starting…",
    downloaded: 0,
    total: null,
    percent: 0,
  });
  renderModulesList(moduleSnapshot);
  try {
    await invoke("download_module", { moduleId });
    downloadProgress.delete(moduleId);
    await refreshModuleState();
    showToast({
      type: "success",
      title: "Module installed",
      message: `${name} downloaded and installed.`,
      duration: 3600,
    });
  } catch (error) {
    downloadProgress.delete(moduleId);
    await refreshModuleState();
    showToast({
      type: "error",
      title: "Download failed",
      message: String(error),
      duration: 5200,
    });
  }
}

async function handleUninstall(moduleId: string): Promise<void> {
  const moduleInfo = moduleSnapshot.find((candidate) => candidate.id === moduleId);
  const name = moduleInfo?.name || moduleId;
  if (!window.confirm(`Uninstall ${name}? Its files will be removed; you can download it again later.`)) {
    return;
  }
  try {
    await invoke("uninstall_module", { moduleId });
    await refreshModuleState();
    showToast({
      type: "success",
      title: "Module uninstalled",
      message: `${name} was removed.`,
      duration: 3200,
    });
  } catch (error) {
    showToast({
      type: "error",
      title: "Uninstall failed",
      message: String(error),
      duration: 5000,
    });
    await refreshModuleState();
  }
}

function bindModulesEvents(): void {
  if (!dom.modulesList) return;
  dom.modulesList.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const button = target?.closest<HTMLButtonElement>("[data-module-action]");
    if (!button) return;

    const moduleId = button.dataset.moduleId || "";
    const action = button.dataset.moduleAction || "";

    if (action === "enable") {
      void handleEnable(moduleId);
      return;
    }
    if (action === "install") {
      void handleInstall(moduleId);
      return;
    }
    if (action === "download") {
      void handleDownload(moduleId);
      return;
    }
    if (action === "uninstall") {
      void handleUninstall(moduleId);
      return;
    }
    if (action === "disable") {
      void handleDisable(moduleId);
      return;
    }
    if (action === "health") {
      void handleHealth(moduleId);
      return;
    }
    if (action === "updates") {
      void handleUpdates(moduleId);
      return;
    }
    if (action === "launch-gdd") {
      void openGddFlow();
      return;
    }
    if (action === "launch-analysis") {
      showToast({
        type: "info",
        title: "Analysis module",
        message: "Analysis module launcher will be wired when analysis module is installed.",
        duration: 3200,
      });
      return;
    }
    if (action === "open-config") {
      openModuleConfig(moduleId);
      return;
    }
  });

  dom.moduleConfigModalClose?.addEventListener("click", closeModuleConfig);
  dom.moduleConfigModalBackdrop?.addEventListener("click", closeModuleConfig);

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !dom.moduleConfigModal?.hasAttribute("hidden")) {
      closeModuleConfig();
    }
  });

  window.addEventListener("modules:focus", (event: Event) => {
    const detail = (event as CustomEvent<string>).detail;
    if (!detail || !dom.modulesList) return;
    const card = dom.modulesList.querySelector<HTMLElement>(`[data-module-card='${detail}']`);
    card?.scrollIntoView({ behavior: "smooth", block: "center" });
  });
}

export function initModulesHub(): void {
  if (initialized) return;
  initialized = true;
  bindModulesEvents();
  void listen<ModuleDownloadProgress>("module:download-progress", (event) => {
    const p = event.payload;
    if (!p || !p.module_id) return;
    // Terminal stages are handled by the awaited invoke in handleDownload
    // (toast + refresh); ignore them here to avoid a double refresh.
    if (p.stage === "complete" || p.stage === "error") return;
    downloadProgress.set(p.module_id, p);
    // Targeted button update to avoid re-rendering the whole list each tick.
    const row = dom.modulesList?.querySelector<HTMLElement>(`[data-module-card="${p.module_id}"]`);
    const btn = row?.querySelector<HTMLButtonElement>(".module-row-actions button");
    if (btn) {
      btn.textContent = downloadStageLabel(p);
      btn.disabled = true;
    } else {
      renderModulesList(moduleSnapshot);
    }
  });
  void refreshModuleState();
}

export function refreshModulesHub(): void {
  void refreshModuleState();
}
