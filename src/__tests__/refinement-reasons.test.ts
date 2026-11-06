import { describe, expect, it } from "vitest";
import {
  describeRefinementReason,
  isModelUnavailableReason,
  normalizeRefinementReason,
} from "../refinement-reasons";

const CONTRACT_CODES = [
  "model_not_loaded",
  "ps_query_failed",
  "gpu_busy",
  "cold_start_raw_first",
  "raw_timeout",
  "queue_full",
  "prepare_failed",
  "provider_error",
  "disabled",
];

describe("refinement reasons", () => {
  it("has readable English texts for every contract code", () => {
    for (const code of CONTRACT_CODES) {
      const text = describeRefinementReason(code);
      expect(text, code).not.toBeNull();
      expect(text!.label, code).not.toContain("_");
      expect(text!.past, code).not.toContain("_");
    }
    expect(describeRefinementReason("model_not_loaded")).toEqual({
      label: "model not loaded",
      past: "model was not loaded",
    });
  });

  it("maps the legacy gate code onto the unified one", () => {
    expect(normalizeRefinementReason("ollama_model_not_loaded")).toBe("model_not_loaded");
    expect(describeRefinementReason("ollama_model_not_loaded")?.label).toBe("model not loaded");
  });

  it("falls back to the spaced code for unknown reasons", () => {
    expect(describeRefinementReason("brand_new_code")).toEqual({
      label: "brand new code",
      past: "brand new code",
    });
    // Object prototype keys are not reasons.
    expect(describeRefinementReason("constructor")?.label).toBe("constructor");
  });

  it("returns null for missing input", () => {
    expect(describeRefinementReason(undefined)).toBeNull();
    expect(describeRefinementReason(null)).toBeNull();
    expect(describeRefinementReason("   ")).toBeNull();
  });

  it("flags the codes under which the model is unusable right now", () => {
    expect(isModelUnavailableReason("model_not_loaded")).toBe(true);
    expect(isModelUnavailableReason("ollama_model_not_loaded")).toBe(true);
    expect(isModelUnavailableReason("ps_query_failed")).toBe(true);
    expect(isModelUnavailableReason("cold_start_raw_first")).toBe(true);
    expect(isModelUnavailableReason("gpu_busy")).toBe(false);
    expect(isModelUnavailableReason("disabled")).toBe(false);
    expect(isModelUnavailableReason(null)).toBe(false);
  });
});
