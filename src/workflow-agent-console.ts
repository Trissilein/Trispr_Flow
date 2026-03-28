import { invoke } from "@tauri-apps/api/core";
import * as dom from "./dom-refs";
import { settings } from "./state";
import { showToast } from "./toast";
import { isAmbiguousSelection, isValidTargetLanguage } from "./workflow-agent-policy";
import type {
  AgentBuildExecutionPlanRequest,
  AgentCommandParseResult,
  AgentExecutionPlan,
  AgentExecutionResult,
  AssistantStateChangedEvent,
  TranscriptionRawResultEvent,
  TranscriptSessionCandidate,
} from "./types";

let initialized = false;
let lastParse: AgentCommandParseResult | null = null;
let lastCandidates: TranscriptSessionCandidate[] = [];
let selectedSessionId = "";
let currentPlan: AgentExecutionPlan | null = null;
let languageExplicitlySet = false;
let latestAssistantState: AssistantStateChangedEvent | null = null;

function ensureWorkflowAgentDefaults(): void {
  if (!settings) return;
  settings.workflow_agent ??= {
    enabled: false,
    wakewords: ["trispr", "hey trispr", "trispr agent"],
    intent_keywords: {
      gdd_generate_publish: [
        "gdd",
        "game design document",
        "designdokument",
        "publish",
        "confluence",
        "generate",
        "draft",
      ],
    },
    model: "qwen3:4b",
    temperature: 0.2,
    max_tokens: 512,
    session_gap_minutes: 20,
    max_candidates: 3,
  };
}

function isModuleEnabled(moduleId: string): boolean {
  return settings?.module_settings?.enabled_modules?.includes(moduleId) ?? false;
}

function isWorkflowAgentEnabled(): boolean {
  ensureWorkflowAgentDefaults();
  return isModuleEnabled("workflow_agent") && Boolean(settings?.workflow_agent?.enabled);
}

function isAssistantModeEnabled(): boolean {
  return settings?.product_mode === "assistant";
}

function appendLog(line: string): void {
  if (!dom.workflowAgentExecutionLog) return;
  const now = new Date().toLocaleTimeString();
  const next = `[${now}] ${line}`;
  const current = dom.workflowAgentExecutionLog.value.trim();
  dom.workflowAgentExecutionLog.value = current ? `${current}\n${next}` : next;
  dom.workflowAgentExecutionLog.scrollTop = dom.workflowAgentExecutionLog.scrollHeight;
}

function isReviewConfirmed(): boolean {
  return Boolean(dom.workflowAgentReviewConfirm?.checked);
}

function setReviewConfirmed(value: boolean): void {
  if (dom.workflowAgentReviewConfirm) {
    dom.workflowAgentReviewConfirm.checked = value;
  }
}

function renderReviewGate(): void {
  const interactionEnabled = isWorkflowAgentEnabled() && isAssistantModeEnabled();
  const hasPlan = Boolean(currentPlan);
  const publishLaneActive = (currentPlan?.execution_steps?.length ?? 0) > 0;
  const reviewConfirmed = isReviewConfirmed();
  if (dom.workflowAgentReviewSummary) {
    if (!hasPlan) {
      dom.workflowAgentReviewSummary.textContent =
        "Build a plan first, then review signals, assumptions, and action lanes.";
    } else if (publishLaneActive) {
      dom.workflowAgentReviewSummary.textContent =
        "Side-effect lane detected (publish). Review and confirm is required before execute.";
    } else {
      dom.workflowAgentReviewSummary.textContent =
        "Draft-only lane detected (no publish step). Confirm review to execute.";
    }
  }
  const canExecute = interactionEnabled && hasPlan && reviewConfirmed;
  dom.workflowAgentExecuteBtn?.toggleAttribute("disabled", !canExecute);
}

function renderStatus(): void {
  if (!dom.workflowAgentConsole) return;
  const enabled = isWorkflowAgentEnabled();
  dom.workflowAgentConsole.hidden = !enabled;
  const interactionEnabled = enabled && isAssistantModeEnabled();
  const controls: Array<HTMLElement | null> = [
    dom.workflowAgentCommandInput,
    dom.workflowAgentParseBtn,
    dom.workflowAgentRefreshCandidatesBtn,
    dom.workflowAgentTargetLanguage,
    dom.workflowAgentBuildPlanBtn,
    dom.workflowAgentReviewConfirm,
  ];
  controls.forEach((control) => control?.toggleAttribute("disabled", !interactionEnabled));

  if (!dom.workflowAgentStatus) return;
  if (!enabled) {
    dom.workflowAgentStatus.textContent = "Agent disabled.";
    renderReviewGate();
    return;
  }
  if (!isAssistantModeEnabled()) {
    dom.workflowAgentStatus.textContent = "Assistant mode is off. Switch Product mode to Assistant.";
    renderReviewGate();
    return;
  }

  const stateLabel = latestAssistantState?.state ?? "listening";
  const base = `Assistant state: ${stateLabel.replace(/_/g, " ")}.`;
  if (!latestAssistantState?.capability?.degraded) {
    dom.workflowAgentStatus.textContent = base;
    renderReviewGate();
    return;
  }
  const softMissing = latestAssistantState.capability.missing_capabilities
    .filter((id) => id === "output_voice_tts" || id === "input_vision")
    .join(", ");
  dom.workflowAgentStatus.textContent = softMissing
    ? `${base} Degraded capability: ${softMissing}.`
    : `${base} Degraded capability mode active.`;
  renderReviewGate();
}

function renderCandidates(): void {
  if (!dom.workflowAgentCandidates) return;
  if (!lastCandidates.length) {
    dom.workflowAgentCandidates.innerHTML = `<div class="field-hint">No session candidates yet.</div>`;
    return;
  }
  dom.workflowAgentCandidates.innerHTML = lastCandidates
    .map((candidate) => {
      const selected = candidate.session_id === selectedSessionId;
      return `<button type="button" class="ghost-btn workflow-agent-candidate${
        selected ? " is-active" : ""
      }" data-session-id="${candidate.session_id}">
        <strong>${new Date(candidate.start_ms).toLocaleString()}</strong>
        <span>${candidate.entry_count} entries · score ${(candidate.score * 100).toFixed(0)}%</span>
        <span>${candidate.preview || "No preview"}</span>
      </button>`;
    })
    .join("");
}

function renderPlanPreview(): void {
  if (!dom.workflowAgentPlanPreview) return;
  if (!currentPlan) {
    dom.workflowAgentPlanPreview.value = "";
    renderReviewGate();
    return;
  }
  const steps = currentPlan.steps.map((step) => `- ${step.title}`).join("\n");
  const recognizedSignals = (currentPlan.recognized_signals ?? [])
    .map((item) => `- ${item}`)
    .join("\n");
  const assumptions = (currentPlan.assumptions ?? []).map((item) => `- ${item}`).join("\n");
  const proposedActions = (currentPlan.proposed_actions ?? [])
    .map((item) => `- ${item}`)
    .join("\n");
  const analysisSteps = (currentPlan.analysis_steps ?? []).map((step) => `- ${step.title}`).join("\n");
  const executionSteps = (currentPlan.execution_steps ?? [])
    .map((step) => `- ${step.title}`)
    .join("\n");
  dom.workflowAgentPlanPreview.value = [
    `Intent: ${currentPlan.intent}`,
    `Session: ${currentPlan.session_id}`,
    `Target language: ${currentPlan.target_language}`,
    `Publish: ${currentPlan.publish ? "yes" : "no"}`,
    "",
    "Recognized signals:",
    recognizedSignals || "- none",
    "",
    "Assumptions:",
    assumptions || "- none",
    "",
    "Proposed actions:",
    proposedActions || "- none",
    "",
    "Analysis lane (no side effects):",
    analysisSteps || "- none",
    "",
    "Execution lane (side effects):",
    executionSteps || "- none",
    "",
    "Combined steps:",
    steps || "- none",
  ].join("\n");
  renderReviewGate();
}

function matchesWakeword(text: string): boolean {
  const normalized = text.trim().toLowerCase();
  if (!normalized) return false;
  const wakewords = settings?.workflow_agent?.wakewords ?? [];
  return wakewords.some((wakeword) => {
    const needle = wakeword.trim().toLowerCase();
    return Boolean(needle) && normalized.includes(needle);
  });
}

async function parseCommand(commandText: string): Promise<void> {
  if (!isAssistantModeEnabled()) {
    showToast({
      type: "info",
      title: "Assistant mode required",
      message: "Switch Product mode to Assistant before running workflow-agent commands.",
      duration: 3200,
    });
    return;
  }
  if (!commandText.trim()) {
    showToast({
      type: "warning",
      title: "No command",
      message: "Enter or speak a command first.",
      duration: 3000,
    });
    return;
  }
  const parsed = await invoke<AgentCommandParseResult>("agent_parse_command", {
    request: {
      command_text: commandText,
      source: "ui_console",
    },
  });
  lastParse = parsed;
  languageExplicitlySet = false; // M10: reset on each new command
  appendLog(
    `Parsed command -> intent=${parsed.intent}, confidence=${(parsed.confidence * 100).toFixed(0)}%, publish=${parsed.publish_requested}`
  );
  if (!parsed.detected) {
    showToast({
      type: "info",
      title: "No actionable intent",
      message: "Wakeword or intent keywords were missing.",
      duration: 3200,
    });
    return;
  }
  await refreshCandidates();
}

async function refreshCandidates(): Promise<void> {
  if (!isAssistantModeEnabled()) return;
  if (!lastParse) {
    showToast({
      type: "info",
      title: "Parse first",
      message: "Parse a command before searching sessions.",
      duration: 3000,
    });
    return;
  }
  const candidates = await invoke<TranscriptSessionCandidate[]>("search_transcript_sessions", {
    request: {
      temporal_hint: lastParse.temporal_hint ?? null,
      topic_hint: lastParse.topic_hint ?? null,
      session_gap_minutes: settings?.workflow_agent?.session_gap_minutes ?? 20,
      max_candidates: settings?.workflow_agent?.max_candidates ?? 3,
    },
  });
  lastCandidates = candidates;
  selectedSessionId = ""; // M9: require explicit selection, no auto-select
  currentPlan = null;
  setReviewConfirmed(false);
  renderCandidates();
  renderPlanPreview();
  appendLog(`Session search -> ${candidates.length} candidate(s) found.`);
  if (candidates.length === 0) {
    appendLog("No matching sessions found. Try different keywords or check transcript history.");
    return;
  }
  if (lastParse?.topic_hint) {
    appendLog(`Detected topic: "${lastParse.topic_hint}"`);
  }
  if (lastParse?.temporal_hint) {
    appendLog(`Detected time hint: "${lastParse.temporal_hint}"`);
  }
  if (isAmbiguousSelection(candidates)) {
    appendLog(
      `⚠ Top sessions have similar scores (${(candidates[0].score * 100).toFixed(0)}% vs ${(candidates[1].score * 100).toFixed(0)}%). Please review and select manually.`
    );
  }
  appendLog("Select a session above before building the plan.");
}

async function buildPlan(): Promise<void> {
  if (!isAssistantModeEnabled()) return;
  if (!lastParse?.detected) {
    showToast({
      type: "warning",
      title: "No command parsed",
      message: "Parse a command first.",
      duration: 3200,
    });
    return;
  }
  if (!selectedSessionId) {
    showToast({
      type: "warning",
      title: "No session selected",
      message: "Click a session candidate above to select it before building the plan.",
      duration: 3500,
    });
    return;
  }
  if (!languageExplicitlySet) {
    showToast({
      type: "warning",
      title: "Language required",
      message: "Please select the target language before building the plan.",
      duration: 3500,
    });
    return;
  }
  const targetLanguage = dom.workflowAgentTargetLanguage?.value ?? "";
  if (!isValidTargetLanguage(targetLanguage)) {
    showToast({
      type: "warning",
      title: "Invalid language",
      message: `Language "${targetLanguage}" is not supported. Please select a valid option.`,
      duration: 3500,
    });
    return;
  }
  const req: AgentBuildExecutionPlanRequest = {
    intent: lastParse.intent,
    session_id: selectedSessionId,
    target_language: targetLanguage,
    publish: Boolean(lastParse.publish_requested),
    command_text: lastParse.command_text,
    temporal_hint: lastParse.temporal_hint ?? null,
    topic_hint: lastParse.topic_hint ?? null,
    parse_confidence: lastParse.confidence,
  };
  currentPlan = await invoke<AgentExecutionPlan>("agent_build_execution_plan", { request: req });
  setReviewConfirmed(false);
  renderPlanPreview();
  appendLog("Execution plan ready. Review the plan details and confirm review before execution.");
}

async function executePlan(): Promise<void> {
  if (!isAssistantModeEnabled()) return;
  if (!currentPlan) {
    showToast({
      type: "warning",
      title: "No plan",
      message: "Build a plan before execution.",
      duration: 3000,
    });
    return;
  }
  if (!isReviewConfirmed()) {
    showToast({
      type: "warning",
      title: "Review required",
      message: "Review the plan and toggle review confirmation before execution.",
      duration: 3500,
    });
    return;
  }
  appendLog("Executing plan...");
  const result = await invoke<AgentExecutionResult>("agent_execute_gdd_plan", {
    request: {
      plan: currentPlan,
      title: null,
      preset_id: null,
      max_chunk_chars: null,
      space_key: settings?.confluence_settings?.default_space_key || null,
      parent_page_id: settings?.confluence_settings?.default_parent_page_id || null,
      target_page_id: null,
    },
  });
  appendLog(`Execution result -> ${result.status}: ${result.message}`);
  showToast({
    type: result.status === "failed" ? "error" : result.status === "queued" ? "warning" : "success",
    title: "Workflow Agent",
    message: result.message,
    duration: 4200,
  });
}

function bindUi(): void {
  dom.workflowAgentParseBtn?.addEventListener("click", () => {
    void parseCommand(dom.workflowAgentCommandInput?.value || "");
  });
  dom.workflowAgentRefreshCandidatesBtn?.addEventListener("click", () => {
    void refreshCandidates();
  });
  dom.workflowAgentBuildPlanBtn?.addEventListener("click", () => {
    void buildPlan();
  });
  dom.workflowAgentExecuteBtn?.addEventListener("click", () => {
    void executePlan();
  });
  dom.workflowAgentCandidates?.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const button = target?.closest<HTMLButtonElement>("[data-session-id]");
    if (!button) return;
    selectedSessionId = button.dataset.sessionId || "";
    currentPlan = null;
    setReviewConfirmed(false);
    renderCandidates();
    renderPlanPreview();
    appendLog(`Selected session ${selectedSessionId}`);
  });
  dom.workflowAgentTargetLanguage?.addEventListener("change", () => {
    languageExplicitlySet = true; // M10: user actively chose language
    if (!currentPlan) {
      return;
    }
    currentPlan = null;
    setReviewConfirmed(false);
    renderPlanPreview();
    appendLog("Target language changed. Rebuild the execution plan.");
  });
  dom.workflowAgentReviewConfirm?.addEventListener("change", () => {
    renderReviewGate();
  });
}

export function focusWorkflowAgentConsole(): void {
  renderStatus();
  dom.workflowAgentConsole?.scrollIntoView({ behavior: "smooth", block: "start" });
  dom.workflowAgentCommandInput?.focus();
}

export function initWorkflowAgentConsole(): void {
  if (initialized) return;
  initialized = true;
  bindUi();
  renderStatus();
  renderCandidates();
  renderPlanPreview();
  renderReviewGate();
}

export function syncWorkflowAgentConsoleState(): void {
  renderStatus();
  renderReviewGate();
}

export async function handleWorkflowAgentRawResult(
  payload: TranscriptionRawResultEvent
): Promise<void> {
  if (!isWorkflowAgentEnabled()) return;
  if (!isAssistantModeEnabled()) return;
  if (!payload?.text?.trim()) return;
  if (!matchesWakeword(payload.text)) return;

  appendLog(`Wakeword detected from ${payload.source}.`);
  if (dom.workflowAgentCommandInput) {
    dom.workflowAgentCommandInput.value = payload.text;
  }
  await parseCommand(payload.text);
}

export function appendWorkflowAgentLog(line: string): void {
  appendLog(line);
}

export function handleAssistantStateChanged(payload: AssistantStateChangedEvent): void {
  latestAssistantState = payload;
  renderStatus();
}
