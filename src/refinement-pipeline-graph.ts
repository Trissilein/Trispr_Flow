import type {
  TranscriptionRefinedEvent,
  TranscriptionRefinementFailedEvent,
  TranscriptionRefinementStartedEvent,
  TranscriptionResultEvent,
} from "./types";
import { settings, startupStatus } from "./state";
import { getOllamaRuntimeCardState } from "./ollama-models";
import * as dom from "./dom-refs";

type NodeState = "idle" | "active" | "success" | "bypassed" | "blocked" | "error" | "timeout" | "warming";
type EdgeState = "idle" | "active" | "muted";
type PipelinePhase = "idle" | "raw_emitted" | "refining" | "refined" | "failed" | "timed_out";
type ToggleVisualState = "off" | "pending" | "on";

type PipelineJobState = {
  jobId: string;
  source: string;
  phase: PipelinePhase;
  deferred: boolean;
  model: string;
  error: string;
};

const PIPELINE_TERMINAL_RESET_MS = 2200;

const pipelineJobState: PipelineJobState = {
  jobId: "",
  source: "",
  phase: "idle",
  deferred: false,
  model: "",
  error: "",
};

let pipelineTerminalResetTimer: number | null = null;

function clearPipelineTerminalResetTimer(): void {
  if (pipelineTerminalResetTimer !== null) {
    window.clearTimeout(pipelineTerminalResetTimer);
    pipelineTerminalResetTimer = null;
  }
}

function resetPipelineJobState(): void {
  pipelineJobState.jobId = "";
  pipelineJobState.source = "";
  pipelineJobState.phase = "idle";
  pipelineJobState.deferred = false;
  pipelineJobState.model = "";
  pipelineJobState.error = "";
}

function schedulePipelineTerminalReset(jobId: string): void {
  const normalized = jobId.trim();
  if (!normalized) return;
  clearPipelineTerminalResetTimer();
  pipelineTerminalResetTimer = window.setTimeout(() => {
    pipelineTerminalResetTimer = null;
    if (pipelineJobState.jobId.trim() !== normalized) {
      return;
    }
    if (pipelineJobState.phase === "idle" || pipelineJobState.phase === "refining") {
      return;
    }
    resetPipelineJobState();
    renderRefinementPipelineGraph();
  }, PIPELINE_TERMINAL_RESET_MS);
}

function setNodeState(nodeId: string, state: NodeState): void {
  const element = document.getElementById(nodeId);
  if (!element) return;
  element.dataset.state = state;
}

function setEdgeState(edgeId: string, state: EdgeState): void {
  const element = document.getElementById(edgeId);
  if (!element) return;
  element.dataset.state = state;
}

function setPipelineToggleVisualState(
  nodeId: string,
  visualState: ToggleVisualState,
  labelText: string,
): void {
  const node = document.getElementById(nodeId);
  if (!node) return;
  const toggle = node.querySelector<HTMLLabelElement>(".pipeline-node-toggle");
  const label = node.querySelector<HTMLElement>(".pipeline-node-toggle-label");
  if (toggle) {
    toggle.dataset.visualState = visualState;
  }
  if (label) {
    label.textContent = labelText;
  }
}

function resolveConfiguredAiModel(): string {
  const configured = settings?.ai_fallback?.model?.trim();
  if (configured) return configured;
  const provider = settings?.ai_fallback?.provider;
  if (provider === "lm_studio") {
    const lmModel = settings?.providers?.lm_studio?.preferred_model?.trim();
    if (lmModel) return lmModel;
  } else if (provider === "oobabooga") {
    const oobModel = settings?.providers?.oobabooga?.preferred_model?.trim();
    if (oobModel) return oobModel;
  } else {
    const ollamaModel = settings?.providers?.ollama?.preferred_model?.trim();
    if (ollamaModel) return ollamaModel;
  }
  const postproc = settings?.postproc_llm_model?.trim();
  if (postproc) return postproc;
  return "";
}

function setAiNodeCopy(summary: string): void {
  const node = document.getElementById("pipeline-node-ai");
  const body = node?.querySelector("p");
  if (body) {
    body.textContent = summary;
  }
}

function isAiRefinementModuleEnabled(): boolean {
  return settings?.module_settings?.enabled_modules?.includes("ai_refinement") ?? false;
}

function isLocalAiPathEnabled(): boolean {
  if (!isAiRefinementModuleEnabled() || !settings?.ai_fallback?.enabled) return false;
  return (
    settings.ai_fallback.provider === "ollama"
    && settings.ai_fallback.execution_mode === "local_primary"
  );
}

function isCompatLocalPathEnabled(): boolean {
  if (!isAiRefinementModuleEnabled() || !settings?.ai_fallback?.enabled) return false;
  const p = settings.ai_fallback.provider;
  return p === "lm_studio" || p === "oobabooga";
}

function describeIdleState(aiEnabled: boolean, rulesEnabled: boolean): string {
  if (aiEnabled && rulesEnabled) {
    return "Idle: AI refinement is primary, rule-based refinement remains available (non-AI, no token/API cost).";
  }
  if (aiEnabled) {
    return "Idle: AI-only path is active (rule-based non-AI fallback disabled).";
  }
  if (rulesEnabled) {
    return "Idle: rule-based path is active (non-AI, no token/API cost).";
  }
  return "Idle: raw transcription output (no refinement active).";
}

function formatJobToken(jobId: string): string {
  if (!jobId) return "—";
  if (jobId.length <= 28) return jobId;
  return `${jobId.slice(0, 16)}…${jobId.slice(-8)}`;
}

function updateLiveSummary(
  hasJob: boolean,
  aiEnabled: boolean,
  rulesEnabled: boolean,
  localAiPath: boolean,
): void {
  if (!dom.refinementPipelineLive) return;

  if (!hasJob) {
    const runtime = getOllamaRuntimeCardState();
    const localAiPath = isLocalAiPathEnabled();
    const isCompatLocalNow = isCompatLocalPathEnabled();
    if (isCompatLocalNow && aiEnabled) {
      dom.refinementPipelineLive.textContent = describeIdleState(aiEnabled, rulesEnabled);
    } else if (localAiPath && (runtime.busy || runtime.backgroundStarting)) {
      dom.refinementPipelineLive.textContent = "Ollama runtime is starting. AI refiner will activate automatically when ready.";
    } else {
      dom.refinementPipelineLive.textContent = describeIdleState(aiEnabled, rulesEnabled);
    }
    return;
  }

  const jobToken = formatJobToken(pipelineJobState.jobId);
  switch (pipelineJobState.phase) {
    case "raw_emitted":
      dom.refinementPipelineLive.textContent = localAiPath && pipelineJobState.deferred
        ? `Job ${jobToken}: raw transcript ready, waiting for AI refinement${pipelineJobState.model ? ` (${pipelineJobState.model})` : ""}.`
        : `Job ${jobToken}: raw/rule output path selected.`;
      break;
    case "refining":
      dom.refinementPipelineLive.textContent =
        `Job ${jobToken}: AI refinement running in background${pipelineJobState.model ? ` (${pipelineJobState.model})` : ""}.`;
      break;
    case "refined":
      dom.refinementPipelineLive.textContent = `Job ${jobToken}: refined output ready${pipelineJobState.model ? ` (${pipelineJobState.model})` : ""}.`;
      break;
    case "failed":
      dom.refinementPipelineLive.textContent = `Job ${jobToken}: AI failed, raw fallback used${pipelineJobState.error ? ` (${pipelineJobState.error})` : ""}.`;
      break;
    case "timed_out":
      dom.refinementPipelineLive.textContent = `Job ${jobToken}: AI timeout, raw fallback pasted.`;
      break;
    default:
      dom.refinementPipelineLive.textContent = describeIdleState(aiEnabled, rulesEnabled);
      break;
  }
}

export function renderRefinementPipelineGraph(): void {
  if (!dom.refinementPipelineGraph) return;

  const aiEnabled = isAiRefinementModuleEnabled() && Boolean(settings?.ai_fallback?.enabled);
  const rulesEnabled = Boolean(settings?.postproc_enabled);
  const rulesReady = startupStatus?.rules_ready ?? true;
  const ollamaReady = Boolean(startupStatus?.ollama_ready);
  const ollamaStarting = Boolean(startupStatus?.ollama_starting);
  const localAiPath = isLocalAiPathEnabled();
  const isCompatLocal = isCompatLocalPathEnabled();
  const runtime = getOllamaRuntimeCardState();
  const hasJob = pipelineJobState.phase !== "idle" && pipelineJobState.jobId.trim().length > 0;
  const configuredAiModel = resolveConfiguredAiModel();
  const rulesVisualState: ToggleVisualState = rulesEnabled && rulesReady ? "on" : "off";
  const aiVisualState: ToggleVisualState = (aiEnabled && ollamaReady) || (aiEnabled && isCompatLocal)
    ? "on"
    : aiEnabled && ollamaStarting
      ? "pending"
      : "off";
  const aiBlocked = localAiPath
    && !runtime.healthy
    && !runtime.busy
    && !runtime.backgroundStarting;
  const aiWarming = localAiPath && (runtime.busy || runtime.backgroundStarting) && !runtime.healthy && !hasJob;

  const transcribeState: NodeState = !hasJob
    ? "idle"
    : pipelineJobState.phase === "raw_emitted" || pipelineJobState.phase === "refining"
      ? "active"
      : "success";
  const rulesState: NodeState = !rulesEnabled
    ? "bypassed"
    : !hasJob
      ? "idle"
      : pipelineJobState.phase === "raw_emitted" || pipelineJobState.phase === "refining"
        ? "active"
        : "success";

  let aiState: NodeState = "idle";
  if (!localAiPath && !isCompatLocal) {
    aiState = "bypassed";
  } else if (aiWarming) {
    aiState = "warming";
  } else if (aiBlocked) {
    aiState = "blocked";
  } else if (!hasJob) {
    aiState = "idle";
  } else if (pipelineJobState.phase === "refining") {
    aiState = "active";
  } else if (pipelineJobState.phase === "refined") {
    aiState = "success";
  } else if (pipelineJobState.phase === "failed") {
    aiState = "error";
  } else if (pipelineJobState.phase === "timed_out") {
    aiState = "timeout";
  } else if (localAiPath && pipelineJobState.deferred) {
    aiState = "active";
  }

  const gateEnabled = localAiPath || isCompatLocal;
  const gateState: NodeState = !gateEnabled
    ? "bypassed"
    : !hasJob
      ? "idle"
      : pipelineJobState.phase === "raw_emitted" || pipelineJobState.phase === "refining"
        ? "active"
        : pipelineJobState.phase === "refined"
          ? "success"
          : pipelineJobState.phase === "failed"
            ? "error"
            : pipelineJobState.phase === "timed_out"
              ? "timeout"
              : "idle";

  const refinedOutputState: NodeState = !hasJob
    ? "idle"
    : pipelineJobState.phase === "refined"
      ? "active"
      : "idle";

  const rawOutputState: NodeState = !hasJob
    ? "idle"
    : pipelineJobState.phase === "failed"
      || pipelineJobState.phase === "timed_out"
      || (!gateEnabled && pipelineJobState.phase !== "refined")
      ? "active"
      : "idle";

  setPipelineToggleVisualState(
    "pipeline-node-rules",
    rulesVisualState,
    rulesEnabled && rulesReady ? "Rule-based active" : "Enable rule-based",
  );
  setPipelineToggleVisualState(
    "pipeline-node-ai",
    aiVisualState,
    aiVisualState === "on"
      ? "AI refinement active"
      : aiVisualState === "pending"
        ? "AI refinement queued"
        : "Enable AI refinement",
  );
  const aiProvider = settings?.ai_fallback?.provider ?? "ollama";
  const providerLabels: Record<string, string> = {
    ollama: "Ollama",
    lm_studio: "LM Studio",
    oobabooga: "Oobabooga",
    claude: "Claude",
    openai: "OpenAI",
    gemini: "Gemini",
  };
  const providerLabel = providerLabels[aiProvider] ?? aiProvider;

  if (pipelineJobState.phase === "refined" && pipelineJobState.model.trim()) {
    setAiNodeCopy(`${providerLabel} refinement active. Model in use: ${pipelineJobState.model.trim()}.`);
  } else if (configuredAiModel) {
    setAiNodeCopy(
      aiVisualState === "on"
        ? `${providerLabel} refinement ready. Active model: ${configuredAiModel}.`
        : aiVisualState === "pending"
          ? `${providerLabel} refinement queued. Model selected: ${configuredAiModel}.`
          : `${providerLabel} local refinement for final wording. Selected model: ${configuredAiModel}.`,
    );
  } else {
    setAiNodeCopy(`${providerLabel} local refinement for final wording.`);
  }

  setNodeState("pipeline-node-transcribe", transcribeState);
  setNodeState("pipeline-node-rules", rulesState);
  setNodeState("pipeline-node-ai", aiState);
  setNodeState("pipeline-node-gate", gateState);
  setNodeState("pipeline-node-output-refined", refinedOutputState);
  setNodeState("pipeline-node-output-raw", rawOutputState);

  const useRulesBypassToAi = hasJob && localAiPath && !rulesEnabled;
  const useDirectRawBypass = hasJob && !localAiPath && !rulesEnabled;
  const useRulesOnlyRawPath =
    hasJob && !localAiPath && rulesEnabled && pipelineJobState.phase !== "refined";
  const useGateRawFallback =
    hasJob
    && gateEnabled
    && (pipelineJobState.phase === "failed" || pipelineJobState.phase === "timed_out");

  setEdgeState("pipeline-edge-transcribe-rules", hasJob && rulesEnabled ? "active" : "muted");
  setEdgeState("pipeline-edge-transcribe-ai", useRulesBypassToAi ? "active" : "muted");
  setEdgeState("pipeline-edge-transcribe-raw", useDirectRawBypass ? "active" : "muted");
  setEdgeState(
    "pipeline-edge-rules-ai",
    hasJob && localAiPath && rulesEnabled ? "active" : "muted",
  );
  setEdgeState("pipeline-edge-ai-gate", hasJob && gateEnabled ? "active" : "muted");
  setEdgeState("pipeline-edge-gate-refined", pipelineJobState.phase === "refined" ? "active" : "muted");
  setEdgeState("pipeline-edge-gate-raw", useGateRawFallback ? "active" : "muted");
  setEdgeState("pipeline-edge-rules-raw", useRulesOnlyRawPath ? "active" : "muted");

  updateLiveSummary(hasJob, aiEnabled, rulesEnabled, localAiPath);
}

export function syncRefinementPipelineGraphFromSettings(): void {
  renderRefinementPipelineGraph();
}

export function handlePipelineTranscriptionResult(payload: TranscriptionResultEvent): void {
  const jobId = (payload?.job_id || "").trim();
  if (!jobId) {
    return;
  }
  clearPipelineTerminalResetTimer();
  pipelineJobState.jobId = jobId;
  pipelineJobState.source = payload.source || "";
  pipelineJobState.phase = "raw_emitted";
  pipelineJobState.deferred = Boolean(payload.paste_deferred);
  pipelineJobState.model = "";
  pipelineJobState.error = "";
  renderRefinementPipelineGraph();
  if (!pipelineJobState.deferred) {
    schedulePipelineTerminalReset(jobId);
  }
}

export function handlePipelineRefinementStarted(payload: TranscriptionRefinementStartedEvent): void {
  const jobId = (payload?.job_id || "").trim();
  if (!jobId) return;
  clearPipelineTerminalResetTimer();
  pipelineJobState.jobId = jobId;
  pipelineJobState.source = payload.source || pipelineJobState.source;
  pipelineJobState.phase = "refining";
  pipelineJobState.model = payload.model || pipelineJobState.model;
  renderRefinementPipelineGraph();
}

export function handlePipelineRefined(payload: TranscriptionRefinedEvent): void {
  const jobId = (payload?.job_id || "").trim();
  if (!jobId) return;
  pipelineJobState.jobId = jobId;
  pipelineJobState.source = payload.source || pipelineJobState.source;
  pipelineJobState.phase = "refined";
  pipelineJobState.model = payload.model || "";
  pipelineJobState.error = "";
  renderRefinementPipelineGraph();
  schedulePipelineTerminalReset(jobId);
}

export function handlePipelineRefinementFailed(payload: TranscriptionRefinementFailedEvent): void {
  const jobId = (payload?.job_id || "").trim();
  if (!jobId) return;
  const reason = (payload.reason || "").trim();
  const reasonLabel = reason === "runtime_not_ready" ? "runtime_not_ready" : "";
  pipelineJobState.jobId = jobId;
  pipelineJobState.source = payload.source || pipelineJobState.source;
  pipelineJobState.phase = "failed";
  pipelineJobState.error = reasonLabel
    ? `${reasonLabel}: ${payload.error || ""}`.trim()
    : (payload.error || "");
  renderRefinementPipelineGraph();
  schedulePipelineTerminalReset(jobId);
}

export function handlePipelineRefinementTimeout(jobId: string): void {
  const normalized = (jobId || "").trim();
  if (!normalized) return;
  pipelineJobState.jobId = normalized;
  pipelineJobState.phase = "timed_out";
  renderRefinementPipelineGraph();
  schedulePipelineTerminalReset(normalized);
}

export function handlePipelineRefinementReset(reason: string): void {
  if (!pipelineJobState.jobId) return;
  pipelineJobState.phase = "failed";
  pipelineJobState.error = reason || "refinement reset";
  renderRefinementPipelineGraph();
  schedulePipelineTerminalReset(pipelineJobState.jobId);
}

export function reconcilePipelineRefinementIdle(reason: string): void {
  if (!pipelineJobState.jobId) return;
  if (pipelineJobState.phase !== "refining") return;
  pipelineJobState.phase = "failed";
  pipelineJobState.error = reason || "refinement finished without terminal event";
  renderRefinementPipelineGraph();
  schedulePipelineTerminalReset(pipelineJobState.jobId);
}
