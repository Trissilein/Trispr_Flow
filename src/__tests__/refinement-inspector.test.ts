// Inspector meta line: gate skip reason shown as readable text.
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const el = () => ({ style: { display: "" }, textContent: "", innerHTML: "", classList: { add() {}, remove() {} } });
  return {
    dom: {
      refinementInspectorEmpty: el(),
      refinementInspectorContent: el(),
      refinementInspectorMeta: el(),
      refinementInspectorRaw: el(),
      refinementInspectorRefined: el(),
      refinementInspectorDiff: el(),
      refinementInspectorError: el(),
      refinementInspectorStatus: undefined,
    },
  };
});

vi.mock("../dom-refs", () => mocks.dom);
vi.mock("../state", () => ({ isRefinementEnabled: () => true }));

import { handleTranscriptionResultForInspector } from "../refinement-inspector";

function emit(jobId: string, skippedReason: string | null): string {
  handleTranscriptionResultForInspector({
    text: "hello",
    source: "mic",
    job_id: jobId,
    refinement_gate: {
      enabled: true,
      provider: "ollama",
      model: "gemma",
      should_refine: skippedReason === null,
      paste_deferred: false,
      skipped_reason: skippedReason,
    },
  });
  return mocks.dom.refinementInspectorMeta.textContent;
}

describe("refinement inspector gate text", () => {
  beforeEach(() => {
    mocks.dom.refinementInspectorMeta.textContent = "";
  });

  it("shows a readable reason for each skip code", () => {
    expect(emit("j1", "model_not_loaded")).toContain("gate: model not loaded");
    expect(emit("j2", "ps_query_failed")).toContain("gate: Ollama status unavailable");
    expect(emit("j3", "gpu_busy")).toContain("gate: GPU busy");
  });

  it("maps the legacy gate code", () => {
    expect(emit("j4", "ollama_model_not_loaded")).toContain("gate: model not loaded");
  });

  it("still says refine / skip when there is no reason", () => {
    expect(emit("j5", null)).toContain("gate: refine");
    expect(emit("j6", "")).toContain("gate: skip");
  });
});
