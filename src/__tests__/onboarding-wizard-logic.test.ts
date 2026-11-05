import { describe, expect, it } from "vitest";
import {
  buildWizardSteps,
  describeProgress,
  formatMegabytes,
  planAfterSelfTest,
  variantStatus,
  type RuntimeVerifyResult,
  type WhisperRuntimeStatus,
} from "../onboarding-wizard-logic";

function status(opts: { cudaReady?: boolean; vulkanReady?: boolean }): WhisperRuntimeStatus {
  const variant = (name: "cuda" | "vulkan", ready: boolean) => ({
    variant: name,
    bundled: false,
    downloaded: ready,
    ready,
    zip_size: name === "cuda" ? 509_246_371 : 15_711_052,
  });
  const variants = [variant("cuda", Boolean(opts.cudaReady)), variant("vulkan", Boolean(opts.vulkanReady))];
  return {
    variants,
    any_ready: variants.some((v) => v.ready),
    whisper_cpp_version: "v1.9.5",
    runtime_dir: "C:\\runtime",
  };
}

const failedCuda: RuntimeVerifyResult = {
  variant: "cuda",
  ok: false,
  detail: "The NVIDIA driver is too old.",
  fallback_variant: "vulkan",
};

describe("buildWizardSteps", () => {
  it("first run without a runtime shows every step, engine first", () => {
    expect(buildWizardSteps({ runtimeReady: false, modelInstalled: false, completed: false })).toEqual([
      "runtime",
      "model",
      "mode",
      "hotkey",
      "refinement",
    ]);
  });

  it("first run with a bundled or downloaded runtime skips the engine step", () => {
    expect(buildWizardSteps({ runtimeReady: true, modelInstalled: false, completed: false })).toEqual([
      "model",
      "mode",
      "hotkey",
      "refinement",
    ]);
  });

  it("someone with a model or a finished wizard is left alone while the engine exists", () => {
    expect(buildWizardSteps({ runtimeReady: true, modelInstalled: true, completed: false })).toEqual([]);
    expect(buildWizardSteps({ runtimeReady: true, modelInstalled: false, completed: true })).toEqual([]);
  });

  it("a missing engine reopens the wizard with the engine step alone", () => {
    expect(buildWizardSteps({ runtimeReady: false, modelInstalled: true, completed: true })).toEqual(["runtime"]);
    expect(buildWizardSteps({ runtimeReady: false, modelInstalled: true, completed: false })).toEqual(["runtime"]);
  });
});

describe("planAfterSelfTest", () => {
  it("a passing test is done", () => {
    const ok: RuntimeVerifyResult = { variant: "cuda", ok: true, detail: "whisper.cpp 1.9.5", fallback_variant: null };
    expect(planAfterSelfTest(ok, status({ cudaReady: true }))).toEqual({ action: "ready" });
  });

  it("CUDA failing loads Vulkan when it is not on disk", () => {
    expect(planAfterSelfTest(failedCuda, status({ cudaReady: true }))).toEqual({
      action: "download",
      variant: "vulkan",
    });
    expect(planAfterSelfTest(failedCuda, null)).toEqual({ action: "download", variant: "vulkan" });
  });

  it("CUDA failing switches to Vulkan when it is already there", () => {
    expect(planAfterSelfTest(failedCuda, status({ cudaReady: true, vulkanReady: true }))).toEqual({
      action: "switch",
      variant: "vulkan",
    });
  });

  it("any other failure is reported with its reason", () => {
    const failedVulkan: RuntimeVerifyResult = {
      variant: "vulkan",
      ok: false,
      detail: "whisper-cli did not answer.",
      fallback_variant: null,
    };
    expect(planAfterSelfTest(failedVulkan, status({ vulkanReady: true }))).toEqual({
      action: "failed",
      message: "whisper-cli did not answer.",
    });
  });
});

describe("progress text", () => {
  it("shows megabytes and percent when the total is known", () => {
    expect(describeProgress(15_728_640, 31_457_280, "Downloading")).toEqual({
      percent: 50,
      text: "15 MB / 30 MB (50%)",
    });
  });

  it("stays indeterminate without a total and falls back to the message without bytes", () => {
    expect(describeProgress(2_097_152, null, "x")).toEqual({ percent: null, text: "2 MB downloaded" });
    expect(describeProgress(null, null, "Verifying download...")).toEqual({
      percent: null,
      text: "Verifying download...",
    });
  });

  it("clamps overshoot and formats sizes", () => {
    expect(describeProgress(200, 100, "x").percent).toBe(100);
    expect(formatMegabytes(509_246_371)).toBe("486 MB");
  });
});

describe("variantStatus", () => {
  it("finds a variant or nothing", () => {
    expect(variantStatus(status({ vulkanReady: true }), "vulkan")?.ready).toBe(true);
    expect(variantStatus(null, "cuda")).toBeUndefined();
  });
});
