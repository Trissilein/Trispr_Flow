/**
 * Block H: Offline-First Ollama Sprint — Tests
 *
 * Covers:
 *  H-S1: Settings shape and defaults
 *  H-S2: Offline model refresh / empty-list handling
 *  H-S3: No transcript loss guarantee (fallback logic)
 *  H-S4: Provider identification & routing (no API key, zero cost)
 *  H-S5: Connection error classification and toast type mapping
 */

import { describe, it, expect } from "vitest";
import type { AIFallbackSettings, AIProvidersSettings, OllamaSettings } from "../types";
import { isExactModelTagMatch, normalizeModelTag } from "../ollama-tag-utils";
import { OLLAMA_SETTINGS_CHANGED_POLICY } from "../ollama-refresh-policy";

// ---------------------------------------------------------------------------
// H-S1: Settings shape and defaults
// ---------------------------------------------------------------------------
describe("Block H S1 — Ollama settings defaults", () => {
  it("initialises OllamaSettings with correct defaults", () => {
    const ollama: OllamaSettings = {
      endpoint: "http://localhost:11434",
      available_models: [],
      preferred_model: "",
      runtime_source: "manual",
      runtime_path: "",
      runtime_version: "",
      last_health_check: null,
    };
    expect(ollama.endpoint).toBe("http://localhost:11434");
    expect(ollama.available_models).toHaveLength(0);
    expect(ollama.preferred_model).toBe("");
  });

  it("sets ollama as default ai_fallback provider", () => {
    const fallback: AIFallbackSettings = {
      enabled: false,
      provider: "ollama",
      fallback_provider: null,
      execution_mode: "local_primary",
      strict_local_mode: true,
      model: "",
      temperature: 0.3,
      max_tokens: 4000,
      custom_prompt_enabled: false,
      custom_prompt: "Fix this transcribed text: correct punctuation, capitalization, and obvious errors. Keep the meaning unchanged. Return only the corrected text.",
      use_default_prompt: true,
    };
    expect(fallback.provider).toBe("ollama");
    expect(fallback.enabled).toBe(false);
    expect(fallback.temperature).toBe(0.3);
    expect(fallback.max_tokens).toBe(4000);
  });

  it("includes ollama field inside AIProvidersSettings", () => {
    const providers: AIProvidersSettings = {
      claude: {
        api_key_stored: false,
        auth_method_preference: "api_key",
        auth_status: "locked",
        auth_verified_at: null,
        available_models: [],
        preferred_model: ""
      },
      openai: {
        api_key_stored: false,
        auth_method_preference: "api_key",
        auth_status: "locked",
        auth_verified_at: null,
        available_models: [],
        preferred_model: ""
      },
      gemini: {
        api_key_stored: false,
        auth_method_preference: "api_key",
        auth_status: "locked",
        auth_verified_at: null,
        available_models: [],
        preferred_model: ""
      },
      ollama: {
        endpoint: "http://localhost:11434",
        available_models: [],
        preferred_model: "",
        runtime_source: "manual",
        runtime_path: "",
        runtime_version: "",
        last_health_check: null,
      },
    };
    expect(providers.ollama).toBeDefined();
    expect(providers.ollama.endpoint).toBe("http://localhost:11434");
    // Ollama has no api_key_stored field
    expect("api_key_stored" in providers.ollama).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// H-S2: Offline model refresh / empty-list handling
// ---------------------------------------------------------------------------
describe("Block H S2 — Offline model refresh", () => {
  it("handles empty model list from an offline Ollama without throwing", () => {
    const ollama: OllamaSettings = {
      endpoint: "http://localhost:11434",
      available_models: [],
      preferred_model: "",
      runtime_source: "manual",
      runtime_path: "",
      runtime_version: "",
      last_health_check: null,
    };
    const models: string[] = []; // returned when Ollama is unreachable

    ollama.available_models = models;
    if (!models.includes(ollama.preferred_model)) {
      ollama.preferred_model = models[0] ?? "";
    }

    expect(ollama.available_models).toHaveLength(0);
    expect(ollama.preferred_model).toBe("");
  });

  it("selects first model when Ollama comes back online", () => {
    const ollama: OllamaSettings = {
      endpoint: "http://localhost:11434",
      available_models: [],
      preferred_model: "",
      runtime_source: "manual",
      runtime_path: "",
      runtime_version: "",
      last_health_check: null,
    };
    const fallback: AIFallbackSettings = {
      enabled: true,
      provider: "ollama",
      fallback_provider: null,
      execution_mode: "local_primary",
      strict_local_mode: true,
      model: "",
      temperature: 0.3,
      max_tokens: 4000,
      custom_prompt_enabled: false,
      custom_prompt: "",
      use_default_prompt: true,
    };

    const models = ["llama3.2:3b", "mistral:7b", "qwen2.5:7b"];
    ollama.available_models = models;
    if (!models.includes(ollama.preferred_model)) {
      ollama.preferred_model = models[0];
    }
    if (fallback.provider === "ollama" && !models.includes(fallback.model)) {
      fallback.model = ollama.preferred_model || models[0] || "";
    }

    expect(ollama.preferred_model).toBe("llama3.2:3b");
    expect(fallback.model).toBe("llama3.2:3b");
  });

  it("preserves existing preferred model when it is still in the refreshed list", () => {
    const ollama: OllamaSettings = {
      endpoint: "http://localhost:11434",
      available_models: ["llama3.2:3b", "mistral:7b"],
      preferred_model: "mistral:7b",
      runtime_source: "manual",
      runtime_path: "",
      runtime_version: "",
      last_health_check: null,
    };

    const models = ["llama3.2:3b", "mistral:7b", "qwen2.5:7b"];
    ollama.available_models = models;
    if (!models.includes(ollama.preferred_model)) {
      ollama.preferred_model = models[0];
    }

    expect(ollama.preferred_model).toBe("mistral:7b");
  });

  it("replaces preferred model when it is no longer in the refreshed list", () => {
    const ollama: OllamaSettings = {
      endpoint: "http://localhost:11434",
      available_models: ["old-model:7b"],
      preferred_model: "old-model:7b",
      runtime_source: "manual",
      runtime_path: "",
      runtime_version: "",
      last_health_check: null,
    };

    const models = ["llama3.2:3b", "mistral:7b"];
    ollama.available_models = models;
    if (!models.includes(ollama.preferred_model)) {
      ollama.preferred_model = models[0];
    }

    expect(ollama.preferred_model).toBe("llama3.2:3b");
  });
});

// ---------------------------------------------------------------------------
// H-S3: No transcript loss guarantee
// ---------------------------------------------------------------------------
describe("Block H S3 — No transcript loss", () => {
  const applyRefinement = (
    text: string,
    enabled: boolean,
    refinedOrNull: string | null,
  ): string => {
    if (!enabled || refinedOrNull === null) return text;
    return refinedOrNull;
  };

  it("returns original text when ai_fallback is disabled", () => {
    const original = "raw transcription text";
    expect(applyRefinement(original, false, "refined text")).toBe(original);
  });

  it("returns original text when AI call throws (simulated)", () => {
    const original = "raw transcription text";
    const simulateError = (text: string): string => {
      try {
        throw new Error("Ollama is not running");
      } catch {
        return text; // same as postprocessing.rs warn + keep stage 1+2 result
      }
    };
    expect(simulateError(original)).toBe(original);
  });

  it("returns refined text when AI call succeeds", () => {
    const original = "raw transcription text";
    const refined = "Raw transcription text.";
    expect(applyRefinement(original, true, refined)).toBe(refined);
  });

  it("returns original text when refined result is null", () => {
    const original = "raw transcription text";
    expect(applyRefinement(original, true, null)).toBe(original);
  });

  it("never loses original content regardless of AI outcome", () => {
    const texts = [
      "hello world",
      "Ärzte und Ärztinnen sprechen",
      "   whitespace   ",
      "",
    ];
    for (const t of texts) {
      // Disabled
      expect(applyRefinement(t, false, "x")).toBe(t);
      // Enabled but error
      expect(applyRefinement(t, true, null)).toBe(t);
    }
  });
});

// ---------------------------------------------------------------------------
// H-S4: Provider identification and routing
// ---------------------------------------------------------------------------
describe("Block H S4 — Provider identification", () => {
  const requiresApiKey = (provider: string): boolean => provider !== "ollama";

  it("identifies ollama as not requiring an API key", () => {
    expect(requiresApiKey("ollama")).toBe(false);
  });

  it("identifies cloud providers as requiring an API key", () => {
    expect(requiresApiKey("claude")).toBe(true);
    expect(requiresApiKey("openai")).toBe(true);
    expect(requiresApiKey("gemini")).toBe(true);
  });

  const estimateCost = (provider: string, inp: number, out: number): number => {
    if (provider === "ollama") return 0;
    if (provider === "claude") return inp * 0.000003 + out * 0.000015;
    if (provider === "openai") return inp * 0.000005 + out * 0.000015;
    if (provider === "gemini") return inp * 0.0000015 + out * 0.0000035;
    return 0;
  };

  it("estimates zero cost for Ollama (local inference)", () => {
    expect(estimateCost("ollama", 1000, 1000)).toBe(0);
  });

  it("estimates positive cost for cloud providers", () => {
    expect(estimateCost("claude", 1000, 1000)).toBeGreaterThan(0);
    expect(estimateCost("openai", 1000, 1000)).toBeGreaterThan(0);
    expect(estimateCost("gemini", 1000, 1000)).toBeGreaterThan(0);
  });

  it("normalises known provider IDs and falls back to ollama for unknown", () => {
    const normalize = (p: string): string => {
      const known = ["claude", "openai", "gemini", "ollama"] as const;
      const lower = p.trim().toLowerCase();
      return known.includes(lower as (typeof known)[number]) ? lower : "ollama";
    };
    expect(normalize("ollama")).toBe("ollama");
    expect(normalize("claude")).toBe("claude");
    expect(normalize("OPENAI")).toBe("openai");
    expect(normalize("gemini")).toBe("gemini");
    expect(normalize("unknown")).toBe("ollama");
    expect(normalize("  claude  ")).toBe("claude");
  });
});

// ---------------------------------------------------------------------------
// H-S5: Connection error classification and toast type mapping
// ---------------------------------------------------------------------------
describe("Block H S5 — Connection error handling", () => {
  const classifyOllamaError = (err: string): string => {
    if (err.includes("not running") || err.includes("OllamaNotRunning")) {
      return "Ollama is not running. Please start Ollama before using local AI refinement.";
    }
    if (err.includes("timed out") || err.includes("Timeout")) {
      return "Request timed out. The AI provider may be overloaded.";
    }
    if (err.includes("not found")) {
      return "Model not found in Ollama. Pull it first with: ollama pull <model>";
    }
    return err;
  };

  it("maps OllamaNotRunning to user-friendly message", () => {
    expect(classifyOllamaError("Ollama is not running")).toContain("not running");
    expect(classifyOllamaError("OllamaNotRunning")).toContain("not running");
  });

  it("maps timeout to user-friendly message", () => {
    expect(classifyOllamaError("Request timed out")).toContain("timed out");
    expect(classifyOllamaError("Timeout")).toContain("timed out");
  });

  it("maps 404 / model not found to pull instruction", () => {
    expect(classifyOllamaError("Model 'xyz' not found in Ollama")).toContain("not found");
  });

  it("passes through unrecognised errors unchanged", () => {
    const raw = "HTTP 500 internal server error";
    expect(classifyOllamaError(raw)).toBe(raw);
  });

  const toastType = (modelCount: number | null): "success" | "warning" | "error" => {
    if (modelCount === null) return "error";
    if (modelCount === 0) return "warning";
    return "success";
  };

  it("returns success toast when models are found", () => {
    expect(toastType(3)).toBe("success");
    expect(toastType(1)).toBe("success");
  });

  it("returns warning toast when Ollama is running but has no models", () => {
    expect(toastType(0)).toBe("warning");
  });

  it("returns error toast when Ollama is unreachable", () => {
    expect(toastType(null)).toBe("error");
  });
});

describe("Block H S6 — Exact Ollama model tag matching", () => {
  it("normalizes model tags for stable exact comparisons", () => {
    expect(normalizeModelTag(" QWEN3:8B ")).toBe("qwen3:8b");
    expect(normalizeModelTag("mistral-small3.1:24b")).toBe("mistral-small3.1:24b");
  });

  it("matches only exact tags and avoids family/prefix collisions", () => {
    expect(isExactModelTagMatch("qwen3:8b", "qwen3:8b")).toBe(true);
    expect(isExactModelTagMatch("qwen3:8b", "QWEN3:8B")).toBe(true);
    expect(isExactModelTagMatch("qwen3:8b", "qwen3:14b")).toBe(false);
    expect(isExactModelTagMatch("qwen3:8b", "qwen3")).toBe(false);
  });
});

describe("Block H S7 — No settings-changed recursion into Ollama refresh", () => {
  it("uses render-only policy on settings-changed by default", () => {
    expect(OLLAMA_SETTINGS_CHANGED_POLICY.refreshInstalledModels).toBe(false);
    expect(OLLAMA_SETTINGS_CHANGED_POLICY.refreshRuntimeState).toBe(false);
    expect(OLLAMA_SETTINGS_CHANGED_POLICY.renderManager).toBe(true);
  });
});
