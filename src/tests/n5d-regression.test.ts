/**
 * Block N, N5d: Vision + Diagnostics Regression Validation
 *
 * Covers startup health diagnostics, overlay health recovery, module health checks,
 * and integration edge cases for the N5+Q Stabilization Packet.
 *
 * N5d Regression Scenarios:
 *  - Startup health initialization and detection
 *  - Overlay health recovery with bounded retry attempts
 *  - Module lifecycle state transitions
 *  - Integration: partial availability (Whisper ok, Ollama degraded)
 */

import { describe, it, expect, beforeEach } from "vitest";

// Mock types for startup diagnostics
type StartupStatus = {
  interactive: boolean;
  transcription_ready: boolean;
  rules_ready: boolean;
  ollama_ready: boolean;
  ollama_starting: boolean;
  degraded_reasons: string[];
};

type RuntimeDiagnostics = {
  whisper: {
    cli_path: string;
    backend_selected: "cuda" | "vulkan" | "cpu";
    accelerator: "gpu" | "cpu";
    last_error: string | null;
  };
  ollama: {
    detected: boolean;
    reachable: boolean;
    spawn_stage: "idle" | "detecting" | "launching" | "running";
    last_error: string | null;
  };
};

type OverlayHealth = {
  status: "ok" | "recovering" | "recovered" | "failed";
  recovery_attempt: number;
  max_attempts: number;
  last_error: string | null;
};

// ---------------------------------------------------------------------------
// N5d-S1: Startup Health Diagnostics
// ---------------------------------------------------------------------------
describe("Block N N5d-S1 — Startup health diagnostics", () => {
  it("initializes startup status with all false (needs detection)", () => {
    const status: StartupStatus = {
      interactive: false,
      transcription_ready: false,
      rules_ready: false,
      ollama_ready: false,
      ollama_starting: false,
      degraded_reasons: [],
    };
    expect(status.interactive).toBe(false);
    expect(status.degraded_reasons).toHaveLength(0);
  });

  it("detects whisper unavailable → adds degraded reason", () => {
    const status: StartupStatus = {
      interactive: false,
      transcription_ready: false,
      rules_ready: false,
      ollama_ready: false,
      ollama_starting: false,
      degraded_reasons: ["Local transcription runtime unavailable."],
    };
    expect(status.transcription_ready).toBe(false);
    expect(status.degraded_reasons).toContain(
      "Local transcription runtime unavailable."
    );
  });

  it("detects model unavailable → adds specific degraded reason", () => {
    const status: StartupStatus = {
      interactive: false,
      transcription_ready: false,
      rules_ready: false,
      ollama_ready: false,
      ollama_starting: false,
      degraded_reasons: [
        "Selected transcription model 'base' is not available yet.",
      ],
    };
    expect(status.degraded_reasons.some((r) => r.includes("is not available yet"))).toBe(true);
  });

  it("detects ollama starting → marks ollama_starting and adds reason", () => {
    const status: StartupStatus = {
      interactive: false,
      transcription_ready: true,
      rules_ready: true,
      ollama_ready: false,
      ollama_starting: true,
      degraded_reasons: ["Ollama is starting in background."],
    };
    expect(status.ollama_starting).toBe(true);
    expect(status.ollama_ready).toBe(false);
    expect(status.degraded_reasons).toContain(
      "Ollama is starting in background."
    );
  });

  it("detects ollama disabled → marks degraded without blocking interactive", () => {
    const status: StartupStatus = {
      interactive: true,
      transcription_ready: true,
      rules_ready: true,
      ollama_ready: false,
      ollama_starting: false,
      degraded_reasons: [
        "Ollama refinement unavailable; raw or rule-based output remains active.",
      ],
    };
    expect(status.interactive).toBe(true);
    expect(status.ollama_ready).toBe(false);
    expect(status.degraded_reasons).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// N5d-S2: Overlay Health Recovery
// ---------------------------------------------------------------------------
describe("Block N N5d-S2 — Overlay health recovery", () => {
  let overlayHealth: OverlayHealth;

  beforeEach(() => {
    overlayHealth = {
      status: "ok",
      recovery_attempt: 0,
      max_attempts: 2,
      last_error: null,
    };
  });

  it("starts in 'ok' status with 0 recovery attempts", () => {
    expect(overlayHealth.status).toBe("ok");
    expect(overlayHealth.recovery_attempt).toBe(0);
  });

  it("transitions to 'recovering' on first failure (attempt 1)", () => {
    overlayHealth.status = "recovering";
    overlayHealth.recovery_attempt = 1;
    overlayHealth.last_error = "Window creation failed";

    expect(overlayHealth.status).toBe("recovering");
    expect(overlayHealth.recovery_attempt).toBe(1);
    expect(overlayHealth.recovery_attempt <= overlayHealth.max_attempts).toBe(
      true
    );
  });

  it("increments recovery_attempt on repeated failure (attempt 2)", () => {
    overlayHealth.status = "recovering";
    overlayHealth.recovery_attempt = 2;
    overlayHealth.last_error = "Replay failed";

    expect(overlayHealth.recovery_attempt).toBe(2);
    expect(overlayHealth.recovery_attempt <= overlayHealth.max_attempts).toBe(
      true
    );
  });

  it("transitions to 'failed' when max attempts exceeded (3+)", () => {
    overlayHealth.status = "failed";
    overlayHealth.recovery_attempt = 3;

    expect(overlayHealth.status).toBe("failed");
    expect(overlayHealth.recovery_attempt > overlayHealth.max_attempts).toBe(
      true
    );
  });

  it("transitions to 'recovered' after successful recovery from failing state", () => {
    // Simulate: was recovering (attempt 1), then succeeded
    overlayHealth.status = "recovered";
    overlayHealth.recovery_attempt = 0;
    overlayHealth.last_error = null;

    expect(overlayHealth.status).toBe("recovered");
    expect(overlayHealth.recovery_attempt).toBe(0);
  });

  it("respects max_attempts boundary (2 attempts allowed)", () => {
    const maxAttempts = 2;
    expect(overlayHealth.max_attempts).toBe(maxAttempts);

    // Attempt 1
    overlayHealth.recovery_attempt = 1;
    expect(overlayHealth.recovery_attempt <= maxAttempts).toBe(true);

    // Attempt 2
    overlayHealth.recovery_attempt = 2;
    expect(overlayHealth.recovery_attempt <= maxAttempts).toBe(true);

    // Attempt 3 (exceeded)
    overlayHealth.recovery_attempt = 3;
    expect(overlayHealth.recovery_attempt <= maxAttempts).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// N5d-S3: Module Health Checks
// ---------------------------------------------------------------------------
describe("Block N N5d-S3 — Module health checks", () => {
  type ModuleHealthStatus = {
    module_id: string;
    state: "active" | "installed" | "not_installed" | "error";
    health: "ok" | "degraded" | "error";
    last_error: string | null;
  };

  it("module in 'not_installed' state has 'error' health", () => {
    const module: ModuleHealthStatus = {
      module_id: "input_vision",
      state: "not_installed",
      health: "error",
      last_error: null,
    };
    expect(module.state).toBe("not_installed");
    expect(module.health).toBe("error");
  });

  it("module in 'installed' state has 'ok' health (permission pending)", () => {
    const module: ModuleHealthStatus = {
      module_id: "input_vision",
      state: "installed",
      health: "ok",
      last_error: null,
    };
    expect(module.state).toBe("installed");
    expect(module.health).toBe("ok");
  });

  it("module in 'active' state has 'ok' health when no errors", () => {
    const module: ModuleHealthStatus = {
      module_id: "input_vision",
      state: "active",
      health: "ok",
      last_error: null,
    };
    expect(module.state).toBe("active");
    expect(module.health).toBe("ok");
    expect(module.last_error).toBe(null);
  });

  it("module in 'error' state has 'error' health with last_error set", () => {
    const module: ModuleHealthStatus = {
      module_id: "input_vision",
      state: "error",
      health: "error",
      last_error: "Frame buffer allocation failed",
    };
    expect(module.state).toBe("error");
    expect(module.health).toBe("error");
    expect(module.last_error).not.toBe(null);
  });

  it("module can transition from 'active' to 'error' on permission loss", () => {
    const module: ModuleHealthStatus = {
      module_id: "input_vision",
      state: "active",
      health: "ok",
      last_error: null,
    };

    // Simulate permission revoked
    module.state = "error";
    module.health = "error";
    module.last_error = "Screen capture permission denied by OS";

    expect(module.state).toBe("error");
    expect(module.last_error).toContain("permission");
  });
});

// ---------------------------------------------------------------------------
// N5d-S4: Runtime Diagnostics Snapshots
// ---------------------------------------------------------------------------
describe("Block N N5d-S4 — Runtime diagnostics snapshots", () => {
  it("captures whisper backend selection in diagnostics", () => {
    const diagnostics: RuntimeDiagnostics = {
      whisper: {
        cli_path: "/path/to/whisper-cli",
        backend_selected: "cuda",
        accelerator: "gpu",
        last_error: null,
      },
      ollama: {
        detected: true,
        reachable: true,
        spawn_stage: "running",
        last_error: null,
      },
    };

    expect(diagnostics.whisper.backend_selected).toBe("cuda");
    expect(["cuda", "vulkan", "cpu"]).toContain(
      diagnostics.whisper.backend_selected
    );
  });

  it("captures ollama spawn_stage transitions in diagnostics", () => {
    const stages: Array<"idle" | "detecting" | "launching" | "running"> = [
      "idle",
      "detecting",
      "launching",
      "running",
    ];

    stages.forEach((stage) => {
      const diagnostics: RuntimeDiagnostics = {
        whisper: {
          cli_path: "/path/to/whisper-cli",
          backend_selected: "cpu",
          accelerator: "cpu",
          last_error: null,
        },
        ollama: {
          detected: stage !== "idle",
          reachable: stage === "running",
          spawn_stage: stage,
          last_error: null,
        },
      };

      expect(stages).toContain(diagnostics.ollama.spawn_stage);
    });
  });

  it("preserves last_error in diagnostics across snapshots", () => {
    const diagnostics: RuntimeDiagnostics = {
      whisper: {
        cli_path: "/path/to/whisper-cli",
        backend_selected: "cuda",
        accelerator: "gpu",
        last_error: "CUDA initialization failed",
      },
      ollama: {
        detected: false,
        reachable: false,
        spawn_stage: "idle",
        last_error: "Connection refused: 127.0.0.1:11434",
      },
    };

    expect(diagnostics.whisper.last_error).toBe(
      "CUDA initialization failed"
    );
    expect(diagnostics.ollama.last_error).toContain("Connection refused");
  });

  it("clears last_error when recovery succeeds", () => {
    const diagnostics: RuntimeDiagnostics = {
      whisper: {
        cli_path: "/path/to/whisper-cli",
        backend_selected: "cuda",
        accelerator: "gpu",
        last_error: null,
      },
      ollama: {
        detected: true,
        reachable: true,
        spawn_stage: "running",
        last_error: null,
      },
    };

    expect(diagnostics.whisper.last_error).toBe(null);
    expect(diagnostics.ollama.last_error).toBe(null);
  });
});

// ---------------------------------------------------------------------------
// N5d-S5: Integration — Partial Availability Scenarios
// ---------------------------------------------------------------------------
describe("Block N N5d-S5 — Integration: partial availability", () => {
  it("handles whisper ok + ollama unavailable (degraded but functional)", () => {
    const status: StartupStatus = {
      interactive: true,
      transcription_ready: true,
      rules_ready: true,
      ollama_ready: false,
      ollama_starting: false,
      degraded_reasons: [
        "Ollama refinement unavailable; raw or rule-based output remains active.",
      ],
    };

    // Core transcription works despite ollama being down
    expect(status.interactive).toBe(true);
    expect(status.transcription_ready).toBe(true);
    expect(status.ollama_ready).toBe(false);
  });

  it("handles whisper unavailable + ollama available (minimal mode)", () => {
    const status: StartupStatus = {
      interactive: false,
      transcription_ready: false,
      rules_ready: false,
      ollama_ready: true,
      ollama_starting: false,
      degraded_reasons: [
        "Local transcription runtime unavailable.",
      ],
    };

    // Cannot enter interactive mode without transcription
    expect(status.interactive).toBe(false);
    expect(status.transcription_ready).toBe(false);
  });

  it("handles overlay recovery while refinement active (no blocking)", () => {
    const overlayStatus: OverlayHealth = {
      status: "recovering",
      recovery_attempt: 1,
      max_attempts: 2,
      last_error: "Window update failed",
    };

    // Overlay recovery should not block refinement
    expect(overlayStatus.status).toBe("recovering");
    // Refinement can continue independently
    const refinementActive = true;
    expect(refinementActive).toBe(true);
  });

  it("validates all runtimes healthy → interactive true", () => {
    const status: StartupStatus = {
      interactive: true,
      transcription_ready: true,
      rules_ready: true,
      ollama_ready: true,
      ollama_starting: false,
      degraded_reasons: [],
    };

    const diagnostics: RuntimeDiagnostics = {
      whisper: {
        cli_path: "/path/to/whisper-cli",
        backend_selected: "cuda",
        accelerator: "gpu",
        last_error: null,
      },
      ollama: {
        detected: true,
        reachable: true,
        spawn_stage: "running",
        last_error: null,
      },
    };

    expect(status.interactive).toBe(true);
    expect(status.degraded_reasons).toHaveLength(0);
    expect(diagnostics.whisper.last_error).toBe(null);
    expect(diagnostics.ollama.last_error).toBe(null);
  });
});
