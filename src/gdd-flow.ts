import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import * as dom from "./dom-refs";
import {
  ONE_CLICK_ROUTE_CONFIDENCE_THRESHOLD,
  requiresOneClickPublishConfirmation,
} from "./gdd-policy";
import { buildConversationHistory, buildConversationText } from "./history";
import { focusFirstElement, trapFocusInModal } from "./modal-focus";
import { appRuntimeStartedMs, settings } from "./state";
import { persistSettings } from "./settings";
import { showToast } from "./toast";
import type {
  ConfluenceTargetSuggestion,
  GddDraft,
  GddPendingPublishJob,
  GddPreset,
  GddPublishAttemptResult,
  GddPublishResult,
  GddRecognitionResult,
  GddTemplateSourceResult,
  GenerateGddDraftRequest,
  HistoryEntry,
} from "./types";

let initialized = false;
let loadedTemplate: GddTemplateSourceResult | null = null;
let lastDraft: GddDraft | null = null;
let lastTargetSuggestion: ConfluenceTargetSuggestion | null = null;
let lastFocusedBeforeOpen: HTMLElement | null = null;

function htmlEscape(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function effectiveDraftTitle(): string {
  return dom.gddFlowTitle?.value?.trim() || lastDraft?.title?.trim() || "Game Design Document";
}

function modeFromSettings(): "standard" | "advanced" {
  return settings?.gdd_module_settings?.workflow_mode_default === "advanced"
    ? "advanced"
    : "standard";
}

function setWorkflowMode(mode: "standard" | "advanced", persist = false): void {
  const card = dom.gddFlowModal?.querySelector<HTMLElement>(".gdd-flow-modal-card");
  if (card) {
    card.classList.toggle("gdd-mode-standard", mode === "standard");
    card.classList.toggle("gdd-mode-advanced", mode === "advanced");
  }
  dom.gddFlowModeStandard?.classList.toggle("is-active", mode === "standard");
  dom.gddFlowModeAdvanced?.classList.toggle("is-active", mode === "advanced");

  if (persist && settings?.gdd_module_settings) {
    settings.gdd_module_settings.workflow_mode_default = mode;
    void persistSettings();
  }
}

function isOneClickPublishPreferred(): boolean {
  return Boolean(settings?.gdd_module_settings?.prefer_one_click_publish);
}

function oneClickConfidenceThreshold(): number {
  const configured = Number(settings?.gdd_module_settings?.one_click_confidence_threshold);
  if (!Number.isFinite(configured) || configured < 0 || configured > 1) {
    return ONE_CLICK_ROUTE_CONFIDENCE_THRESHOLD;
  }
  return configured;
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

function runtimeTranscriptEntries(): HistoryEntry[] {
  const now = Date.now();
  return buildConversationHistory()
    .filter((entry) => entry.timestamp_ms >= appRuntimeStartedMs && entry.timestamp_ms <= now)
    .sort((a, b) => a.timestamp_ms - b.timestamp_ms);
}

function formatClock(timestampMs: number): string {
  return new Date(timestampMs).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function updateRuntimeSummary(): void {
  const entries = runtimeTranscriptEntries();
  if (!dom.gddFlowRuntimeSummary) return;

  if (entries.length === 0) {
    dom.gddFlowRuntimeSummary.textContent =
      "Current runtime session: no transcript entries yet. Generate is disabled.";
    return;
  }

  const first = entries[0].timestamp_ms;
  const last = entries[entries.length - 1].timestamp_ms;
  dom.gddFlowRuntimeSummary.textContent =
    `Current runtime session: ${entries.length} entries, ${formatClock(first)} - ${formatClock(last)}.`;
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
    dom.gddFlowTemplateMeta.textContent = `${result.source_label} - ${result.original_chars} chars${truncation}`;
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

  const defaultPreset = settings?.gdd_module_settings?.default_preset_id;
  if (previous && presets.some((preset) => preset.id === previous)) {
    dom.gddFlowPreset.value = previous;
  } else if (defaultPreset && presets.some((preset) => preset.id === defaultPreset)) {
    dom.gddFlowPreset.value = defaultPreset;
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
    const top = result.candidates
      .slice(0, 3)
      .map((candidate) => candidate.label)
      .join(", ");
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
  const entries = runtimeTranscriptEntries();
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
      message: "No runtime session entries available for GDD generation.",
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
    lastTargetSuggestion = null;
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

function applyConfluenceTargetSuggestion(suggestion: ConfluenceTargetSuggestion): void {
  lastTargetSuggestion = suggestion;
  if (dom.gddFlowSpaceKey && suggestion.space_key) {
    dom.gddFlowSpaceKey.value = suggestion.space_key;
  }
  if (dom.gddFlowParentPageId) {
    dom.gddFlowParentPageId.value = suggestion.parent_page_id || "";
  }
  if (dom.gddFlowTargetPageId) {
    dom.gddFlowTargetPageId.value = suggestion.existing_page_id || "";
  }
}

async function requestConfluenceTargetSuggestion(): Promise<ConfluenceTargetSuggestion> {
  const title = effectiveDraftTitle();
  const spaceKey = dom.gddFlowSpaceKey?.value?.trim() || "";
  const parentPageId = dom.gddFlowParentPageId?.value?.trim() || "";

  const suggestion = await invoke<ConfluenceTargetSuggestion>("suggest_confluence_target", {
    request: {
      title,
      preset_id: dom.gddFlowPreset?.value || null,
      space_key: spaceKey || null,
      parent_page_id: parentPageId || null,
    },
  });
  applyConfluenceTargetSuggestion(suggestion);
  return suggestion;
}

function setPublishBusy(busy: boolean): void {
  if (dom.gddFlowPublish) dom.gddFlowPublish.disabled = busy;
  if (dom.gddFlowOneClickPublish) dom.gddFlowOneClickPublish.disabled = busy;
  if (dom.gddFlowSuggestTarget) dom.gddFlowSuggestTarget.disabled = busy;
  if (dom.gddFlowQueueRefresh) dom.gddFlowQueueRefresh.disabled = busy;
}

async function suggestConfluenceTarget(): Promise<void> {
  setStatus("Resolving Confluence target suggestion...");
  try {
    const suggestion = await requestConfluenceTargetSuggestion();
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

async function publishViaAttempt(
  suggestion: ConfluenceTargetSuggestion | null
): Promise<GddPublishAttemptResult> {
  if (!lastDraft) {
    return {
      status: "failed",
      error: "No draft available.",
    };
  }

  const spaceKey = dom.gddFlowSpaceKey?.value?.trim() || "";
  if (!spaceKey) {
    return {
      status: "failed",
      error: "Confluence space key is required.",
    };
  }

  const parentPageId = dom.gddFlowParentPageId?.value?.trim() || null;
  const targetPageId = dom.gddFlowTargetPageId?.value?.trim() || null;
  const title = effectiveDraftTitle();

  const draft = { ...lastDraft, title };
  const storageBody = await invoke<string>("render_gdd_for_confluence", { draft });

  return invoke<GddPublishAttemptResult>("publish_or_queue_gdd_to_confluence", {
    request: {
      draft,
      publish_request: {
        title,
        storage_body: storageBody,
        space_key: spaceKey,
        parent_page_id: parentPageId,
        target_page_id: targetPageId,
      },
      routing_confidence: suggestion?.confidence ?? null,
      routing_reasoning: suggestion?.reasoning ?? null,
    },
  });
}

function applyPublishSuccessUi(result: GddPublishResult): void {
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
}

function applyPublishQueuedUi(job: GddPendingPublishJob, errorMessage: string | undefined): void {
  setStatus("Confluence not reachable. Publish request queued locally.");
  showToast({
    type: "warning",
    title: "Saved for later publish",
    message: errorMessage
      ? `Queued (${job.job_id}). ${errorMessage}`
      : `Queued (${job.job_id}) at ${job.bundle_dir}`,
    duration: 5600,
  });
}

async function publishToConfluenceInternal(
  suggestion: ConfluenceTargetSuggestion | null,
  oneClick: boolean
): Promise<void> {
  if (!lastDraft) {
    showToast({
      type: "warning",
      title: "No draft yet",
      message: "Generate a draft first.",
      duration: 3200,
    });
    return;
  }

  if (oneClick && suggestion) {
    const threshold = oneClickConfidenceThreshold();
    const confidencePct = Math.round(suggestion.confidence * 100);
    if (requiresOneClickPublishConfirmation(suggestion.confidence, threshold)) {
      const confirmed = window.confirm(
        `Routing confidence is ${confidencePct}% (threshold ${Math.round(
          threshold * 100
        )}%). ${suggestion.reasoning}\n\nContinue and publish anyway?`
      );
      if (!confirmed) {
        setStatus("One-click publish cancelled (low-confidence route).");
        showToast({
          type: "info",
          title: "Publish cancelled",
          message: "Review or adjust target fields before publishing.",
          duration: 3800,
        });
        return;
      }
    }
  }

  setStatus("Publishing to Confluence...");
  resetPublishLink();

  try {
    const result = await publishViaAttempt(suggestion);
    if (result.status === "published" && result.publish_result) {
      applyPublishSuccessUi(result.publish_result);
    } else if (result.status === "queued" && result.queued_job) {
      applyPublishQueuedUi(result.queued_job, result.error);
    } else {
      setStatus("Publish failed.");
      showToast({
        type: "error",
        title: "Publish failed",
        message: result.error || "Unknown error",
        duration: 6000,
      });
    }
  } catch (error) {
    setStatus("Publish failed.");
    showToast({
      type: "error",
      title: "Publish failed",
      message: String(error),
      duration: 6000,
    });
  } finally {
    await refreshPendingQueue();
  }
}

async function publishToConfluence(): Promise<void> {
  setPublishBusy(true);
  try {
    await publishToConfluenceInternal(lastTargetSuggestion, false);
  } finally {
    setPublishBusy(false);
  }
}

async function oneClickPublishToConfluence(): Promise<void> {
  if (!lastDraft) {
    showToast({
      type: "warning",
      title: "No draft yet",
      message: "Generate a draft first.",
      duration: 3200,
    });
    return;
  }

  setPublishBusy(true);
  try {
    let suggestion = lastTargetSuggestion;
    if (!suggestion) {
      setStatus("Resolving target for one-click publish...");
      suggestion = await requestConfluenceTargetSuggestion();
    } else {
      setStatus("Using cached target suggestion for one-click publish...");
    }
    await publishToConfluenceInternal(suggestion, true);
  } catch (error) {
    setStatus("One-click target suggestion failed.");
    showToast({
      type: "error",
      title: "One-click publish failed",
      message: String(error),
      duration: 5200,
    });
  } finally {
    setPublishBusy(false);
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
    filters: [{ name: "Template files", extensions: ["pdf", "docx", "txt", "md"] }],
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

function renderPendingQueue(jobs: GddPendingPublishJob[]): void {
  if (!dom.gddFlowQueueList) return;
  if (jobs.length === 0) {
    dom.gddFlowQueueList.innerHTML = '<div class="archive-empty">No pending publishes.</div>';
    return;
  }

  dom.gddFlowQueueList.innerHTML = jobs
    .map((job) => {
      const confidence =
        Number.isFinite(job.routing_confidence) && job.routing_confidence !== null
          ? `${Math.round(Number(job.routing_confidence) * 100)}%`
          : "n/a";
      return `
        <div class="gdd-queue-item" data-job-id="${htmlEscape(job.job_id)}">
          <div class="gdd-queue-item-title">${htmlEscape(job.title)}</div>
          <div class="gdd-queue-item-meta">Space: ${htmlEscape(job.space_key)} | Retries: ${job.retry_count} | Confidence: ${confidence}</div>
          <div class="gdd-queue-item-meta">Updated: ${htmlEscape(job.updated_at_iso)}</div>
          <div class="gdd-queue-item-meta">Last error: ${htmlEscape(job.last_error || "-")}</div>
          <div class="gdd-queue-item-actions">
            <button type="button" class="hotkey-record-btn" data-action="retry" data-job-id="${htmlEscape(job.job_id)}">Retry</button>
            <button type="button" class="hotkey-record-btn" data-action="delete" data-job-id="${htmlEscape(job.job_id)}">Delete</button>
          </div>
        </div>
      `;
    })
    .join("");
}

async function refreshPendingQueue(): Promise<void> {
  try {
    const jobs = await invoke<GddPendingPublishJob[]>("list_pending_gdd_publishes");
    renderPendingQueue(jobs);
  } catch (error) {
    if (dom.gddFlowQueueList) {
      dom.gddFlowQueueList.innerHTML = `<div class="archive-empty">Failed to load queue: ${htmlEscape(
        String(error)
      )}</div>`;
    }
  }
}

async function retryPendingJob(jobId: string): Promise<void> {
  setPublishBusy(true);
  try {
    const result = await invoke<GddPublishAttemptResult>("retry_pending_gdd_publish", { jobId });
    if (result.status === "published" && result.publish_result) {
      applyPublishSuccessUi(result.publish_result);
    } else if (result.status === "queued") {
      setStatus("Retry failed transiently. Job remains queued.");
      showToast({
        type: "warning",
        title: "Retry still queued",
        message: result.error || "Confluence still unreachable.",
        duration: 4200,
      });
    } else {
      setStatus("Retry failed (non-queueable). Job kept for manual cleanup.");
      showToast({
        type: "error",
        title: "Retry failed",
        message: result.error || "Retry failed",
        duration: 5200,
      });
    }
  } catch (error) {
    showToast({
      type: "error",
      title: "Retry failed",
      message: String(error),
      duration: 5200,
    });
  } finally {
    await refreshPendingQueue();
    setPublishBusy(false);
  }
}

async function deletePendingJob(jobId: string): Promise<void> {
  try {
    await invoke<boolean>("delete_pending_gdd_publish", { jobId });
    showToast({
      type: "info",
      title: "Queue item removed",
      message: `Removed ${jobId}`,
      duration: 3000,
    });
  } catch (error) {
    showToast({
      type: "error",
      title: "Delete failed",
      message: String(error),
      duration: 4200,
    });
  } finally {
    await refreshPendingQueue();
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
  lastTargetSuggestion = null;
  dom.gddFlowModal.hidden = false;
  setWorkflowMode(modeFromSettings(), false);
  updateTemplateModeVisibility();
  await refreshPresetOptions();
  await refreshPendingQueue();

  updateRuntimeSummary();
  const transcript = currentTranscriptText();
  setStatus(
    transcript
      ? "Uses the current runtime session transcript as input."
      : "No runtime session transcript available yet."
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
  setPublishBusy(false);
  resetPublishLink();

  const modalCard = dom.gddFlowModal.querySelector<HTMLElement>(".gdd-flow-modal-card");
  focusFirstElement(modalCard ?? dom.gddFlowModal, dom.gddFlowClose ?? dom.gddFlowGenerate);
}

export function initGddFlow(): void {
  if (initialized) return;
  initialized = true;

  if (!dom.gddFlowModal) return;

  dom.gddFlowModeStandard?.addEventListener("click", () => {
    setWorkflowMode("standard", true);
  });
  dom.gddFlowModeAdvanced?.addEventListener("click", () => {
    setWorkflowMode("advanced", true);
  });

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
  dom.gddFlowQueueRefresh?.addEventListener("click", () => {
    void refreshPendingQueue();
  });
  dom.gddFlowOneClickPublish?.addEventListener("click", () => {
    void oneClickPublishToConfluence();
  });
  dom.gddFlowPublish?.addEventListener("click", () => {
    if (isOneClickPublishPreferred()) {
      void oneClickPublishToConfluence();
      return;
    }
    void publishToConfluence();
  });

  dom.gddFlowQueueList?.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const button = target?.closest<HTMLButtonElement>("button[data-action][data-job-id]");
    if (!button) return;
    const action = button.dataset.action;
    const jobId = button.dataset.jobId;
    if (!jobId) return;
    if (action === "retry") {
      void retryPendingJob(jobId);
      return;
    }
    if (action === "delete") {
      void deletePendingJob(jobId);
    }
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
