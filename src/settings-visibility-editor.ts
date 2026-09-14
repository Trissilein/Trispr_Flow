import {
  MAX_REMOVAL_REASON_LENGTH,
  SETTINGS_VISIBILITY_EXPORT_FILENAME,
  SettingsVisibilityDefinition,
  SettingsVisibilityProfile,
  Visibility,
  VisibilityProfileStorage,
  effectiveVisibility,
  exportVisibilityProfile,
  isVisibilityValue,
  saveVisibilityProfile,
} from "./settings-visibility-profile";

export interface SettingsVisibilityEntry extends SettingsVisibilityDefinition {
  element: HTMLElement;
  groupElement?: HTMLElement;
}

export interface MountSettingsVisibilityEditorOptions {
  host: HTMLElement;
  entries: readonly SettingsVisibilityEntry[];
  profile: SettingsVisibilityProfile;
  onCommit: (profile: SettingsVisibilityProfile) => void;
  /** Complete catalog, including dynamic definitions not currently mounted. */
  knownDefinitions?: readonly SettingsVisibilityDefinition[];
  storage?: VisibilityProfileStorage;
  onExport?: (json: string) => void | Promise<void>;
}

export interface SettingsVisibilityEditorHandle {
  destroy(): void;
  refresh(
    entries: readonly SettingsVisibilityEntry[],
    profile?: SettingsVisibilityProfile,
  ): void;
}

export const SETTINGS_VISIBILITY_EDITOR_CLASSES = {
  root: "settings-visibility-editor",
  customizeButton: "settings-visibility-customize-button",
  toolbar: "settings-visibility-toolbar",
  group: "settings-visibility-group",
  groupTitle: "settings-visibility-group-title",
  groupSelect: "settings-visibility-group-select",
  entry: "settings-visibility-entry",
  entryLabel: "settings-visibility-entry-label",
  visibilitySelect: "settings-visibility-visibility-select",
  resetButton: "settings-visibility-reset-button",
  candidate: "settings-visibility-candidate",
  reasonLabel: "settings-visibility-reason-label",
  reason: "settings-visibility-reason",
  error: "settings-visibility-error",
  status: "settings-visibility-status",
  exportButton: "settings-visibility-export-button",
} as const;

const OWNED_ATTRIBUTE = "data-settings-visibility-editor-owned";
const MANAGED_SELECTOR = '[data-settings-visibility-managed="true"]';
const INDEPENDENT_NESTED_ATTRIBUTE = "data-settings-visibility-independent";

type GroupState = {
  entries: SettingsVisibilityEntry[];
  groupElement?: HTMLElement;
};

function hasOwn(value: object, key: PropertyKey): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function cloneProfile(profile: SettingsVisibilityProfile): SettingsVisibilityProfile {
  return {
    version: 1,
    overrides: Object.fromEntries(Object.entries(profile.overrides)),
    removalCandidates: Object.fromEntries(
      Object.entries(profile.removalCandidates).map(([key, candidate]) => [key, { reason: candidate.reason }]),
    ),
  };
}

function definitionFromEntry(entry: SettingsVisibilityEntry): SettingsVisibilityDefinition {
  return {
    key: entry.key,
    label: entry.label,
    group: entry.group,
    defaultVisibility: entry.defaultVisibility,
  };
}

function mergeDefinitions(
  definitions: readonly SettingsVisibilityDefinition[],
  entries: readonly SettingsVisibilityEntry[],
): SettingsVisibilityDefinition[] {
  const result: SettingsVisibilityDefinition[] = [];
  const seen = new Set<string>();
  for (const definition of definitions) {
    if (seen.has(definition.key)) continue;
    seen.add(definition.key);
    result.push({ ...definition });
  }
  for (const entry of entries) {
    if (seen.has(entry.key)) continue;
    seen.add(entry.key);
    result.push(definitionFromEntry(entry));
  }
  return result;
}

function uniqueEntries(entries: readonly SettingsVisibilityEntry[]): SettingsVisibilityEntry[] {
  const result: SettingsVisibilityEntry[] = [];
  const seen = new Set<string>();
  for (const entry of entries) {
    if (seen.has(entry.key)) continue;
    seen.add(entry.key);
    result.push(entry);
  }
  return result;
}

function editorEntries(entries: readonly SettingsVisibilityEntry[]): SettingsVisibilityEntry[] {
  const activeTab = document.querySelector<HTMLElement>(".main-tab-content.active");
  const scoped = activeTab ? entries.filter((entry) => activeTab.contains(entry.element)) : entries;
  return uniqueEntries(
    scoped.filter((entry) => {
      const managedAncestor = entry.element.parentElement?.closest<HTMLElement>(MANAGED_SELECTOR);
      return !managedAncestor || entry.element.getAttribute(INDEPENDENT_NESTED_ATTRIBUTE) === "true";
    }),
  );
}

function slug(value: string): string {
  const normalized = value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return normalized || "field";
}

function makeButton(className: string, text: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = className;
  button.textContent = text;
  return button;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Unknown error";
}

function markOwned(node: HTMLElement): HTMLElement {
  node.dataset.settingsVisibilityEditorOwned = "true";
  return node;
}

function stopEditorEvents(node: HTMLElement): void {
  const stop = (event: Event): void => event.stopPropagation();
  node.addEventListener("click", stop);
  node.addEventListener("change", stop);
  node.addEventListener("input", stop);
}

function insertAfter(target: HTMLElement, node: HTMLElement, fallback: HTMLElement): void {
  const parent = target.parentElement;
  const inlineParent = parent && /^(LABEL|SPAN|A|P)$/.test(parent.tagName) ? parent : null;
  const anchor = inlineParent ?? target;
  if (anchor.parentNode) {
    anchor.parentNode.insertBefore(node, anchor.nextSibling);
  } else {
    fallback.append(node);
  }
}

function insertGroupControl(
  groupElement: HTMLElement | undefined,
  firstEntry: HTMLElement | undefined,
  node: HTMLElement,
  fallback: HTMLElement,
): void {
  if (groupElement && groupElement.parentNode) {
    const isControl = /^(INPUT|SELECT|TEXTAREA|BUTTON|LABEL)$/.test(groupElement.tagName);
    if (isControl) {
      groupElement.parentNode.insertBefore(node, groupElement);
    } else {
      groupElement.insertBefore(node, groupElement.firstChild);
    }
    return;
  }
  if (firstEntry?.parentNode) {
    const parent = firstEntry.parentElement;
    const inlineParent = parent && /^(LABEL|SPAN|A|P)$/.test(parent.tagName) ? parent : null;
    (inlineParent ?? firstEntry).parentNode?.insertBefore(node, inlineParent ?? firstEntry);
    return;
  }
  fallback.append(node);
}

function triggerMetadataDownload(json: string): void {
  if (typeof URL.createObjectURL !== "function") {
    throw new Error("Visibility profile export is unavailable in this browser.");
  }
  const url = URL.createObjectURL(new Blob([json], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = SETTINGS_VISIBILITY_EXPORT_FILENAME;
  link.click();
  URL.revokeObjectURL(url);
}

export function mountSettingsVisibilityEditor(
  options: MountSettingsVisibilityEditorOptions,
): SettingsVisibilityEditorHandle {
  let entries = editorEntries(options.entries);
  let knownDefinitions = mergeDefinitions(options.knownDefinitions ?? [], options.entries);
  let committedProfile = cloneProfile(options.profile);
  let draftProfile: SettingsVisibilityProfile | null = null;
  let editing = false;
  let destroyed = false;
  let committing = false;
  let toolbar: HTMLElement | null = null;
  let errorEl: HTMLElement | null = null;
  let statusEl: HTMLElement | null = null;
  let doneButton: HTMLButtonElement | null = null;
  const ownedNodes = new Set<HTMLElement>();
  let groupControls = new Map<string, HTMLSelectElement>();
  let visibilityControls = new Map<string, HTMLSelectElement>();
  let resetControls = new Map<string, HTMLButtonElement>();
  let candidateControls = new Map<string, HTMLInputElement>();
  let reasonLabels = new Map<string, HTMLLabelElement>();
  let reasonControls = new Map<string, HTMLTextAreaElement>();

  const root = document.createElement("div");
  root.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.root;
  root.setAttribute(OWNED_ATTRIBUTE, "true");
  root.setAttribute("aria-label", "Settings visibility");
  const customizeButton = makeButton(
    SETTINGS_VISIBILITY_EDITOR_CLASSES.customizeButton,
    "Customize view",
  );
  root.append(customizeButton);
  options.host.append(root);

  function setError(message: string): void {
    if (!errorEl) return;
    errorEl.textContent = message;
    errorEl.hidden = message.length === 0;
  }

  function setStatus(message: string): void {
    if (!statusEl) return;
    statusEl.textContent = message;
  }

  function currentDraft(): SettingsVisibilityProfile {
    if (!draftProfile) throw new Error("Visibility editor is not open.");
    return draftProfile;
  }

  function visibilityFor(entry: SettingsVisibilityEntry): Visibility {
    return effectiveVisibility(entry, currentDraft());
  }

  function clearOwnedNodes(): void {
    for (const node of ownedNodes) node.remove();
    ownedNodes.clear();
    toolbar = null;
    errorEl = null;
    statusEl = null;
    doneButton = null;
    groupControls = new Map();
    visibilityControls = new Map();
    resetControls = new Map();
    candidateControls = new Map();
    reasonLabels = new Map();
    reasonControls = new Map();
  }

  function closeEditor(): void {
    editing = false;
    committing = false;
    draftProfile = null;
    options.host.removeAttribute("data-settings-visibility-editing");
    clearOwnedNodes();
    customizeButton.focus();
  }

  function setReasonVisibility(entry: SettingsVisibilityEntry): void {
    const draft = currentDraft();
    const candidate = hasOwn(draft.removalCandidates, entry.key)
      ? draft.removalCandidates[entry.key]
      : undefined;
    const reasonLabel = reasonLabels.get(entry.key);
    const reason = reasonControls.get(entry.key);
    if (reasonLabel) reasonLabel.hidden = candidate === undefined;
    if (reason) {
      reason.hidden = candidate === undefined;
      reason.setAttribute("aria-hidden", String(candidate === undefined));
      reason.value = candidate?.reason ?? "";
    }
  }

  function syncFieldControl(entry: SettingsVisibilityEntry): void {
    const draft = currentDraft();
    const visibility = visibilityControls.get(entry.key);
    if (visibility) visibility.value = visibilityFor(entry);
    const reset = resetControls.get(entry.key);
    if (reset) reset.disabled = !hasOwn(draft.overrides, entry.key);
    const candidate = candidateControls.get(entry.key);
    if (candidate) candidate.checked = hasOwn(draft.removalCandidates, entry.key);
    setReasonVisibility(entry);
  }

  function syncControls(groups: Map<string, GroupState>): void {
    for (const [group, groupState] of groups) {
      const select = groupControls.get(group);
      if (!select) continue;
      const values = groupState.entries.map((entry) => visibilityFor(entry));
      select.value = values.every((value) => value === values[0]) ? values[0] : "";
    }
    for (const entry of entries) syncFieldControl(entry);
  }

  function renderEditor(): void {
    if (!editing) return;
    const draft = currentDraft();
    clearOwnedNodes();

    toolbar = markOwned(document.createElement("div"));
    toolbar.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.toolbar;
    toolbar.setAttribute("role", "toolbar");
    toolbar.setAttribute("aria-label", "Customize settings view");
    stopEditorEvents(toolbar);
    const done = makeButton("settings-visibility-done-button", "Done");
    const cancel = makeButton("settings-visibility-cancel-button", "Cancel");
    const restore = makeButton("settings-visibility-restore-button", "Restore defaults");
    const exportButton = makeButton(
      SETTINGS_VISIBILITY_EDITOR_CLASSES.exportButton,
      "Export saved view",
    );
    doneButton = done;
    toolbar.append(done, cancel, restore, exportButton);
    statusEl = document.createElement("span");
    statusEl.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.status;
    statusEl.setAttribute("aria-live", "polite");
    errorEl = document.createElement("div");
    errorEl.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.error;
    errorEl.setAttribute("role", "alert");
    errorEl.hidden = true;
    toolbar.append(statusEl, errorEl);
    options.host.append(toolbar);
    ownedNodes.add(toolbar);

    const groups = new Map<string, GroupState>();
    for (const entry of entries) {
      const group = groups.get(entry.group);
      if (group) group.entries.push(entry);
      else groups.set(entry.group, { entries: [entry], groupElement: entry.groupElement });
    }
    groupControls = new Map();
    visibilityControls = new Map();
    resetControls = new Map();
    candidateControls = new Map();
    reasonLabels = new Map();
    reasonControls = new Map();

    let groupIndex = 0;
    for (const [groupName, groupState] of groups) {
      const groupControl = markOwned(document.createElement("div"));
      groupControl.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.group;
      groupControl.dataset.settingsVisibilityGroup = groupName;
      groupControl.setAttribute(OWNED_ATTRIBUTE, "true");
      stopEditorEvents(groupControl);
      const title = document.createElement("span");
      title.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.groupTitle;
      title.textContent = `Group: ${groupName}`;
      const groupSelect = document.createElement("select");
      const groupSelectId = `settings-visibility-group-${slug(groupName)}-${groupIndex}`;
      groupSelect.id = groupSelectId;
      groupSelect.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.groupSelect;
      const groupLabel = document.createElement("label");
      groupLabel.htmlFor = groupSelectId;
      groupLabel.textContent = `Set ${groupName} visibility`;
      groupSelect.setAttribute("aria-label", `Set ${groupName} visibility`);
      for (const [value, label] of [["", "Mixed"], ["basic", "Basic"], ["expert", "Expert only"]] as const) {
        const option = document.createElement("option");
        option.value = value;
        option.textContent = label;
        groupSelect.append(option);
      }
      groupControls.set(groupName, groupSelect);
      groupSelect.addEventListener("change", (event) => {
        event.stopPropagation();
        if (!isVisibilityValue(groupSelect.value)) return;
        for (const entry of groupState.entries) draft.overrides[entry.key] = groupSelect.value;
        setError("");
        setStatus("");
        syncControls(groups);
      });
      groupControl.append(title, groupLabel, groupSelect);
      insertGroupControl(
        groupState.groupElement,
        groupState.entries[0]?.element,
        groupControl,
        options.host,
      );
      ownedNodes.add(groupControl);

      const seen = new Set<string>();
      for (const entry of groupState.entries) {
        if (seen.has(entry.key)) continue;
        seen.add(entry.key);
        const row = markOwned(document.createElement("div"));
        row.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.entry;
        row.dataset.settingsVisibilityKey = entry.key;
        row.setAttribute(OWNED_ATTRIBUTE, "true");
        stopEditorEvents(row);

        const fieldId = `settings-visibility-field-${slug(entry.key)}-${groupIndex}`;
        const visibility = document.createElement("select");
        visibility.id = `${fieldId}-visibility`;
        visibility.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.visibilitySelect;
        visibility.setAttribute("aria-label", `${entry.label} visibility`);
        for (const [value, label] of [["basic", "Basic"], ["expert", "Expert only"]] as const) {
          const option = document.createElement("option");
          option.value = value;
          option.textContent = label;
          visibility.append(option);
        }
        visibilityControls.set(entry.key, visibility);
        visibility.addEventListener("change", (event) => {
          event.stopPropagation();
          if (!isVisibilityValue(visibility.value)) return;
          draft.overrides[entry.key] = visibility.value;
          setError("");
          setStatus("");
          syncControls(groups);
        });

        const reset = makeButton(SETTINGS_VISIBILITY_EDITOR_CLASSES.resetButton, "Reset");
        reset.setAttribute("aria-label", `Reset ${entry.label} visibility`);
        reset.addEventListener("click", (event) => {
          event.stopPropagation();
          delete draft.overrides[entry.key];
          setError("");
          setStatus("");
          syncControls(groups);
        });
        resetControls.set(entry.key, reset);

        const candidateLabel = document.createElement("label");
        candidateLabel.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.candidate;
        const candidate = document.createElement("input");
        candidate.type = "checkbox";
        candidate.id = `${fieldId}-candidate`;
        candidateLabel.htmlFor = candidate.id;
        candidateLabel.append(candidate, document.createTextNode(" Review for removal"));
        candidateControls.set(entry.key, candidate);
        candidate.addEventListener("change", (event) => {
          event.stopPropagation();
          if (candidate.checked) {
            draft.removalCandidates[entry.key] = {
              reason: draft.removalCandidates[entry.key]?.reason ?? "",
            };
          } else {
            delete draft.removalCandidates[entry.key];
          }
          setError("");
          setStatus("");
          syncControls(groups);
        });

        const reasonLabel = document.createElement("label");
        reasonLabel.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.reasonLabel;
        reasonLabel.htmlFor = `${fieldId}-reason`;
        reasonLabel.textContent = `Reason for reviewing ${entry.label}`;
        const reason = document.createElement("textarea");
        reason.id = `${fieldId}-reason`;
        reason.className = SETTINGS_VISIBILITY_EDITOR_CLASSES.reason;
        reason.maxLength = MAX_REMOVAL_REASON_LENGTH;
        reason.rows = 2;
        reason.setAttribute("aria-label", `Reason for reviewing ${entry.label}`);
        reasonControls.set(entry.key, reason);
        reasonLabels.set(entry.key, reasonLabel);
        reason.addEventListener("input", (event) => {
          event.stopPropagation();
          if (!candidate.checked) return;
          draft.removalCandidates[entry.key] = { reason: reason.value };
          setError("");
          setStatus("");
        });

        row.dataset.settingsVisibilityLabel = entry.label;
        row.append(visibility, reset, candidateLabel, reasonLabel, reason);
        insertAfter(entry.element, row, options.host);
        ownedNodes.add(row);
      }
      groupIndex += 1;
    }

    done.addEventListener("click", (event) => {
      event.stopPropagation();
      commitDraft();
    });
    cancel.addEventListener("click", (event) => {
      event.stopPropagation();
      closeEditor();
    });
    restore.addEventListener("click", (event) => {
      event.stopPropagation();
      draft.overrides = Object.create(null) as Record<string, Visibility>;
      setError("");
      setStatus("Defaults restored in draft");
      syncControls(groups);
    });
    exportButton.addEventListener("click", (event) => {
      event.stopPropagation();
      void exportCommittedProfile();
    });

    syncControls(groups);
  }

  function commitDraft(): void {
    if (!editing || !draftProfile || !doneButton || committing) return;
    const invalid = Object.entries(draftProfile.removalCandidates).find(
      ([, candidate]) => candidate.reason.trim().length === 0,
    );
    if (invalid) {
      const entry = entries.find((candidate) => candidate.key === invalid[0]);
      setError(`Review reason required${entry ? ` for ${entry.label}` : ""}.`);
      reasonControls.get(invalid[0])?.focus();
      return;
    }

    const activeDone = doneButton;
    committing = true;
    activeDone.disabled = true;
    setError("");
    setStatus("Saving…");
    try {
      const saved = saveVisibilityProfile(draftProfile, knownDefinitions, options.storage);
      options.onCommit(saved);
      committedProfile = cloneProfile(saved);
      closeEditor();
    } catch (error) {
      committing = false;
      if (editing && doneButton === activeDone) {
        activeDone.disabled = false;
        setStatus("");
        setError(`Could not save settings visibility: ${errorMessage(error)}`);
      }
    }
  }

  async function exportCommittedProfile(): Promise<void> {
    if (!editing) return;
    setError("");
    try {
      const json = exportVisibilityProfile(committedProfile, knownDefinitions);
      if (options.onExport) await options.onExport(json);
      else triggerMetadataDownload(json);
      setStatus("Exported");
    } catch (error) {
      setStatus("");
      setError(`Could not export settings visibility: ${errorMessage(error)}`);
    }
  }

  function openEditor(): void {
    if (destroyed || editing) return;
    editing = true;
    draftProfile = cloneProfile(committedProfile);
    options.host.dataset.settingsVisibilityEditing = "true";
    renderEditor();
  }

  customizeButton.addEventListener("click", (event) => {
    event.stopPropagation();
    openEditor();
  });

  return {
    destroy(): void {
      if (destroyed) return;
      destroyed = true;
      closeEditor();
      root.remove();
    },
    refresh(nextEntries, profile): void {
      if (destroyed) return;
      entries = editorEntries(nextEntries);
      knownDefinitions = mergeDefinitions(knownDefinitions, nextEntries);
      if (!editing && profile) committedProfile = cloneProfile(profile);
      if (editing) renderEditor();
    },
  };
}
