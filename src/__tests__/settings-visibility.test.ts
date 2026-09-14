import { afterEach, describe, expect, it, vi } from "vitest";
import {
  SettingsVisibilityDefinition,
  SettingsVisibilityProfile,
  VisibilityProfileStorage,
  effectiveVisibility,
  emptyVisibilityProfile,
  exportVisibilityProfile,
  readVisibilityProfile,
  saveVisibilityProfile,
} from "../settings-visibility-profile";
import {
  SettingsVisibilityEntry,
  mountSettingsVisibilityEditor,
} from "../settings-visibility-editor";

class MemoryStorage implements VisibilityProfileStorage {
  private readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

const definitions: SettingsVisibilityDefinition[] = [
  { key: "capture.enabled", label: "Enable capture", group: "Capture", defaultVisibility: "basic" },
  { key: "capture.gain", label: "Microphone gain", group: "Capture", defaultVisibility: "expert" },
  { key: "output.alias", label: "History alias", group: "Text Output", defaultVisibility: "basic" },
];

function profile(
  overrides: Record<string, "basic" | "expert"> = {},
  removalCandidates: Record<string, { reason: string }> = {},
): SettingsVisibilityProfile {
  return { version: 1, overrides, removalCandidates };
}

function entry(
  definition: SettingsVisibilityDefinition,
  groupElement?: HTMLElement,
): SettingsVisibilityEntry {
  const element = document.createElement("input");
  element.dataset.settingsTestKey = definition.key;
  return { ...definition, element, groupElement };
}

function setupEditor(
  initialProfile: SettingsVisibilityProfile = emptyVisibilityProfile(),
  storage: VisibilityProfileStorage = new MemoryStorage(),
): {
  host: HTMLDivElement;
  storage: VisibilityProfileStorage;
  onCommit: ReturnType<typeof vi.fn>;
  handle: ReturnType<typeof mountSettingsVisibilityEditor>;
} {
  const host = document.createElement("div");
  document.body.append(host);
  const groupElement = document.createElement("section");
  groupElement.id = "capture-group";
  const entries = definitions.map((definition) =>
    entry(definition, definition.group === "Capture" ? groupElement : undefined),
  );
  host.append(groupElement);
  for (const candidate of entries) (candidate.groupElement ?? host).append(candidate.element);
  const onCommit = vi.fn();
  const handle = mountSettingsVisibilityEditor({
    host,
    entries,
    knownDefinitions: definitions,
    profile: initialProfile,
    storage,
    onCommit,
  });
  return { host, storage, onCommit, handle };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("settings visibility profile", () => {
  it("returns defaults and computes effective visibility", () => {
    const stored = new MemoryStorage();
    const empty = readVisibilityProfile(definitions, stored);

    expect(empty).toEqual(emptyVisibilityProfile());
    expect(effectiveVisibility(definitions[0], empty)).toBe("basic");
    expect(effectiveVisibility(definitions[1], empty)).toBe("expert");

    stored.setItem(
      "trispr-settings-visibility-v1",
      JSON.stringify(profile({ "capture.enabled": "basic" }, { "capture.gain": { reason: "Too technical" } })),
    );
    const loaded = readVisibilityProfile(definitions, stored);
    expect(loaded.overrides["capture.enabled"]).toBe("basic");
    expect(loaded.removalCandidates["capture.gain"].reason).toBe("Too technical");
  });

  it("rejects malformed schema and invalid enum/reason values while retaining valid dynamic keys", () => {
    const stored = new MemoryStorage();
    stored.setItem(
      "trispr-settings-visibility-v1",
      JSON.stringify({
        version: 1,
        overrides: {
          "capture.enabled": "expert",
          "capture.invalid": "sideways",
          "dynamic.module.field": "expert",
        },
        removalCandidates: {
          "capture.gain": { reason: "  keep this  " },
          "capture.empty": { reason: "   " },
          "dynamic.module.field": { reason: "dynamic is currently unmounted" },
        },
      }),
    );

    const loaded = readVisibilityProfile(definitions, stored);
    expect(loaded.overrides).toEqual({
      "capture.enabled": "expert",
      "dynamic.module.field": "expert",
    });
    expect(loaded.removalCandidates["capture.gain"]).toEqual({ reason: "keep this" });
    expect(loaded.removalCandidates["dynamic.module.field"]).toEqual({
      reason: "dynamic is currently unmounted",
    });

    stored.setItem("trispr-settings-visibility-v1", JSON.stringify({ version: 2 }));
    expect(readVisibilityProfile(definitions, stored)).toEqual(emptyVisibilityProfile());
    stored.setItem("trispr-settings-visibility-v1", "not-json");
    expect(readVisibilityProfile(definitions, stored)).toEqual(emptyVisibilityProfile());
  });

  it("saves explicit overrides equal to defaults and preserves dynamic candidates", () => {
    const stored = new MemoryStorage();
    const saved = saveVisibilityProfile(
      profile(
        { "capture.enabled": "basic", "dynamic.module.field": "expert" },
        { "dynamic.module.field": { reason: "Wait for module review" } },
      ),
      definitions,
      stored,
    );

    expect(saved.overrides["capture.enabled"]).toBe("basic");
    expect(saved.overrides["dynamic.module.field"]).toBe("expert");
    expect(readVisibilityProfile(definitions, stored)).toEqual(saved);
  });

  it("does not crash when read storage is unavailable, but save reports failure", () => {
    const failing: VisibilityProfileStorage = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    expect(readVisibilityProfile(definitions, failing)).toEqual(emptyVisibilityProfile());
    expect(() => saveVisibilityProfile(emptyVisibilityProfile(), definitions, failing)).toThrow("blocked");
  });

  it("exports defaults and metadata only", () => {
    const json = exportVisibilityProfile(
      profile(
        { "capture.enabled": "expert" },
        { "capture.gain": { reason: "  simplify this setting " } },
      ),
      definitions,
    );
    const exported = JSON.parse(json) as Record<string, unknown>;

    expect(exported).toEqual({
      version: 1,
      defaults: {
        "capture.enabled": "basic",
        "capture.gain": "expert",
        "output.alias": "basic",
      },
      overrides: { "capture.enabled": "expert" },
      removalCandidates: { "capture.gain": { reason: "simplify this setting" } },
    });
    expect(json).not.toContain("input-secret");
  });

  it("exports unknown dynamic metadata as unresolved until its definition exists", () => {
    const dynamicDefinition: SettingsVisibilityDefinition = {
      key: "dynamic.module.field",
      label: "Dynamic field",
      group: "Dynamic",
      defaultVisibility: "basic",
    };
    const dynamicProfile = profile(
      { "capture.enabled": "expert", [dynamicDefinition.key]: "expert" },
      {
        "capture.gain": { reason: "Review this known field" },
        [dynamicDefinition.key]: { reason: "Review after module mounts" },
      },
    );

    const beforeDefinition = JSON.parse(
      exportVisibilityProfile(dynamicProfile, definitions),
    ) as Record<string, unknown>;
    expect(beforeDefinition).toMatchObject({
      overrides: { "capture.enabled": "expert" },
      removalCandidates: { "capture.gain": { reason: "Review this known field" } },
      unresolvedOverrides: { [dynamicDefinition.key]: "expert" },
      unresolvedRemovalCandidates: {
        [dynamicDefinition.key]: { reason: "Review after module mounts" },
      },
    });

    const afterDefinition = JSON.parse(
      exportVisibilityProfile(dynamicProfile, [...definitions, dynamicDefinition]),
    ) as Record<string, unknown>;
    expect(afterDefinition).toMatchObject({
      overrides: {
        "capture.enabled": "expert",
        [dynamicDefinition.key]: "expert",
      },
      removalCandidates: {
        "capture.gain": { reason: "Review this known field" },
        [dynamicDefinition.key]: { reason: "Review after module mounts" },
      },
    });
    expect(afterDefinition).not.toHaveProperty("unresolvedOverrides");
    expect(afterDefinition).not.toHaveProperty("unresolvedRemovalCandidates");
  });
});

describe("settings visibility editor", () => {
  it("opens inline controls, shows mixed groups, and bulk updates a group", () => {
    const { host, handle } = setupEditor();
    const customize = host.querySelector<HTMLButtonElement>(
      ".settings-visibility-customize-button",
    );
    expect(customize).not.toBeNull();
    customize?.click();

    expect(host.dataset.settingsVisibilityEditing).toBe("true");
    expect(host.querySelector(".settings-visibility-editor")?.getAttribute(
      "data-settings-visibility-editor-owned",
    )).toBe("true");
    const originalField = host.querySelector('[data-settings-test-key="capture.enabled"]');
    expect(originalField?.nextElementSibling?.classList.contains("settings-visibility-entry")).toBe(true);
    expect(host.querySelectorAll(".settings-visibility-entry")).toHaveLength(3);
    expect(host.querySelector(".settings-visibility-panel")).toBeNull();
    expect((host.querySelector(".settings-visibility-reason-label") as HTMLElement | null)?.hidden).toBe(true);
    const groupSelect = host.querySelector<HTMLSelectElement>(
      ".settings-visibility-group-select",
    );
    expect(groupSelect?.value).toBe("");

    if (!groupSelect) throw new Error("group select missing");
    groupSelect.value = "basic";
    groupSelect.dispatchEvent(new Event("change", { bubbles: true }));
    expect(document.querySelector<HTMLSelectElement>('[aria-label="Enable capture visibility"]')?.value).toBe(
      "basic",
    );
    expect(host.querySelector<HTMLSelectElement>('[aria-label="Microphone gain visibility"]')?.value).toBe(
      "basic",
    );
    handle.destroy();
  });

  it("requires a removal reason, then commits draft and calls onCommit only after save", async () => {
    const { host, onCommit, storage, handle } = setupEditor();
    host.querySelector<HTMLButtonElement>(".settings-visibility-customize-button")?.click();

    const candidate = host.querySelector<HTMLInputElement>(
      '[aria-label="Reason for reviewing Microphone gain"]',
    );
    const candidateCheckbox = host.querySelector<HTMLInputElement>(
      '[data-settings-visibility-key="capture.gain"] input[type=checkbox]',
    );
    const done = host.querySelector<HTMLButtonElement>(".settings-visibility-done-button");
    expect(candidate).not.toBeNull();
    expect(candidateCheckbox).not.toBeNull();
    expect(done).not.toBeNull();

    if (!candidateCheckbox || !done || !candidate) throw new Error("editor controls missing");
    candidateCheckbox.checked = true;
    candidateCheckbox.dispatchEvent(new Event("change", { bubbles: true }));
    done.click();
    expect(onCommit).not.toHaveBeenCalled();
    expect(host.querySelector(".settings-visibility-error")?.textContent).toContain("required");

    candidate.value = "No longer needed";
    candidate.dispatchEvent(new Event("input", { bubbles: true }));
    done.click();
    await Promise.resolve();

    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(JSON.parse(storage.getItem("trispr-settings-visibility-v1") ?? "{}")).toMatchObject({
      removalCandidates: { "capture.gain": { reason: "No longer needed" } },
    });
    expect(host.dataset.settingsVisibilityEditing).toBeUndefined();
    handle.destroy();
  });

  it("restore defaults clears overrides but keeps candidates, while cancel discards the draft", () => {
    const { host, onCommit, handle } = setupEditor(
      profile({ "capture.enabled": "expert" }, { "capture.enabled": { reason: "Review" } }),
    );
    host.querySelector<HTMLButtonElement>(".settings-visibility-customize-button")?.click();

    host.querySelector<HTMLButtonElement>(".settings-visibility-restore-button")?.click();
    const visibility = host.querySelector<HTMLSelectElement>('[aria-label="Enable capture visibility"]');
    const candidate = host.querySelector<HTMLInputElement>(
      '[aria-label="Reason for reviewing Enable capture"]',
    );
    expect(visibility?.value).toBe("basic");
    expect(candidate?.value).toBe("Review");

    host.querySelector<HTMLButtonElement>(".settings-visibility-cancel-button")?.click();
    expect(onCommit).not.toHaveBeenCalled();
    expect(host.dataset.settingsVisibilityEditing).toBeUndefined();
    handle.destroy();
  });

  it("keeps a draft through refresh with rebuilt elements", () => {
    const { host, handle } = setupEditor();
    host.querySelector<HTMLButtonElement>(".settings-visibility-customize-button")?.click();
    const visibility = host.querySelector<HTMLSelectElement>('[aria-label="Enable capture visibility"]');
    if (!visibility) throw new Error("visibility select missing");
    visibility.value = "expert";
    visibility.dispatchEvent(new Event("change", { bubbles: true }));
    const candidate = host.querySelector<HTMLInputElement>(
      '[data-settings-visibility-key="capture.gain"] input[type=checkbox]',
    );
    const reason = host.querySelector<HTMLTextAreaElement>(
      '[aria-label="Reason for reviewing Microphone gain"]',
    );
    if (!candidate || !reason) throw new Error("candidate controls missing");
    candidate.checked = true;
    candidate.dispatchEvent(new Event("change", { bubbles: true }));
    reason.value = "Keep draft while Basic mode hides editor";
    reason.dispatchEvent(new Event("input", { bubbles: true }));

    const rebuilt = definitions.map((definition) => entry(definition));
    handle.refresh(rebuilt);
    expect(document.querySelector<HTMLSelectElement>('[aria-label="Enable capture visibility"]')?.value).toBe(
      "expert",
    );
    const refreshedCandidate = host.querySelector<HTMLInputElement>(
      '[data-settings-visibility-key="capture.gain"] input[type=checkbox]',
    );
    const refreshedReason = host.querySelector<HTMLTextAreaElement>(
      '[aria-label="Reason for reviewing Microphone gain"]',
    );
    expect(refreshedCandidate?.checked).toBe(true);
    expect(refreshedReason?.value).toBe("Keep draft while Basic mode hides editor");
    handle.destroy();
  });

  it("renders only fields in the active settings tab, excluding global header controls", () => {
    const host = document.createElement("div");
    const activeTab = document.createElement("div");
    activeTab.className = "main-tab-content active";
    const activeField = document.createElement("input");
    activeField.dataset.settingsTestKey = "active";
    activeTab.append(activeField);
    const globalField = document.createElement("button");
    globalField.dataset.settingsTestKey = "global";
    document.body.append(host, activeTab, globalField);

    const activeDefinition: SettingsVisibilityDefinition = {
      key: "active",
      label: "Active setting",
      group: "Capture",
      defaultVisibility: "expert",
    };
    const globalDefinition: SettingsVisibilityDefinition = {
      key: "global",
      label: "Global action",
      group: "Global",
      defaultVisibility: "expert",
    };
    const handle = mountSettingsVisibilityEditor({
      host,
      entries: [
        { ...activeDefinition, element: activeField },
        { ...globalDefinition, element: globalField },
      ],
      knownDefinitions: [activeDefinition, globalDefinition],
      profile: emptyVisibilityProfile(),
      onCommit: vi.fn(),
    });
    host.querySelector<HTMLButtonElement>(".settings-visibility-customize-button")?.click();

    expect(host.querySelector('[data-settings-visibility-key="active"]')).toBeNull();
    expect(activeField.nextElementSibling?.getAttribute("data-settings-visibility-key")).toBe("active");
    expect(globalField.nextElementSibling).toBeNull();
    handle.destroy();
  });

  it("refreshes inline rows across tabs without losing the draft", () => {
    const host = document.createElement("div");
    const transcriptionTab = document.createElement("section");
    transcriptionTab.className = "main-tab-content active";
    transcriptionTab.dataset.settingsTestTab = "transcription";
    const settingsTab = document.createElement("section");
    settingsTab.className = "main-tab-content";
    settingsTab.dataset.settingsTestTab = "settings";
    const transcriptionField = document.createElement("input");
    const settingsField = document.createElement("input");
    transcriptionField.dataset.settingsTestKey = "capture.enabled";
    settingsField.dataset.settingsTestKey = "output.alias";
    transcriptionTab.append(transcriptionField);
    settingsTab.append(settingsField);
    document.body.append(host, transcriptionTab, settingsTab);

    const transcriptionDefinition = definitions[0];
    const settingsDefinition = definitions[2];
    const entries: SettingsVisibilityEntry[] = [
      { ...transcriptionDefinition, element: transcriptionField },
      { ...settingsDefinition, element: settingsField },
    ];
    const handle = mountSettingsVisibilityEditor({
      host,
      entries,
      knownDefinitions: definitions,
      profile: emptyVisibilityProfile(),
      onCommit: vi.fn(),
    });
    host.querySelector<HTMLButtonElement>(".settings-visibility-customize-button")?.click();

    const transcriptionSelect = document.querySelector<HTMLSelectElement>(
      '[aria-label="Enable capture visibility"]',
    );
    if (!transcriptionSelect) throw new Error("transcription visibility select missing");
    transcriptionSelect.value = "expert";
    transcriptionSelect.dispatchEvent(new Event("change", { bubbles: true }));

    settingsTab.classList.add("active");
    transcriptionTab.classList.remove("active");
    handle.refresh(entries);
    expect(transcriptionField.nextElementSibling).toBeNull();
    expect(settingsField.nextElementSibling?.getAttribute("data-settings-visibility-key")).toBe(
      "output.alias",
    );

    settingsTab.classList.remove("active");
    transcriptionTab.classList.add("active");
    handle.refresh(entries);
    expect(transcriptionField.nextElementSibling?.getAttribute("data-settings-visibility-key")).toBe(
      "capture.enabled",
    );
    expect(document.querySelector<HTMLSelectElement>('[aria-label="Enable capture visibility"]')?.value).toBe(
      "expert",
    );
    handle.destroy();
  });

  it("shows storage errors and does not report a false save", () => {
    const failing: VisibilityProfileStorage = {
      getItem: () => null,
      setItem: () => {
        throw new Error("disk full");
      },
    };
    const { host, onCommit, handle } = setupEditor(emptyVisibilityProfile(), failing);
    host.querySelector<HTMLButtonElement>(".settings-visibility-customize-button")?.click();
    host.querySelector<HTMLButtonElement>(".settings-visibility-done-button")?.click();

    expect(onCommit).not.toHaveBeenCalled();
    expect(host.querySelector(".settings-visibility-error")?.textContent).toContain("disk full");
    expect(host.dataset.settingsVisibilityEditing).toBe("true");
    handle.destroy();
  });

  it("exports the committed profile while ignoring unsaved field values", async () => {
    const onExport = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const handle = mountSettingsVisibilityEditor({
      host,
      entries: [entry(definitions[0])],
      knownDefinitions: definitions,
      profile: profile({ "capture.enabled": "expert" }),
      onCommit: vi.fn(),
      onExport,
    });
    host.querySelector<HTMLButtonElement>(".settings-visibility-customize-button")?.click();
    const visibility = host.querySelector<HTMLSelectElement>('[aria-label="Enable capture visibility"]');
    if (!visibility) throw new Error("visibility select missing");
    visibility.value = "basic";
    visibility.dispatchEvent(new Event("change", { bubbles: true }));
    host.querySelector<HTMLButtonElement>(".settings-visibility-export-button")?.click();
    await Promise.resolve();

    expect(onExport).toHaveBeenCalledTimes(1);
    const exported = JSON.parse(onExport.mock.calls[0][0] as string) as Record<string, unknown>;
    expect((exported.overrides as Record<string, string>)["capture.enabled"]).toBe("expert");
    handle.destroy();
  });

  it("renders one row for a composite parent while keeping an opted-in nested field", () => {
    const host = document.createElement("div");
    const composite = document.createElement("div");
    composite.dataset.settingsVisibilityManaged = "true";
    composite.dataset.settingsVisibilityKey = "ai.prompt_preset";
    const compositeChild = document.createElement("button");
    compositeChild.dataset.settingsVisibilityManaged = "true";
    compositeChild.dataset.settingsVisibilityKey = "ai.prompt_preset.child";
    const independentChild = document.createElement("input");
    independentChild.dataset.settingsVisibilityManaged = "true";
    independentChild.dataset.settingsVisibilityKey = "ai.prompt_preset.independent";
    independentChild.setAttribute("data-settings-visibility-independent", "true");
    composite.append(compositeChild, independentChild);
    document.body.append(host, composite);

    const definitions: SettingsVisibilityDefinition[] = [
      { key: "ai.prompt_preset", label: "Prompt style", group: "AI Refinement", defaultVisibility: "basic" },
      { key: "ai.prompt_preset.child", label: "Prompt child action", group: "AI Refinement", defaultVisibility: "basic" },
      { key: "ai.prompt_preset.independent", label: "Independent nested setting", group: "AI Refinement", defaultVisibility: "expert" },
    ];
    const handle = mountSettingsVisibilityEditor({
      host,
      entries: [
        { ...definitions[0], element: composite },
        { ...definitions[1], element: compositeChild },
        { ...definitions[2], element: independentChild },
      ],
      knownDefinitions: definitions,
      profile: emptyVisibilityProfile(),
      onCommit: vi.fn(),
    });

    host.querySelector<HTMLButtonElement>(".settings-visibility-customize-button")?.click();

    expect(document.querySelectorAll(".settings-visibility-entry")).toHaveLength(2);
    expect(
      document
        .querySelector('.settings-visibility-entry[data-settings-visibility-key="ai.prompt_preset"]')
        ?.getAttribute("data-settings-visibility-label"),
    ).toBe("Prompt style");
    expect(document.querySelector('.settings-visibility-entry[data-settings-visibility-key="ai.prompt_preset.child"]')).toBeNull();
    expect(
      document
        .querySelector('.settings-visibility-entry[data-settings-visibility-key="ai.prompt_preset.independent"]')
        ?.getAttribute("data-settings-visibility-label"),
    ).toBe("Independent nested setting");
    handle.destroy();
  });
});
