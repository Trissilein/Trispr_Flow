import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  live: { textContent: "" } as { textContent: string },
  state: {
    settings: null as unknown,
    startupStatus: null as unknown,
  },
}));

vi.mock("../dom-refs", () => ({
  refinementPipelineGraph: {},
  get refinementPipelineLive() {
    return mocks.live;
  },
}));

vi.mock("../state", () => ({
  get settings() {
    return mocks.state.settings;
  },
  get startupStatus() {
    return mocks.state.startupStatus;
  },
}));

vi.mock("../ollama-models", () => ({
  getOllamaRuntimeCardState: () => ({ healthy: true, busy: false, backgroundStarting: false }),
}));

import {
  handlePipelineRefinementFailed,
  handlePipelineRefinementStarted,
  handlePipelineTranscriptionResult,
} from "../refinement-pipeline-graph";

const NODES = [
  "pipeline-node-transcribe",
  "pipeline-node-rules",
  "pipeline-node-ai",
  "pipeline-node-gate",
  "pipeline-node-output-refined",
  "pipeline-node-output-raw",
  "pipeline-edge-gate-raw",
];

function nodeState(id: string): string | undefined {
  return document.getElementById(id)?.dataset.state;
}

function emitRaw(jobId: string, skippedReason: string | null, pasteDeferred = false): void {
  handlePipelineTranscriptionResult({
    text: "hello",
    source: "mic",
    job_id: jobId,
    paste_deferred: pasteDeferred,
    refinement_gate: {
      enabled: true,
      provider: "ollama",
      model: "gemma",
      should_refine: skippedReason === null,
      paste_deferred: pasteDeferred,
      skipped_reason: skippedReason,
    },
  });
}

describe("refinement pipeline graph: model not loaded", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = NODES.map((id) => `<div id="${id}"></div>`).join("");
    mocks.live.textContent = "";
    mocks.state.startupStatus = { rules_ready: true, ollama_ready: true, ollama_starting: false };
    mocks.state.settings = {
      postproc_enabled: true,
      ai_fallback: { enabled: true, provider: "ollama", model: "gemma" },
      providers: { ollama: { preferred_model: "gemma" } },
    };
  });

  it("marks the raw output node active when raw was pasted because the model was not loaded", () => {
    emitRaw("job-not-loaded", "ollama_model_not_loaded");

    expect(nodeState("pipeline-node-output-raw")).toBe("active");
    expect(nodeState("pipeline-edge-gate-raw")).toBe("active");
    expect(nodeState("pipeline-node-output-refined")).toBe("idle");
    expect(nodeState("pipeline-node-ai")).toBe("warming");
  });

  it("uses a neutral message instead of claiming background loading", () => {
    emitRaw("job-not-loaded", "ollama_model_not_loaded");

    expect(mocks.live.textContent).toContain("model not loaded");
    expect(mocks.live.textContent).toContain("raw pasted");
    expect(mocks.live.textContent).not.toContain("loading in background");
  });

  it("keeps raw active while the warm-up refinement runs", () => {
    emitRaw("job-warmup", "ollama_model_not_loaded");
    handlePipelineRefinementStarted({ job_id: "job-warmup", source: "mic", original: "hello", model: "gemma" });

    expect(nodeState("pipeline-node-output-raw")).toBe("active");
  });

  it("keeps raw idle while a deferred refinement is genuinely pending", () => {
    emitRaw("job-deferred", null, true);

    expect(nodeState("pipeline-node-output-raw")).toBe("idle");
  });

  it("says the model is not installed when the warm-up fails with a 404", () => {
    emitRaw("job-404", "ollama_model_not_loaded");
    handlePipelineRefinementFailed({
      job_id: "job-404",
      source: "mic",
      error: "Model 'gemma' not found in Ollama",
      reason: "provider_error",
    });

    expect(mocks.live.textContent).toContain("model not installed");
    expect(mocks.live.textContent).toContain("raw pasted");
    expect(mocks.live.textContent).not.toContain("loading in background");
    expect(nodeState("pipeline-node-output-raw")).toBe("active");
    expect(nodeState("pipeline-node-ai")).toBe("error");
  });

  it("keeps the generic failure text for other errors", () => {
    emitRaw("job-500", "ollama_model_not_loaded");
    handlePipelineRefinementFailed({
      job_id: "job-500",
      source: "mic",
      error: "HTTP 500 internal server error",
      reason: "provider_error",
    });

    expect(mocks.live.textContent).toContain("AI failed, raw fallback used");
    expect(mocks.live.textContent).not.toContain("model not installed");
  });
});
