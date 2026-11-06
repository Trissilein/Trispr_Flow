import { describe, expect, it } from "vitest";
import {
  GENERIC_HINT,
  buildDiagnosticsText,
  buildWizardSteps,
  describeProgress,
  formatMegabytes,
  isRuntimeMissingMessage,
  maskUserPaths,
  orderPackages,
  planAfterSelfTest,
  toRuntimeError,
  variantStatus,
  type ManualPackage,
  type RuntimeVerifyResult,
  type SetupDiagnostics,
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

describe("toRuntimeError", () => {
  it("takes the structured error of the backend as it is", () => {
    expect(
      toRuntimeError({ code: "offline", message: "Could not reach the server.", hint: "Check the VPN.", detail: "Dns Failed" })
    ).toEqual({ code: "offline", message: "Could not reach the server.", hint: "Check the VPN.", detail: "Dns Failed" });
  });

  it("fills a missing hint and code, so the panel always has both", () => {
    expect(toRuntimeError({ message: "boom" })).toEqual({ code: "unknown", message: "boom", hint: GENERIC_HINT, detail: "" });
  });

  it("wraps a plain string, an Error and a strange value", () => {
    expect(toRuntimeError("download failed")).toMatchObject({ code: "unknown", message: "download failed", hint: GENERIC_HINT });
    expect(toRuntimeError(new Error("nope"))).toMatchObject({ code: "unknown", message: "nope" });
    expect(toRuntimeError({ foo: 1 })).toMatchObject({ code: "unknown", message: '{"foo":1}' });
    expect(toRuntimeError(null).message).not.toBe("");
    expect(toRuntimeError(undefined).hint).toBe(GENERIC_HINT);
  });
});

describe("orderPackages", () => {
  const pkg = (variant: "cuda" | "vulkan"): ManualPackage => ({
    variant,
    label: variant,
    asset: `${variant}.zip`,
    url: "https://github.com/x",
    size_mb: 1,
    size_bytes: 1,
    sha256: "a".repeat(64),
    which_gpu_hint: "",
  });

  it("puts the preferred package first, Vulkan without a preference, and keeps the input intact", () => {
    const input = [pkg("cuda"), pkg("vulkan")];
    expect(orderPackages(input, "cuda").map((p) => p.variant)).toEqual(["cuda", "vulkan"]);
    expect(orderPackages(input, "vulkan").map((p) => p.variant)).toEqual(["vulkan", "cuda"]);
    expect(orderPackages(input, null).map((p) => p.variant)).toEqual(["vulkan", "cuda"]);
    expect(input.map((p) => p.variant)).toEqual(["cuda", "vulkan"]);
  });
});

describe("diagnostics text", () => {
  const diagnostics: SetupDiagnostics = {
    app_version: "0.10.0",
    os: "Windows 10.0.26300",
    arch: "x86_64",
    runtime_dir: "%LOCALAPPDATA%\\Trispr Flow\\runtime",
    whisper_cpp_version: "v1.9.5",
    adapters: [{ vendor_id: "0x10DE", name: "NVIDIA GeForce RTX 5070 Ti" }],
    variants: [
      { variant: "cuda", bundled: false, downloaded: false },
      { variant: "vulkan", bundled: false, downloaded: true },
    ],
  };

  it("carries version, OS, GPU detection, engine, error code and text, and the engine folder", () => {
    const text = buildDiagnosticsText({
      diagnostics,
      variant: "cuda",
      error: { code: "checksum", message: "The file is damaged.", hint: "Retry.", detail: "Expected a, got b." },
      recommendation: { recommended: "cuda", gpu_name: "NVIDIA GeForce RTX 5070 Ti", nvidia: true, reason: "CUDA is the fastest." },
    });
    for (const expected of [
      "App version: 0.10.0",
      "OS: Windows 10.0.26300 (x86_64)",
      "Engine folder: %LOCALAPPDATA%",
      "GPU detection: recommends cuda; NVIDIA GeForce RTX 5070 Ti",
      "Installed: cuda=missing, vulkan=downloaded",
      "Engine in question: cuda",
      "Error code: checksum",
      "Error: The file is damaged.",
      "Technical detail: Expected a, got b.",
    ]) {
      expect(text).toContain(expected);
    }
  });

  it("still works when nothing could be read, and never carries the user name", () => {
    const text = buildDiagnosticsText({
      diagnostics: null,
      variant: null,
      error: { code: "unknown", message: "Failed at C:\\Users\\Max Muster\\AppData\\x", hint: "", detail: "C:/Users/Max/x" },
      recommendation: null,
    });
    expect(text).toContain("GPU detection: failed");
    expect(text).not.toMatch(/Max/);
    expect(maskUserPaths("C:\\Users\\Max\\AppData")).toBe("%USERPROFILE%\\AppData");
    expect(maskUserPaths("d:/users/Max/x")).toBe("%USERPROFILE%/x");
    expect(maskUserPaths("D:\\tmp\\runtime")).toBe("D:\\tmp\\runtime");
  });
});

describe("isRuntimeMissingMessage", () => {
  it("recognises the texts the backend uses for a missing or broken speech engine", () => {
    for (const text of [
      "Whisper runtime is missing or incomplete (whisper-cli not found at 'C:\\x'). Restart Trispr Flow to download the speech engine again in the setup wizard, or reinstall Trispr Flow.",
      "Whisper runtime executable is missing.",
      "Whisper CUDA runtime is incomplete for 'C:\\x\\whisper-cli.exe'; missing: cublasLt64_13.dll",
    ]) {
      expect(isRuntimeMissingMessage(text), text).toBe(true);
    }
    for (const text of ["Transcription timed out.", "No audio was recorded.", "Ollama endpoint not reachable"]) {
      expect(isRuntimeMissingMessage(text), text).toBe(false);
    }
  });
});
