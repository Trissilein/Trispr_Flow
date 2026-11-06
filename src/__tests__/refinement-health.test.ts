// refinement:health event -> status light + toasts (contract: SELFHEAL-BRIEF, Vertrag 1).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";

const light = vi.hoisted(() => ({ setOllamaModelState: vi.fn() }));
vi.mock("../ui-state", () => light);

import {
  handleRefinementHealth,
  listenRefinementHealth,
  resetRefinementHealth,
} from "../refinement-health";
import type { RefinementHealthEvent } from "../types";

function ev(
  state: RefinementHealthEvent["state"],
  reason = "model_not_loaded",
  attempt = 1,
): RefinementHealthEvent {
  return { state, reason, message: "backend sentence", attempt, ts_ms: 1_700_000_000_000 };
}

/** Visible toasts (a dismissed toast keeps the "removing" class for 200 ms). */
function visibleToasts(): Array<{ type: string; title: string; message: string }> {
  return Array.from(document.querySelectorAll("#toast-container .toast:not(.removing)")).map((el) => ({
    type: el.className.replace("toast", "").trim(),
    title: el.querySelector(".toast-title")?.textContent ?? "",
    message: el.querySelector(".toast-message")?.textContent ?? "",
  }));
}

function settle(): void {
  vi.advanceTimersByTime(300);
}

describe("refinement:health", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = '<div id="toast-container"></div>';
    resetRefinementHealth();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("listens on the contract event name and hands back an unlisten", async () => {
    const unlisten = await listenRefinementHealth();

    expect(vi.mocked(listen).mock.calls.map((call) => call[0])).toContain("refinement:health");
    expect(typeof unlisten).toBe("function");
  });

  it("event -> toast + light: recovering shows one warning toast and a loading light", async () => {
    await listenRefinementHealth();
    const call = vi.mocked(listen).mock.calls.find((c) => c[0] === "refinement:health");
    const emit = call![1] as (event: { payload: unknown }) => void;

    emit({ payload: ev("recovering") });

    expect(visibleToasts()).toEqual([
      { type: "warning", title: "Refinement paused", message: "Model not loaded. Reloading…" },
    ]);
    expect(light.setOllamaModelState).toHaveBeenLastCalledWith("loading");
  });

  it("makes the recovering text depend on the reason", () => {
    handleRefinementHealth(ev("recovering", "ps_query_failed"));
    expect(visibleToasts()[0].message).toBe("Ollama status unavailable. Reloading…");

    resetRefinementHealth();
    settle();
    handleRefinementHealth(ev("recovering", "some_future_code"));
    expect(visibleToasts()[0].message).toBe("Some future code. Reloading…");
  });

  it("shows one toast per episode: repeated recovering events stay silent", () => {
    handleRefinementHealth(ev("recovering", "model_not_loaded", 1));
    handleRefinementHealth(ev("recovering", "model_not_loaded", 2));
    handleRefinementHealth(ev("recovering", "raw_timeout", 3));

    // No dismissed-and-recreated toast either (flicker): exactly one toast element ever created.
    expect(document.querySelectorAll("#toast-container .toast")).toHaveLength(1);
    expect(visibleToasts()).toHaveLength(1);
    // The light still follows every event.
    expect(light.setOllamaModelState).toHaveBeenCalledTimes(3);
    expect(light.setOllamaModelState).toHaveBeenLastCalledWith("loading");
  });

  it("recovered replaces the recovering toast and turns the light warm", () => {
    handleRefinementHealth(ev("recovering"));
    handleRefinementHealth(ev("recovered"));
    settle();

    expect(visibleToasts()).toEqual([
      {
        type: "success",
        title: "Refinement restored",
        message: "The model is loaded again. Dictation is refined as usual.",
      },
    ]);
    expect(light.setOllamaModelState).toHaveBeenLastCalledWith("warm");
  });

  it("failed replaces the recovering toast, says dictation continues raw, light goes gray", () => {
    handleRefinementHealth(ev("recovering"));
    handleRefinementHealth(ev("failed", "provider_error"));
    settle();

    expect(visibleToasts()).toEqual([
      {
        type: "warning",
        title: "Refinement unavailable",
        message: "Dictation continues as raw text.",
      },
    ]);
    expect(light.setOllamaModelState).toHaveBeenLastCalledWith("cold");
  });

  it("a duplicated closing event does not toast twice", () => {
    handleRefinementHealth(ev("recovering"));
    handleRefinementHealth(ev("failed"));
    handleRefinementHealth(ev("failed"));
    settle();

    expect(visibleToasts()).toHaveLength(1);
  });

  it("a new episode toasts again", () => {
    handleRefinementHealth(ev("recovering"));
    handleRefinementHealth(ev("recovered"));
    settle();
    handleRefinementHealth(ev("recovering"));

    const titles = visibleToasts().map((toast) => toast.title);
    expect(titles).toEqual(["Refinement restored", "Refinement paused"]);
  });

  it("ignores malformed payloads", () => {
    handleRefinementHealth(null);
    handleRefinementHealth(undefined);
    handleRefinementHealth({ ...ev("recovering"), state: "bogus" as never });

    expect(visibleToasts()).toEqual([]);
    expect(light.setOllamaModelState).not.toHaveBeenCalled();
  });

  it("reset dismisses the open toast and forgets the episode", () => {
    handleRefinementHealth(ev("recovering"));
    resetRefinementHealth();
    settle();
    expect(visibleToasts()).toEqual([]);

    handleRefinementHealth(ev("recovering"));
    expect(visibleToasts()).toHaveLength(1);
  });
});
