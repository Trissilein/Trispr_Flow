import { invoke } from "@tauri-apps/api/core";
import * as dom from "./dom-refs";
import { settings } from "./state";
import { showToast } from "./toast";
import { openGddFlow } from "./gdd-flow";
import type { ModuleDescriptor, ModuleHealthStatus, ModuleUpdateInfo } from "./types";

let initialized = false;
let moduleSnapshot: ModuleDescriptor[] = [];

function moduleStateLabel(moduleState: ModuleDescriptor["state"]): string {
  if (moduleState === "active") return "Active";
  if (moduleState === "installed") return "Installed";
  if (moduleState === "enabled") return "Enabled";
  if (moduleState === "not_installed") return "Not installed";
  return "Error";
}

function moduleStateClass(moduleState: ModuleDescriptor["state"]): string {
  if (moduleState === "active") return "model-status--active";
  if (moduleState === "installed" || moduleState === "enabled") return "model-status--available";
  if (moduleState === "not_installed") return "model-status--available";
  return "is-error";
}

function missingConsents(moduleInfo: ModuleDescriptor): string[] {
  const consented = settings?.module_settings?.consented_permissions?.[moduleInfo.id] || [];
  return moduleInfo.permissions.filter((permission) => !consented.includes(permission));
}

function cardActions(moduleInfo: ModuleDescriptor): string {
  const canEnable = moduleInfo.state === "installed" || moduleInfo.state === "error";
  const canDisable = moduleInfo.state === "active" || moduleInfo.state === "enabled";

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
      const dependencies = moduleInfo.dependencies.length
        ? moduleInfo.dependencies.map((dependency) => `<code>${dependency}</code>`).join(", ")
        : "None";
      const permissions = moduleInfo.permissions.length
        ? moduleInfo.permissions.map((permission) => `<code>${permission}</code>`).join(", ")
        : "None";
      const warning = moduleInfo.last_error
        ? `<div class="field-hint" style="color: #ff8a8a;">${moduleInfo.last_error}</div>`
        : "";
      const missing = missingConsents(moduleInfo);
      const consentNotice = missing.length
        ? `<div class="field-hint">Consent required: ${missing.map((permission) => `<code>${permission}</code>`).join(", ")}</div>`
        : "";
      const launch = moduleInfo.id === "gdd"
        ? `<button class="ghost-btn" data-module-action="launch-gdd" data-module-id="gdd">Open GDD Flow</button>`
        : moduleInfo.id === "analysis"
          ? `<button class="ghost-btn" data-module-action="launch-analysis" data-module-id="analysis">Open Analysis Flow</button>`
          : "";

      return `<article class="module-card" data-module-card="${moduleInfo.id}">
        <div class="module-card-header">
          <div>
            <h3>${moduleInfo.name}</h3>
            <div class="field-hint">ID: <code>${moduleInfo.id}</code> · v${moduleInfo.version}</div>
          </div>
          <span class="model-status ${moduleStateClass(moduleInfo.state)}">${moduleStateLabel(moduleInfo.state)}</span>
        </div>
        <div class="field-hint">Dependencies: ${dependencies}</div>
        <div class="field-hint">Permissions: ${permissions}</div>
        ${consentNotice}
        ${warning}
        <div class="module-card-actions">${cardActions(moduleInfo)} ${launch}</div>
      </article>`;
    })
    .join("\n");
}

async function refreshModuleState(): Promise<void> {
  const modules = await invoke<ModuleDescriptor[]>("list_modules");
  moduleSnapshot = modules;
  renderModulesList(modules);
  if (dom.modulesStatus) {
    const active = modules.filter((moduleInfo) => moduleInfo.state === "active").length;
    dom.modulesStatus.textContent = `${active}/${modules.length} active`;
  }
}

async function handleEnable(moduleId: string): Promise<void> {
  const moduleInfo = moduleSnapshot.find((candidate) => candidate.id === moduleId);
  if (!moduleInfo) return;

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
