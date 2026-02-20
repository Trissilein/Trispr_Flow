export type PanelId =
  | "transcription"
  | "capture"
  | "system"
  | "postprocessing"
  | "model"
  | "interface";

type ToggleOrigin = "user" | "sync";

const PANEL_IDS: PanelId[] = [
  "transcription",
  "capture",
  "system",
  "postprocessing",
  "model",
  "interface",
];

const INPUT_SOURCE_GROUP = "input-source";
const INPUT_SOURCE_PANEL_IDS: PanelId[] = ["capture", "system"];

export function isPanelId(value: string): value is PanelId {
  return PANEL_IDS.includes(value as PanelId);
}

function findPanel(panelId: PanelId): HTMLElement | null {
  return document.querySelector<HTMLElement>(`.panel[data-panel="${panelId}"]`);
}

function getPanelLabel(panel: HTMLElement, panelId: PanelId): string {
  return panel.querySelector(".panel-title h2")?.textContent?.trim() || panelId;
}

function updateCollapseButton(panel: HTMLElement, panelId: PanelId, collapsed: boolean): void {
  const button =
    panel.querySelector<HTMLButtonElement>(`[data-panel-collapse="${panelId}"]`) ||
    panel.querySelector<HTMLButtonElement>(".panel-collapse-btn");
  if (!button) return;

  const label = getPanelLabel(panel, panelId);
  button.setAttribute("aria-expanded", String(!collapsed));
  button.setAttribute("title", collapsed ? "Expand" : "Collapse");
  button.setAttribute(
    "aria-label",
    collapsed ? `Expand ${label} panel` : `Collapse ${label} panel`,
  );
}

function getInputSourcePeers(panelId: PanelId, panel: HTMLElement): PanelId[] {
  if (panel.dataset.panelGroup !== INPUT_SOURCE_GROUP) return [];
  if (!INPUT_SOURCE_PANEL_IDS.includes(panelId)) return [];
  return INPUT_SOURCE_PANEL_IDS.filter((id) => id !== panelId);
}

export function setPanelCollapsed(
  panelId: PanelId,
  collapsed: boolean,
  origin: ToggleOrigin = "user",
): void {
  const panel = findPanel(panelId);
  if (!panel) return;

  panel.classList.toggle("panel-collapsed", collapsed);
  updateCollapseButton(panel, panelId, collapsed);

  if (origin === "sync") return;
  for (const peerId of getInputSourcePeers(panelId, panel)) {
    setPanelCollapsed(peerId, collapsed, "sync");
  }
}

export function togglePanel(panelId: PanelId): void {
  const panel = findPanel(panelId);
  if (!panel) return;
  const collapsed = panel.classList.contains("panel-collapsed");
  setPanelCollapsed(panelId, !collapsed);
}

export function isPanelCollapsed(panelId: PanelId): boolean {
  const panel = findPanel(panelId);
  return panel?.classList.contains("panel-collapsed") ?? false;
}

function getDefaultCollapsed(panel: HTMLElement, panelId: PanelId): boolean {
  const fromData = panel.dataset.defaultCollapsed;
  if (fromData === "true") return true;
  if (fromData === "false") return false;
  return panelId !== "transcription";
}

export function initPanelState(): void {
  document.querySelectorAll<HTMLElement>(".panel[data-panel]").forEach((panel) => {
    const id = panel.dataset.panel;
    if (!id || !isPanelId(id)) return;
    setPanelCollapsed(id, getDefaultCollapsed(panel, id), "sync");
  });
}
