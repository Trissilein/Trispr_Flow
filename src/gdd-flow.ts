import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import * as dom from "./dom-refs";
import { buildConversationHistory, buildConversationText } from "./history";
import { focusFirstElement, trapFocusInModal } from "./modal-focus";
import { settings } from "./state";
import { showToast } from "./toast";
import type {
  ConfluenceTargetSuggestion,
  GddDraft,
  GddPreset,
  GddPublishResult,
  GddRecognitionResult,
  GddTemplateSourceResult,
  GenerateGddDraftRequest,
} from "./types";

let initialized = false;
let loadedTemplate: GddTemplateSourceResult | null = null;
let lastDraft: GddDraft | null = null;
let lastFocusedBeforeOpen: HTMLElement | null = null;

function effectiveDraftTitle(): string {
  return (
    dom.gddFlowTitle?.value?.trim() ||
    lastDraft?.title?.trim() ||
    "Game Design Document"
  );
}

function resetPublishLink(): void {
  if (!dom.gddFlowPublishLink) return;
  dom.gddFlowPublishLink.hidden = true;
  dom.gddFlowPublishLink.href = "#";
}

function toPreviewText(draft: GddDraft): string {
  const lines: string[] = [
    `# ${draft.title}`,
    "",
    `Preset: ${draft.preset_id}`,
    `Generated: ${draft.generated_at_iso}`,
    "",
    draft.summary,
    "",
  ];

  for (const section of draft.sections) {
    lines.push(`## ${section.title}`);
    lines.push(section.content);
    lines.push("");
  }

  return lines.join("\n");
}

function setStatus(message: string): void {
  if (dom.gddFlowStatus) {
    dom.gddFlowStatus.textContent = message;
  }
}

function setTemplateResult(result: GddTemplateSourceResult | null): void {
  loadedTemplate = result;
  if (!result) {
    if (dom.gddFlowTemplateMeta) dom.gddFlowTemplateMeta.textContent = "No template loaded.";
    if (dom.gddFlowTemplatePreview) dom.gddFlowTemplatePreview.value = "";
    return;
  }

  const truncation = result.truncated ? " (truncated for safety)" : "";
  if (dom.gddFlowTemplateMeta) {
    dom.gddFlowTemplateMeta.textContent = `${result.source_label} • ${result.original_chars} chars${truncation}`;
  }
  if (dom.gddFlowTemplatePreview) {
    dom.gddFlowTemplatePreview.value = result.text;
  }
}

function updateTemplateModeVisibility(): void {
  const sourceMode = dom.gddFlowTemplateSource?.value ?? "none";
  if (dom.gddFlowTemplateConfluenceGroup) {
    dom.gddFlowTemplateConfluenceGroup.hidden = sourceMode !== "confluence";
  }
  if (dom.gddFlowTemplateFileGroup) {
    dom.gddFlowTemplateFileGroup.hidden = sourceMode !== "file";
  }
  if (sourceMode === "none") {
    setTemplateResult(null);
  }
}

async function refreshPresetOptions(): Promise<void> {
  const presets = await invoke<GddPreset[]>("list_gdd_presets");
  if (!dom.gddFlowPreset) return;
  const previous = dom.gddFlowPreset.value;
  dom.gddFlowPreset.innerHTML = presets
    .map((preset) => {
      const label = preset.is_clone ? `${preset.name} (Clone)` : preset.name;
      return `<option value="${preset.id}">${label}</option>`;
    })
    .join("");
  if (previous && presets.some((preset) => preset.id === previous)) {
    dom.gddFlowPreset.value = previous;
  }
}

async function detectPreset(): Promise<void> {
  const transcript = currentTranscriptText();
  const templateText = loadedTemplate?.text?.trim() || "";
  const combined = [transcript, templateText].filter(Boolean).join("\n");
  if (!combined) {
    showToast({
      type: "warning",
      title: "No input",
      message: "Need transcript and/or template text to detect a preset.",
      duration: 3200,
    });
    return;
  }

  setStatus("Detecting best matching preset...");
  try {
    const result = await invoke<GddRecognitionResult>("detect_gdd_preset", {
      request: { transcript: combined },
    });

    if (dom.gddFlowPreset && result.suggested_preset_id) {
      dom.gddFlowPreset.value = result.suggested_preset_id;
    }

    const confidencePct = Math.round(result.confidence * 100);
    const top = result.candidates.slice(0, 3).map((candidate) => candidate.label).join(", ");
    setStatus(`Preset detected: ${result.suggested_preset_id} (${confidencePct}%). Top: ${top}`);
    showToast({
      type: "info",
      title: "Preset detected",
      message: `${result.suggested_preset_id} (${confidencePct}%)`,
      duration: 3200,
    });
  } catch (error) {
    setStatus("Preset detection failed.");
    showToast({
      type: "error",
      title: "Preset detection failed",
      message: String(error),
      duration: 5000,
    });
  }
}

function currentTranscriptText(): string {
  const entries = [...buildConversationHistory()].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
  return buildConversationText(entries).trim();
}

function readGenerateRequest(): GenerateGddDraftRequest {
  const transcript = currentTranscriptText();
  const presetId = dom.gddFlowPreset?.value || null;
  const title = dom.gddFlowTitle?.value?.trim() || null;
  const maxChunkChars = Number(dom.gddFlowMaxChunk?.value ?? "3500");

  return {
    transcript,
    preset_id: presetId,
    title,
    max_chunk_chars: Number.isFinite(maxChunkChars) ? maxChunkChars : 3500,
    template_hint: loadedTemplate?.text ?? null,
    template_label: loadedTemplate?.source_label ?? null,
  };
}

async function generateDraft(): Promise<void> {
  const request = readGenerateRequest();
  if (!request.transcript) {
    showToast({
      type: "warning",
      title: "No transcript",
      message: "No conversation entries available for GDD generation.",
      duration: 3800,
    });
    return;
  }

  if (dom.gddFlowGenerate) dom.gddFlowGenerate.disabled = true;
  resetPublishLink();
  setStatus("Generating draft...");
  try {
    const draft = await invoke<GddDraft>("generate_gdd_draft", { request });
    lastDraft = draft;
    if (dom.gddFlowOutput) {
      dom.gddFlowOutput.value = toPreviewText(draft);
    }
    setStatus(`Draft generated with preset '${draft.preset_id}'.`);
    showToast({
      type: "success",
      title: "GDD draft ready",
      message: `${draft.sections.length} sections generated.`,
      duration: 3500,
    });
  } catch (error) {
    setStatus("Draft generation failed.");
    showToast({
      type: "error",
      title: "GDD generation failed",
      message: String(error),
      duration: 5200,
    });
  } finally {
    if (dom.gddFlowGenerate) dom.gddFlowGenerate.disabled = false;
  }
}

async function suggestConfluenceTarget(): Promise<void> {
  const title = effectiveDraftTitle();
  const spaceKey = dom.gddFlowSpaceKey?.value?.trim() || "";
  const parentPageId = dom.gddFlowParentPageId?.value?.trim() || "";

  setStatus("Resolving Confluence target suggestion...");
  try {
    const suggestion = await invoke<ConfluenceTargetSuggestion>("suggest_confluence_target", {
      request: {
        title,
        preset_id: dom.gddFlowPreset?.value || null,
        space_key: spaceKey || null,
        parent_page_id: parentPageId || null,
      },
    });

    if (dom.gddFlowSpaceKey && suggestion.space_key) {
      dom.gddFlowSpaceKey.value = suggestion.space_key;
    }
    if (dom.gddFlowParentPageId) {
      dom.gddFlowParentPageId.value = suggestion.parent_page_id || "";
    }
    if (dom.gddFlowTargetPageId) {
      dom.gddFlowTargetPageId.value = suggestion.existing_page_id || "";
    }

    setStatus(`Target suggestion ready (${Math.round(suggestion.confidence * 100)}% confidence).`);
    showToast({
      type: "info",
      title: "Confluence target suggested",
      message: suggestion.reasoning,
      duration: 4800,
    });
  } catch (error) {
    setStatus("Target suggestion failed.");
    showToast({
      type: "error",
      title: "Suggest target failed",
      message: String(error),
      duration: 5200,
    });
  }
}

async function publishToConfluence(): Promise<void> {
  if (!lastDraft) {
    showToast({
      type: "warning",
      title: "No draft yet",
      message: "Generate a draft first.",
      duration: 3200,
    });
    return;
  }

  const spaceKey = dom.gddFlowSpaceKey?.value?.trim() || "";
  if (!spaceKey) {
    showToast({
      type: "warning",
      title: "Space key missing",
      message: "Set a Confluence space key before publishing.",
      duration: 3800,
    });
    return;
  }

  const parentPageId = dom.gddFlowParentPageId?.value?.trim() || null;
  const targetPageId = dom.gddFlowTargetPageId?.value?.trim() || null;
  const title = effectiveDraftTitle();

  if (dom.gddFlowPublish) dom.gddFlowPublish.disabled = true;
  setStatus("Rendering Confluence payload...");
  resetPublishLink();
  try {
    const storageBody = await invoke<string>("render_gdd_for_confluence", {
      draft: {
        ...lastDraft,
        title,
      },
    });

    setStatus("Publishing to Confluence...");
    const result = await invoke<GddPublishResult>("publish_gdd_to_confluence", {
      request: {
        title,
        storage_body: storageBody,
        space_key: spaceKey,
        parent_page_id: parentPageId,
        target_page_id: targetPageId,
      },
    });

    if (dom.gddFlowPublishLink) {
      dom.gddFlowPublishLink.href = result.page_url;
      dom.gddFlowPublishLink.hidden = false;
      dom.gddFlowPublishLink.textContent = `Open page (${result.page_id})`;
    }

    setStatus(result.created ? "Confluence page created." : "Confluence page updated.");
    showToast({
      type: "success",
      title: result.created ? "Confluence page created" : "Confluence page updated",
      message: result.page_url,
      duration: 5200,
    });
  } catch (error) {
    setStatus("Publish failed.");
    showToast({
      type: "error",
      title: "Publish failed",
      message: String(error),
      duration: 6000,
    });
  } finally {
    if (dom.gddFlowPublish) dom.gddFlowPublish.disabled = false;
  }
}

async function validateDraft(): Promise<void> {
  if (!lastDraft) {
    showToast({
      type: "info",
      title: "No draft yet",
      message: "Generate a draft before validation.",
      duration: 3200,
    });
    return;
  }

  try {
    const result = await invoke<{ valid: boolean; errors: string[] }>("validate_gdd_draft", {
      draft: lastDraft,
    });
    if (result.valid) {
      setStatus("Draft validation passed.");
      showToast({
        type: "success",
        title: "Draft valid",
        message: "The generated draft passed schema checks.",
        duration: 3200,
      });
      return;
    }
    setStatus("Draft validation reported issues.");
    showToast({
      type: "warning",
      title: "Validation warnings",
      message: result.errors.join(" | "),
      duration: 5200,
    });
  } catch (error) {
    setStatus("Draft validation failed.");
    showToast({
      type: "error",
      title: "Validation failed",
      message: String(error),
      duration: 5200,
    });
  }
}

async function pickTemplateFile(): Promise<void> {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [
      { name: "Template files", extensions: ["pdf", "docx", "txt", "md"] },
    ],
  });

  if (!selected || Array.isArray(selected)) return;
  if (dom.gddFlowTemplateFilePath) {
    dom.gddFlowTemplateFilePath.value = selected;
  }
}

async function loadTemplateFromFile(): Promise<void> {
  const filePath = dom.gddFlowTemplateFilePath?.value?.trim() || "";
  if (!filePath) {
    showToast({
      type: "warning",
      title: "No file selected",
      message: "Choose a file first.",
      duration: 3200,
    });
    return;
  }

  setStatus("Loading template file...");
  try {
    const result = await invoke<GddTemplateSourceResult>("load_gdd_template_from_file", {
      filePath,
    });
    setTemplateResult(result);
    setStatus(`Template loaded from file: ${result.source_label}`);
    showToast({
      type: "success",
      title: "Template loaded",
      message: `${result.source_label} imported.`,
      duration: 3200,
    });
  } catch (error) {
    setStatus("Template file loading failed.");
    showToast({
      type: "error",
      title: "Template load failed",
      message: String(error),
      duration: 5000,
    });
  }
}

async function loadTemplateFromConfluence(): Promise<void> {
  const sourceUrl = dom.gddFlowTemplateConfluenceUrl?.value?.trim() || "";
  if (!sourceUrl) {
    showToast({
      type: "warning",
      title: "Missing URL",
      message: "Enter a Confluence page URL first.",
      duration: 3200,
    });
    return;
  }

  setStatus("Loading template from Confluence...");
  try {
    const result = await invoke<GddTemplateSourceResult>("load_gdd_template_from_confluence", {
      sourceUrl,
    });
    setTemplateResult(result);
    setStatus(`Template loaded from Confluence: ${result.source_label}`);
    showToast({
      type: "success",
      title: "Confluence template loaded",
      message: result.source_label,
      duration: 3200,
    });
  } catch (error) {
    setStatus("Confluence template loading failed.");
    showToast({
      type: "error",
      title: "Confluence load failed",
      message: String(error),
      duration: 5200,
    });
  }
}

export function closeGddFlow(): void {
  if (!dom.gddFlowModal) return;
  dom.gddFlowModal.hidden = true;
  const restoreTarget = lastFocusedBeforeOpen ?? dom.modulesList ?? null;
  lastFocusedBeforeOpen = null;
  restoreTarget?.focus();
}

export async function openGddFlow(): Promise<void> {
  initGddFlow();
  if (!dom.gddFlowModal) return;

  lastFocusedBeforeOpen = document.activeElement as HTMLElement | null;
  dom.gddFlowModal.hidden = false;
  updateTemplateModeVisibility();
  await refreshPresetOptions();

  const transcript = currentTranscriptText();
  setStatus(
    transcript
      ? "Uses the current conversation transcript as input."
      : "No transcript loaded yet. Generate is disabled until transcript exists."
  );
  if (dom.gddFlowGenerate) {
    dom.gddFlowGenerate.disabled = !transcript;
  }
  if (dom.gddFlowOutput) {
    dom.gddFlowOutput.value = lastDraft ? toPreviewText(lastDraft) : "";
  }
  if (dom.gddFlowSpaceKey) {
    dom.gddFlowSpaceKey.value = settings?.confluence_settings?.default_space_key || "";
  }
  if (dom.gddFlowParentPageId) {
    dom.gddFlowParentPageId.value = settings?.confluence_settings?.default_parent_page_id || "";
  }
  if (dom.gddFlowTargetPageId) {
    dom.gddFlowTargetPageId.value = "";
  }
  resetPublishLink();

  const modalCard = dom.gddFlowModal.querySelector<HTMLElement>(".gdd-flow-modal-card");
  focusFirstElement(modalCard ?? dom.gddFlowModal, dom.gddFlowClose ?? dom.gddFlowGenerate);
}

export function initGddFlow(): void {
  if (initialized) return;
  initialized = true;

  if (!dom.gddFlowModal) return;

  dom.gddFlowTemplateSource?.addEventListener("change", () => {
    updateTemplateModeVisibility();
  });
  dom.gddFlowTemplateFilePick?.addEventListener("click", () => {
    void pickTemplateFile();
  });
  dom.gddFlowTemplateFileLoad?.addEventListener("click", () => {
    void loadTemplateFromFile();
  });
  dom.gddFlowTemplateConfluenceLoad?.addEventListener("click", () => {
    void loadTemplateFromConfluence();
  });
  dom.gddFlowGenerate?.addEventListener("click", () => {
    void generateDraft();
  });
  dom.gddFlowDetectPreset?.addEventListener("click", () => {
    void detectPreset();
  });
  dom.gddFlowValidate?.addEventListener("click", () => {
    void validateDraft();
  });
  dom.gddFlowSuggestTarget?.addEventListener("click", () => {
    void suggestConfluenceTarget();
  });
  dom.gddFlowPublish?.addEventListener("click", () => {
    void publishToConfluence();
  });

  dom.gddFlowClose?.addEventListener("click", closeGddFlow);
  dom.gddFlowBackdrop?.addEventListener("click", closeGddFlow);

  dom.gddFlowModal.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeGddFlow();
      return;
    }
    const modalCard = dom.gddFlowModal?.querySelector<HTMLElement>(".gdd-flow-modal-card");
    if (modalCard) {
      trapFocusInModal(event, modalCard);
    }
  });
}
