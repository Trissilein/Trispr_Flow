import { invoke } from "@tauri-apps/api/core";
import * as dom from "./dom-refs";
import { settings } from "./state";
import { showToast } from "./toast";
import { openGddFlow } from "./gdd-flow";
import { syncWorkflowAgentConsoleState } from "./workflow-agent-console";
import { syncVoiceOutputConsoleState } from "./voice-output-console";
import { focusFirstElement } from "./modal-focus";
import type { ModuleDescriptor, ModuleHealthStatus, ModuleUpdateInfo } from "./types";

let initialized = false;
let moduleSnapshot: ModuleDescriptor[] = [];

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
  if (moduleId === "workflow_agent") {
    return {
      description: "Parses wakeword voice commands into confirmable GDD execution plans.",
      usage: "Use: Enable, open Agent Console, parse command, confirm and execute.",
    };
  }
  if (moduleId === "analysis") {
    return {
      description: "Runs analysis workflows on transcript history and exportable data.",
      usage: "Use: Enable module, then launch Analysis Flow from this page.",
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
  const consented = settings?.module_settings?.consented_permissions?.[moduleInfo.id] || [];
  return moduleInfo.permissions.filter((permission) => !consented.includes(permission));
}

function moduleStateKey(moduleInfo: ModuleDescriptor): string {
  if (moduleInfo.core) return "core";
  if (moduleInfo.state === "active") return "active";
  if (moduleInfo.state === "enabled" || moduleInfo.state === "installed") return "inactive";
  return "unavailable";
}

function openModuleConfig(moduleId: string): void {
  const moduleInfo = moduleSnapshot.find((m) => m.id === moduleId);
  if (!moduleInfo || !dom.moduleConfigModal) return;

  const guide = moduleGuide(moduleId);
  const missing = missingConsents(moduleInfo);
  const feedbackParts: string[] = [];
  if (missing.length) feedbackParts.push(`Consent required: ${missing.join(", ")}`);
  if (moduleInfo.last_error) feedbackParts.push(moduleInfo.last_error);

  if (dom.moduleConfigModalName)
    dom.moduleConfigModalName.textContent = moduleInfo.name;
  if (dom.moduleConfigModalMeta)
    dom.moduleConfigModalMeta.textContent = `ID: ${moduleInfo.id} · v${moduleInfo.version} · ${moduleStateLabel(moduleInfo.state)}`;
  if (dom.moduleConfigModalDesc)
    dom.moduleConfigModalDesc.textContent = guide.description;
  if (dom.moduleConfigModalUsage)
    dom.moduleConfigModalUsage.textContent = guide.usage;
  if (dom.moduleConfigModalDeps)
    dom.moduleConfigModalDeps.textContent = moduleInfo.dependencies.length
      ? `Deps: ${moduleInfo.dependencies.join(", ")}`
      : "";
  if (dom.moduleConfigModalFeedback)
    dom.moduleConfigModalFeedback.textContent = feedbackParts.join(" · ") || "Ready";

  if (dom.workflowAgentConsole)
    dom.workflowAgentConsole.hidden = moduleId !== "workflow_agent";
  if (dom.voiceOutputConsole)
    dom.voiceOutputConsole.hidden = moduleId !== "output_voice_tts";

  dom.moduleConfigModal.removeAttribute("hidden");
  focusFirstElement(dom.moduleConfigModal);
}

function closeModuleConfig(): void {
  dom.moduleConfigModal?.setAttribute("hidden", "");
  syncWorkflowAgentConsoleState();
  syncVoiceOutputConsoleState();
}

function cardActions(moduleInfo: ModuleDescriptor): string {
  if (moduleInfo.core) {
    return `<button class="hotkey-record-btn" disabled>Core (always on)</button>
    <button class="hotkey-record-btn" data-module-action="health" data-module-id="${moduleInfo.id}">Health</button>
    <button class="hotkey-record-btn" data-module-action="updates" data-module-id="${moduleInfo.id}">Check updates</button>`;
  }

  const canEnable = moduleInfo.toggleable && (moduleInfo.state === "installed" || moduleInfo.state === "error");
  const canDisable = moduleInfo.toggleable && (moduleInfo.state === "active" || moduleInfo.state === "enabled");

  const primary = canEnable
    ? `<button class="hotkey-record-btn" data-module-action="enable" data-module-id="${moduleInfo.id}">Enable</button>`
    : canDisable
      ? `<button class="hotkey-record-btn" data-module-action="disable" data-module-id="${moduleInfo.id}">Disable</button>`
      : `<button class="hotkey-record-btn" data-module-action="install" data-module-id="${moduleInfo.id}" disabled>Install (planned)</button>`;

  return `${primary}
    <button class="hotkey-record-btn" data-module-action="health" data-module-id="${moduleInfo.id}">Health</button>
    <button class="hotkey-record-btn" data-module-action="updates" data-module-id="${moduleInfo.id}">Check updates</button>`;
}

function renderModulesList(modules: ModuleDescriptor[]): void {
  if (!dom.modulesList) return;
  if (modules.length === 0) {
    dom.modulesList.innerHTML = `<div class="field-hint">No modules registered.</div>`;
    return;
  }

  dom.modulesList.innerHTML = modules
    .map((moduleInfo) => {
      const summary = `Deps ${moduleInfo.dependencies.length} · Perms ${moduleInfo.permissions.length}`;
      const summaryTitle = escapeHtml(`Dependencies: ${moduleInfo.dependencies.join(", ") || "none"}\nPermissions: ${moduleInfo.permissions.join(", ") || "none"}`);
      const guide = moduleGuide(moduleInfo.id);
      const missing = missingConsents(moduleInfo);
      const feedbackParts: string[] = [];
      if (missing.length) {
        feedbackParts.push(`Consent required: ${missing.join(", ")}`);
      }
      if (moduleInfo.last_error) {
        feedbackParts.push(moduleInfo.last_error);
      }
      const feedbackText = feedbackParts.length ? feedbackParts.join(" · ") : "Ready";
      const feedbackTitle = escapeHtml(feedbackParts.join("\n"));
      const feedbackClass = moduleInfo.last_error
        ? "module-card-feedback is-error"
        : missing.length
          ? "module-card-feedback is-warning"
          : "module-card-feedback is-ok";
      const launch = moduleInfo.id === "gdd"
        ? `<button class="ghost-btn" data-module-action="launch-gdd" data-module-id="gdd">Open GDD Flow</button>`
        : moduleInfo.id === "analysis"
          ? `<button class="ghost-btn" data-module-action="launch-analysis" data-module-id="analysis">Open Analysis Flow</button>`
          : moduleInfo.core
            ? ""
            : `<button class="ghost-btn" data-module-action="open-config" data-module-id="${moduleInfo.id}">Configure</button>`;

      return `<article class="module-card model-item" data-module-card="${moduleInfo.id}" data-module-state="${moduleStateKey(moduleInfo)}">
        <div class="module-card-header model-header">
          <div>
            <div class="model-name">${moduleInfo.name}</div>
            <div class="model-meta">ID: <code>${moduleInfo.id}</code> · v${moduleInfo.version}</div>
          </div>
          <span class="module-state-tag">${moduleInfo.core ? "Core" : moduleStateLabel(moduleInfo.state)}</span>
        </div>
        <div class="model-meta" title="${summaryTitle}">${summary}</div>
        <div class="module-card-desc">${escapeHtml(guide.description)}</div>
        <div class="module-card-usage">${escapeHtml(guide.usage)}</div>
        <div class="${feedbackClass}" title="${feedbackTitle}">${feedbackText}</div>
        <div class="module-card-actions model-actions">${cardActions(moduleInfo)} ${launch}</div>
      </article>`;
    })
    .join("\n");
}

async function refreshModuleState(): Promise<void> {
  const modules = await invoke<ModuleDescriptor[]>("list_modules");
  moduleSnapshot = modules;
  renderModulesList(modules);
  syncWorkflowAgentConsoleState();
  syncVoiceOutputConsoleState();
  if (dom.modulesStatus) {
    const active = modules.filter((moduleInfo) => moduleInfo.state === "active").length;
    dom.modulesStatus.textContent = `${active}/${modules.length} active`;
  }
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
    const confirmed = window.confirm(
      `Enable module '${moduleInfo.name}' and grant permissions: ${missing.join(", ")}?`
    );
    if (!confirmed) return;
  }

  try {
    const result = await invoke<{ restart_required?: boolean; message?: string }>("enable_module", {
      moduleId,
      grantPermissions: grants,
    });
    await refreshModuleState();
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
    showToast({
      type: entry.state === "ok" ? "success" : entry.state === "degraded" ? "warning" : "error",
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
  void refreshModuleState();
}

export function refreshModulesHub(): void {
  void refreshModuleState();
}
