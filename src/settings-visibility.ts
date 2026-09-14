import {
  effectiveVisibility,
  readVisibilityProfile,
  type SettingsVisibilityDefinition,
  type SettingsVisibilityProfile,
} from "./settings-visibility-profile";
import {
  mountSettingsVisibilityEditor,
  type SettingsVisibilityEntry,
} from "./settings-visibility-editor";

const MANAGED_SELECTOR = '[data-settings-visibility-managed="true"]';
const EDITOR_HOST_ID = "settings-visibility-editor-host";

type EditorMount = {
  destroy(): void;
  refresh?(entries: SettingsVisibilityEntry[], profile: SettingsVisibilityProfile): void;
};

let profile: SettingsVisibilityProfile | null = null;
let editor: EditorMount | null = null;

function visibility(value: string | undefined): "basic" | "expert" {
  return value === "basic" ? "basic" : "expert";
}

function labelFor(element: HTMLElement): string {
  const explicit = element.dataset.settingsVisibilityLabel?.trim();
  if (explicit) return explicit;
  const fieldLabel = element.querySelector<HTMLElement>(".field-label")?.textContent?.trim();
  if (fieldLabel) return fieldLabel;
  return element.getAttribute("aria-label")?.trim() || element.id || element.dataset.settingsVisibilityKey || "Setting";
}

export function getSettingsVisibilityEntries(root: ParentNode = document): SettingsVisibilityEntry[] {
  return Array.from(root.querySelectorAll<HTMLElement>(MANAGED_SELECTOR))
    .filter((element) => !element.closest('[data-settings-visibility-editor-owned="true"]'))
    .map<SettingsVisibilityEntry | null>((element) => {
      const key = element.dataset.settingsVisibilityKey?.trim();
      if (!key) return null;
      const definition: SettingsVisibilityDefinition = {
        key,
        label: labelFor(element),
        group: element.dataset.settingsVisibilityGroup?.trim() || "Settings",
        defaultVisibility: visibility(element.dataset.settingsVisibilityDefault),
      };
      const groupElement = element.closest<HTMLElement>("[data-settings-visibility-group-container]");
      return {
        ...definition,
        element,
        ...(groupElement ? { groupElement } : {}),
      };
    })
    .filter((entry): entry is SettingsVisibilityEntry => entry !== null);
}

function definitions(entries: SettingsVisibilityEntry[]): SettingsVisibilityDefinition[] {
  const seen = new Set<string>();
  return entries.filter((entry) => {
    if (seen.has(entry.key)) return false;
    seen.add(entry.key);
    return true;
  }).map(({ key, label, group, defaultVisibility }) => ({ key, label, group, defaultVisibility }));
}

function isBasicSurface(): boolean {
  return document.documentElement.classList.contains("standard-mode");
}

function isRuntimeHidden(element: HTMLElement): boolean {
  for (let node: HTMLElement | null = element; node; node = node.parentElement) {
    if (node.hidden || node.getAttribute("aria-hidden") === "true" || node.style.display === "none") return true;
  }
  return false;
}

function restoreLegacyExpertBlockers(): void {
  document.querySelectorAll<HTMLElement>('[data-settings-visibility-released-expert="true"]').forEach((element) => {
    element.setAttribute("data-expert-only", "true");
    delete element.dataset.settingsVisibilityReleasedExpert;
  });
}

function releaseBasicAncestors(entry: SettingsVisibilityEntry): void {
  if (entry.element.getAttribute("data-expert-only") === "true") {
    entry.element.removeAttribute("data-expert-only");
    entry.element.dataset.settingsVisibilityReleasedExpert = "true";
  }
  for (
    let node: HTMLElement | null = entry.element.parentElement;
    node && !node.matches(".main-tab-content");
    node = node.parentElement
  ) {
    if (node.getAttribute("data-expert-only") !== "true") continue;
    node.removeAttribute("data-expert-only");
    node.dataset.settingsVisibilityReleasedExpert = "true";
  }
}

function applyTabVisibility(entries: SettingsVisibilityEntry[]): void {
  const byTab = new Map<string, SettingsVisibilityEntry[]>();
  for (const entry of entries) {
    const tab = entry.element.closest<HTMLElement>("[data-settings-visibility-tab-content]")
      ?.dataset.settingsVisibilityTabContent;
    if (!tab) continue;
    const bucket = byTab.get(tab) ?? [];
    bucket.push(entry);
    byTab.set(tab, bucket);
  }

  document.querySelectorAll<HTMLElement>("[data-settings-visibility-tab]").forEach((button) => {
    const tab = button.dataset.settingsVisibilityTab;
    const entriesForTab = tab ? byTab.get(tab) ?? [] : [];
    const hasBasic = entriesForTab.some(
      (entry) => profile && effectiveVisibility(entry, profile) === "basic" && !isRuntimeHidden(entry.element),
    );
    button.dataset.settingsVisibilityNoBasic = isBasicSurface() && !hasBasic ? "true" : "false";
    const content = tab ? document.querySelector<HTMLElement>(`[data-settings-visibility-tab-content="${tab}"]`) : null;
    if (content) content.dataset.settingsVisibilityNoBasic = button.dataset.settingsVisibilityNoBasic;
  });
}

function applyGroupVisibility(entries: SettingsVisibilityEntry[]): void {
  document.querySelectorAll<HTMLElement>("[data-settings-visibility-group-container]").forEach((group) => {
    const groupEntries = entries.filter((entry) => group.contains(entry.element));
    const hasBasic = groupEntries.some(
      (entry) => profile && effectiveVisibility(entry, profile) === "basic" && !isRuntimeHidden(entry.element),
    );
    group.dataset.settingsVisibilityNoBasic = isBasicSurface() && !hasBasic ? "true" : "false";
  });
}

export function applySettingsVisibility(): void {
  const entries = getSettingsVisibilityEntries();
  if (!profile) profile = readVisibilityProfile(definitions(entries));
  const activeProfile = profile;
  const basicSurface = isBasicSurface();
  restoreLegacyExpertBlockers();
  for (const entry of entries) {
    const effective = effectiveVisibility(entry, activeProfile);
    if (basicSurface && effective === "basic") releaseBasicAncestors(entry);
    const hasBasicDescendant = entries.some(
      (candidate) => candidate !== entry
        && effectiveVisibility(candidate, activeProfile) === "basic"
        && entry.element.contains(candidate.element),
    );
    const hidden = basicSurface && effective === "expert" && !hasBasicDescendant;
    entry.element.dataset.settingsVisibilityHidden = hidden ? "true" : "false";
  }
  applyGroupVisibility(entries);
  applyTabVisibility(entries);
  window.dispatchEvent(new CustomEvent("settings-visibility:applied"));
}

function mountEditor(): void {
  const host = document.getElementById(EDITOR_HOST_ID);
  if (!host) return;
  const entries = getSettingsVisibilityEntries();
  const current = profile ?? readVisibilityProfile(definitions(entries));
  profile = current;
  if (editor?.refresh) {
    editor.refresh(entries, current);
    return;
  }
  if (host.dataset.settingsVisibilityEditing === "true") return;
  editor?.destroy();
  editor = mountSettingsVisibilityEditor({
    host,
    entries,
    knownDefinitions: definitions(entries),
    profile: current,
    onCommit(nextProfile) {
      profile = nextProfile;
      applySettingsVisibility();
    },
  });
}

export function refreshSettingsVisibilityEditor(): void {
  applySettingsVisibility();
  mountEditor();
}

export function initSettingsVisibility(): void {
  const entries = getSettingsVisibilityEntries();
  profile ??= readVisibilityProfile(definitions(entries));
  applySettingsVisibility();
  mountEditor();
}
