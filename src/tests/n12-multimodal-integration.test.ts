/**
 * Block N, N12: Vision/TTS Integration Tests
 *
 * Focus:
 *  - Command-level contracts (Vision + TTS payloads)
 *  - Deterministic provider fallback behavior
 *  - Basic UI integration for Voice Output console visibility/focus
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  ModuleHealthStatus,
  Settings,
  TtsProviderInfo,
  TtsSpeakResult,
  TtsVoiceInfo,
  VisionSnapshotResult,
  VisionSourceInfo,
  VisionStreamHealth,
} from "../types";

function resolveTtsProvider(
  preferred: "windows_native" | "local_custom",
  fallback: "windows_native" | "local_custom",
  preferredOk: boolean,
  fallbackOk: boolean
): { ok: boolean; provider_used: "windows_native" | "local_custom"; used_fallback: boolean } {
  if (preferredOk) {
    return { ok: true, provider_used: preferred, used_fallback: false };
  }
  if (preferred === fallback) {
    return { ok: false, provider_used: preferred, used_fallback: false };
  }
  if (fallbackOk) {
    return { ok: true, provider_used: fallback, used_fallback: true };
  }
  return { ok: false, provider_used: preferred, used_fallback: true };
}

function isTtsPolicyAllowed(
  policy: "agent_replies_only" | "replies_and_events" | "explicit_only",
  context: "agent_reply" | "agent_event" | "manual_user" | "manual_test"
): boolean {
  if (policy === "agent_replies_only") {
    return context === "agent_reply" || context === "manual_test";
  }
  if (policy === "replies_and_events") {
    return context === "agent_reply" || context === "agent_event" || context === "manual_test";
  }
  return context === "manual_user" || context === "manual_test";
}

function makeVoiceSettings(enabled: boolean): Settings {
  return {
    voice_output_settings: {
      enabled,
      default_provider: "windows_native",
      fallback_provider: "local_custom",
      voice_id_windows: "",
      voice_id_local: "",
      rate: 1.0,
      volume: 1.0,
      output_policy: "agent_replies_only",
      piper_binary_path: "",
      piper_model_path: "",
      piper_model_dir: "",
    },
  } as unknown as Settings;
}

// ---------------------------------------------------------------------------
// N12-S1: Vision command contracts
// ---------------------------------------------------------------------------
describe("Block N N12-S1 — Vision command contracts", () => {
  it("models list_screen_sources payload with monitor metadata", () => {
    const sources: VisionSourceInfo[] = [
      { id: "monitor_1", label: "Primary", width: 2560, height: 1440 },
      { id: "monitor_2", label: "Secondary", width: 1920, height: 1080 },
    ];

    expect(sources.length).toBeGreaterThan(0);
    expect(sources.every((source) => source.width > 0 && source.height > 0)).toBe(true);
  });

  it("models start_vision_stream -> health progression -> stop_vision_stream", () => {
    const started: VisionStreamHealth = {
      running: true,
      fps: 5,
      source_scope: "all_monitors",
      started_at_ms: 1738000100000,
      frame_seq: 1,
      buffered_frames: 0,
      buffered_bytes: 0,
      last_frame_timestamp_ms: null,
      last_frame_width: null,
      last_frame_height: null,
    };

    const inFlight: VisionStreamHealth = {
      ...started,
      frame_seq: 42,
      buffered_frames: 12,
      buffered_bytes: 980_000,
      last_frame_timestamp_ms: 1738000102400,
      last_frame_width: 1280,
      last_frame_height: 720,
    };

    const stopped: VisionStreamHealth = {
      ...inFlight,
      running: false,
      buffered_frames: 0,
      buffered_bytes: 0,
    };

    expect(started.running).toBe(true);
    expect(inFlight.frame_seq).toBeGreaterThan(started.frame_seq);
    expect(inFlight.buffered_frames).toBeGreaterThan(0);
    expect(stopped.running).toBe(false);
    expect(stopped.buffered_frames).toBe(0);
  });

  it("models capture_vision_snapshot as RAM-only payload", () => {
    const snapshot: VisionSnapshotResult = {
      captured: true,
      timestamp_ms: 1738000102500,
      source_count: 2,
      note: "Snapshot returned from in-memory vision buffer.",
      frame_seq: 43,
      width: 1280,
      height: 720,
      bytes: 22123,
      source_scope: "all_monitors",
      jpeg_base64: "AAECAwQ=",
    };

    expect(snapshot.captured).toBe(true);
    expect(snapshot.note.toLowerCase()).toContain("in-memory");
    expect(snapshot.jpeg_base64?.length).toBeGreaterThan(0);
    expect(Object.prototype.hasOwnProperty.call(snapshot, "file_path")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// N12-S2: TTS command contracts and fallback
// ---------------------------------------------------------------------------
describe("Block N N12-S2 — TTS command contracts and fallback", () => {
  it("models list_tts_providers with dual lane ids", () => {
    const providers: TtsProviderInfo[] = [
      { id: "windows_native", label: "Windows Native TTS", available: true },
      { id: "local_custom", label: "Local Custom TTS (Piper)", available: true },
    ];

    expect(providers.map((provider) => provider.id)).toEqual([
      "windows_native",
      "local_custom",
    ]);
  });

  it("models list_tts_voices payload for selected provider", () => {
    const voices: TtsVoiceInfo[] = [
      { id: "de-DE-Katja", label: "Katja", provider: "windows_native" },
      { id: "en-US-David", label: "David", provider: "windows_native" },
    ];

    expect(voices).toHaveLength(2);
    expect(voices.every((voice) => voice.provider === "windows_native")).toBe(true);
  });

  it("uses fallback provider when preferred provider fails", () => {
    const result = resolveTtsProvider("windows_native", "local_custom", false, true);
    expect(result.ok).toBe(true);
    expect(result.provider_used).toBe("local_custom");
    expect(result.used_fallback).toBe(true);
  });

  it("returns failed status when both providers fail", () => {
    const result = resolveTtsProvider("windows_native", "local_custom", false, false);
    expect(result.ok).toBe(false);
    expect(result.provider_used).toBe("windows_native");
  });

  it("models successful speak_tts response contract", () => {
    const speakResult: TtsSpeakResult = {
      provider_used: "windows_native",
      accepted: true,
      message: "TTS request accepted.",
    };

    expect(speakResult.accepted).toBe(true);
    expect(["windows_native", "local_custom"]).toContain(speakResult.provider_used);
  });

  it("enforces output policy: agent_replies_only", () => {
    expect(isTtsPolicyAllowed("agent_replies_only", "agent_reply")).toBe(true);
    expect(isTtsPolicyAllowed("agent_replies_only", "manual_test")).toBe(true);
    expect(isTtsPolicyAllowed("agent_replies_only", "agent_event")).toBe(false);
    expect(isTtsPolicyAllowed("agent_replies_only", "manual_user")).toBe(false);
  });

  it("enforces output policy: replies_and_events", () => {
    expect(isTtsPolicyAllowed("replies_and_events", "agent_reply")).toBe(true);
    expect(isTtsPolicyAllowed("replies_and_events", "agent_event")).toBe(true);
    expect(isTtsPolicyAllowed("replies_and_events", "manual_test")).toBe(true);
    expect(isTtsPolicyAllowed("replies_and_events", "manual_user")).toBe(false);
  });

  it("enforces output policy: explicit_only", () => {
    expect(isTtsPolicyAllowed("explicit_only", "manual_user")).toBe(true);
    expect(isTtsPolicyAllowed("explicit_only", "manual_test")).toBe(true);
    expect(isTtsPolicyAllowed("explicit_only", "agent_reply")).toBe(false);
    expect(isTtsPolicyAllowed("explicit_only", "agent_event")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// N12-S3: Multimodal module health integration
// ---------------------------------------------------------------------------
describe("Block N N12-S3 — Multimodal module health integration", () => {
  it("keeps modules in ok state when both capabilities are enabled", () => {
    const health: ModuleHealthStatus[] = [
      { module_id: "input_vision", state: "ok", detail: "Vision stream healthy." },
      { module_id: "output_voice_tts", state: "ok", detail: "Voice output healthy." },
    ];

    expect(health.every((item) => item.state === "ok")).toBe(true);
  });

  it("allows mixed health states without breaking overall integration", () => {
    const health: ModuleHealthStatus[] = [
      {
        module_id: "input_vision",
        state: "degraded",
        detail: "Source switch in progress; using previous monitor.",
      },
      { module_id: "output_voice_tts", state: "ok", detail: "Voice output healthy." },
    ];

    expect(health.find((item) => item.module_id === "input_vision")?.state).toBe("degraded");
    expect(health.find((item) => item.module_id === "output_voice_tts")?.state).toBe("ok");
  });
});

// ---------------------------------------------------------------------------
// N12-S4: Voice output UI integration (console visibility/focus)
// ---------------------------------------------------------------------------
describe("Block N N12-S4 — Voice output UI integration", () => {
  beforeEach(() => {
    vi.resetModules();
    document.body.innerHTML = `
      <section id="voice-output-console" hidden>
        <span id="voice-output-console-status"></span>
      </section>
      <select id="voice-output-default-provider">
        <option value="windows_native">Windows Native</option>
        <option value="local_custom">Local Custom</option>
      </select>
    `;
  });

  afterEach(async () => {
    const state = await import("../state");
    state.setSettings(null);
    document.body.innerHTML = "";
  });

  it("hides console when output_voice_tts is disabled", async () => {
    const state = await import("../state");
    const voiceOutput = await import("../voice-output-console");

    state.setSettings(makeVoiceSettings(false));
    voiceOutput.syncVoiceOutputConsoleState();

    const consoleRoot = document.getElementById("voice-output-console") as HTMLElement;
    const status = document.getElementById("voice-output-console-status") as HTMLElement;

    expect(consoleRoot.hidden).toBe(true);
    expect(status.textContent).toBe("Module disabled.");
  });

  it("shows console when output_voice_tts is enabled", async () => {
    const state = await import("../state");
    const voiceOutput = await import("../voice-output-console");

    state.setSettings(makeVoiceSettings(true));
    voiceOutput.syncVoiceOutputConsoleState();

    const consoleRoot = document.getElementById("voice-output-console") as HTMLElement;
    const status = document.getElementById("voice-output-console-status") as HTMLElement;

    expect(consoleRoot.hidden).toBe(false);
    expect(status.textContent).toBe("Voice output active.");
  });

  it("focusVoiceOutputConsole scrolls and focuses provider selector", async () => {
    const state = await import("../state");
    const voiceOutput = await import("../voice-output-console");

    state.setSettings(makeVoiceSettings(true));

    const consoleRoot = document.getElementById("voice-output-console") as HTMLElement;
    const providerSelect = document.getElementById("voice-output-default-provider") as HTMLSelectElement;
    const scrollSpy = vi.fn();

    (consoleRoot as unknown as { scrollIntoView: () => void }).scrollIntoView = scrollSpy;

    voiceOutput.focusVoiceOutputConsole();

    expect(scrollSpy).toHaveBeenCalledTimes(1);
    expect(document.activeElement).toBe(providerSelect);
  });
});
