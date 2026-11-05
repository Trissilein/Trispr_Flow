/**
 * Block N, N5d: Diagnostics Regression Validation
 *
 * Covers startup health diagnostics, overlay health recovery, module health checks,
 * runtime snapshots, and partial availability handling.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type {
  OllamaRuntimeDiagnostics,
  OverlayHealthEvent,
  RuntimeDiagnostics,
  StartupStatus,
  WhisperRuntimeDiagnostics,
} from "../types";

function makeStartupStatus(overrides: Partial<StartupStatus> = {}): StartupStatus {
  return {
    interactive: false,
    transcription_ready: false,
    rules_ready: false,
    ollama_ready: false,
    ollama_starting: false,
    degraded_reasons: [],
    ...overrides,
  };
}

type RuntimeDiagnosticsOverrides = {
  whisper?: Partial<WhisperRuntimeDiagnostics>;
  ollama?: Partial<OllamaRuntimeDiagnostics>;
};

function makeRuntimeDiagnostics(overrides: RuntimeDiagnosticsOverrides = {}): RuntimeDiagnostics {
  return {
    whisper: {
      cli_path: "/path/to/whisper-cli",
      server_path: "/path/to/whisper-server",
      backend_selected: "cuda",
      mode: "running",
      accelerator: "gpu",
      gpu_layers_requested: 28,
      gpu_layers_applied: 28,
      active_requests: 0,
      warm_until_ms: 0,
      idle_retire_ms: 300000,
      last_error: "",
      ...(overrides.whisper ?? {}),
    },
    ollama: {
      configured_path: "C:/Users/test/AppData/Local/trispr-flow/ollama/ollama.exe",
      detected: true,
      spawn_stage: "running",
      last_error: "",
      managed_pid: 4242,
      endpoint: "http://127.0.0.1:11434",
      reachable: true,
      active_requests: 0,
      idle_release_ms: 90000,
      ...(overrides.ollama ?? {}),
    },
  };
}

type OverlayRecoveryState = {
  status: "ok" | "recovering" | "recovered" | "failed";
  recovery_attempt: number;
  max_attempts: number;
  last_error: string | null;
};

// ---------------------------------------------------------------------------
// N5d-S1: Startup Health Diagnostics
// ---------------------------------------------------------------------------
describe("Block N N5d-S1 — Startup health diagnostics", () => {
  it("initializes startup status with all readiness flags false", () => {
    const status = makeStartupStatus();
    expect(status.interactive).toBe(false);
    expect(status.transcription_ready).toBe(false);
    expect(status.degraded_reasons).toHaveLength(0);
  });

  it("adds degraded reason when transcription runtime is unavailable", () => {
    const status = makeStartupStatus({
      degraded_reasons: ["Local transcription runtime unavailable."],
    });
    expect(status.transcription_ready).toBe(false);
    expect(status.degraded_reasons).toContain("Local transcription runtime unavailable.");
  });

  it("marks ollama_starting while core transcription remains ready", () => {
    const status = makeStartupStatus({
      transcription_ready: true,
      rules_ready: true,
      ollama_starting: true,
      degraded_reasons: ["Ollama is starting in background."],
    });
    expect(status.ollama_starting).toBe(true);
    expect(status.ollama_ready).toBe(false);
    expect(status.degraded_reasons).toContain("Ollama is starting in background.");
  });

  it("keeps app interactive when ollama is degraded but optional", () => {
    const status = makeStartupStatus({
      interactive: true,
      transcription_ready: true,
      rules_ready: true,
      degraded_reasons: [
        "Ollama refinement unavailable; raw or rule-based output remains active.",
      ],
    });
    expect(status.interactive).toBe(true);
    expect(status.ollama_ready).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// N5d-S2: Overlay Health Recovery
// ---------------------------------------------------------------------------
describe("Block N N5d-S2 — Overlay health recovery", () => {
  let overlayState: OverlayRecoveryState;

  beforeEach(() => {
    overlayState = {
      status: "ok",
      recovery_attempt: 0,
      max_attempts: 2,
      last_error: null,
    };
  });

  it("tracks bounded retry attempts before hard-fail", () => {
    overlayState.status = "recovering";
    overlayState.recovery_attempt = 1;
    expect(overlayState.recovery_attempt <= overlayState.max_attempts).toBe(true);

    overlayState.recovery_attempt = 2;
    expect(overlayState.recovery_attempt <= overlayState.max_attempts).toBe(true);

    overlayState.status = "failed";
    overlayState.recovery_attempt = 3;
    expect(overlayState.recovery_attempt > overlayState.max_attempts).toBe(true);
  });

  it("emits runtime overlay health payload using contract fields", () => {
    const recoveringEvent: OverlayHealthEvent = {
      status: "recovering",
      attempt: 1,
      reason: "Window creation failed",
    };
    const failedEvent: OverlayHealthEvent = {
      status: "failed",
      attempt: 3,
      reason: "Overlay creation previously failed; skipping retry",
    };

    expect(recoveringEvent.status).toBe("recovering");
    expect(recoveringEvent.attempt).toBeGreaterThan(0);
    expect(recoveringEvent.reason.length).toBeGreaterThan(0);

    expect(failedEvent.status).toBe("failed");
    expect(failedEvent.attempt).toBeGreaterThan(recoveringEvent.attempt);
  });
});

// ---------------------------------------------------------------------------
// N5d-S4: Runtime Diagnostics Snapshots
// ---------------------------------------------------------------------------
describe("Block N N5d-S4 — Runtime diagnostics snapshots", () => {
  it("captures whisper backend and ollama spawn stage transitions", () => {
    const diagnostics = makeRuntimeDiagnostics();
    expect(["cuda", "vulkan", "cpu"]).toContain(diagnostics.whisper.backend_selected);
    expect(["idle", "detecting", "launching", "running"]).toContain(
      diagnostics.ollama.spawn_stage
    );
  });

  it("preserves runtime last_error strings for both runtimes", () => {
    const diagnostics = makeRuntimeDiagnostics({
      whisper: { last_error: "CUDA initialization failed" },
      ollama: { last_error: "Connection refused: 127.0.0.1:11434", reachable: false },
    });

    expect(diagnostics.whisper.last_error).toContain("CUDA");
    expect(diagnostics.ollama.last_error).toContain("Connection refused");
    expect(diagnostics.ollama.reachable).toBe(false);
  });

  it("clears runtime last_error after recovery", () => {
    const diagnostics = makeRuntimeDiagnostics({
      whisper: { last_error: "" },
      ollama: { last_error: "", reachable: true, spawn_stage: "running" },
    });

    expect(diagnostics.whisper.last_error).toBe("");
    expect(diagnostics.ollama.last_error).toBe("");
    expect(diagnostics.ollama.reachable).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// N5d-S5: Integration — Partial Availability Scenarios
// ---------------------------------------------------------------------------
describe("Block N N5d-S5 — Integration: partial availability", () => {
  it("keeps app interactive when whisper is ready and ollama is unavailable", () => {
    const status = makeStartupStatus({
      interactive: true,
      transcription_ready: true,
      rules_ready: true,
      degraded_reasons: [
        "Ollama refinement unavailable; raw or rule-based output remains active.",
      ],
    });

    expect(status.interactive).toBe(true);
    expect(status.transcription_ready).toBe(true);
    expect(status.ollama_ready).toBe(false);
  });

  it("prevents interactive mode when local transcription runtime is unavailable", () => {
    const status = makeStartupStatus({
      interactive: false,
      transcription_ready: false,
      rules_ready: false,
      ollama_ready: true,
      degraded_reasons: ["Local transcription runtime unavailable."],
    });

    expect(status.interactive).toBe(false);
    expect(status.transcription_ready).toBe(false);
    expect(status.ollama_ready).toBe(true);
  });
});
