import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import type { OllamaRuntimeVersionInfo } from "../types";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

vi.mock("../toast", () => ({
  showToast: vi.fn(),
}));

const CATALOG_EVENT = "ollama-runtime-catalog-updated";

function entry(version: string, overrides: Partial<OllamaRuntimeVersionInfo> = {}): OllamaRuntimeVersionInfo {
  return {
    version,
    source: "online",
    selected: false,
    installed: false,
    recommended: false,
    prerelease: false,
    installable: true,
    ...overrides,
  };
}

const ONLINE = [entry("0.40.7", { recommended: true, installed: true }), entry("0.35.1"), entry("0.30.4")];
const CACHED = [entry("0.40.7", { recommended: true, installed: true }), entry("0.30.4", { installed: true, installable: false })];

type Handler = (args?: Record<string, unknown>) => unknown;

describe("fetchOnlineVersionCatalog / runtime version catalog", () => {
  let warn: ReturnType<typeof vi.spyOn>;
  let eventSpy: Mock<(event: Event) => void>;

  async function load(handlers: Record<string, Handler>) {
    vi.resetModules();
    const { invoke } = await import("@tauri-apps/api/core");
    const mod = await import("../ollama-models");
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    vi.mocked(invoke).mockImplementation((async (command: string, args?: Record<string, unknown>) => {
      calls.push({ command, args });
      const handler = handlers[command];
      return handler ? handler(args) : {};
    }) as never);
    const callsOf = (command: string) => calls.filter((call) => call.command === command);
    return { mod, calls, callsOf };
  }

  beforeEach(() => {
    warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    eventSpy = vi.fn<(event: Event) => void>();
    window.addEventListener(CATALOG_EVENT, eventSpy);
  });

  afterEach(() => {
    window.removeEventListener(CATALOG_EVENT, eventSpy);
    warn.mockRestore();
  });

  it("success: sets the catalog, clears the status and announces the update", async () => {
    const ctx = await load({ fetch_ollama_online_versions: () => ONLINE });
    const statuses: Array<string | null> = [];

    await ctx.mod.fetchOnlineVersionCatalog((msg) => statuses.push(msg));

    expect(statuses).toEqual(["Fetching version list from GitHub...", null]);
    expect(ctx.mod.getOllamaRuntimeVersionCatalog()).toEqual(ONLINE);
    expect(eventSpy).toHaveBeenCalledTimes(1);
    expect(ctx.mod.RUNTIME_CATALOG_UPDATED_EVENT).toBe(CATALOG_EVENT);
    expect(ctx.mod.isOnlineVersionFetchInProgress()).toBe(false);
    expect(ctx.callsOf("list_ollama_runtime_versions")).toHaveLength(0);
  });

  it("passes force through to the backend (button = true by default, automatic fetch = false)", async () => {
    const ctx = await load({ fetch_ollama_online_versions: () => ONLINE });

    await ctx.mod.fetchOnlineVersionCatalog(() => {});
    await ctx.mod.fetchOnlineVersionCatalog(() => {}, false);
    await ctx.mod.fetchOnlineVersionCatalog(() => {}, true);

    expect(ctx.callsOf("fetch_ollama_online_versions").map((call) => call.args)).toEqual([
      { force: true },
      { force: false },
      { force: true },
    ]);
  });

  it("failure: shows the backend's error text and reloads the cached list", async () => {
    const backendError =
      "Failed to list Ollama releases on GitHub: http status: 403 Showing the last cached list.";
    const ctx = await load({
      fetch_ollama_online_versions: () => {
        throw backendError; // Tauri rejects with the Err(String) itself, not an Error
      },
      list_ollama_runtime_versions: () => CACHED,
    });
    const statuses: Array<string | null> = [];

    await ctx.mod.fetchOnlineVersionCatalog((msg) => statuses.push(msg));

    expect(statuses[0]).toBe("Fetching version list from GitHub...");
    expect(statuses[statuses.length - 1]).toBe(`Could not fetch the version list: ${backendError}`);
    expect(statuses).not.toContain(null);
    expect(ctx.callsOf("list_ollama_runtime_versions")).toHaveLength(1);
    expect(ctx.mod.getOllamaRuntimeVersionCatalog()).toEqual(CACHED);
    expect(eventSpy).not.toHaveBeenCalled();
    expect(ctx.mod.isOnlineVersionFetchInProgress()).toBe(false);
  });

  it("failure with an Error object uses its message", async () => {
    const ctx = await load({
      fetch_ollama_online_versions: () => {
        throw new Error("socket closed");
      },
      list_ollama_runtime_versions: () => CACHED,
    });
    const statuses: Array<string | null> = [];

    await ctx.mod.fetchOnlineVersionCatalog((msg) => statuses.push(msg));

    expect(statuses[statuses.length - 1]).toBe("Could not fetch the version list: socket closed");
  });

  it("failure never empties the list: previous catalog stays when the reload fails too", async () => {
    let fail = false;
    const ctx = await load({
      fetch_ollama_online_versions: () => {
        if (fail) throw "offline";
        return ONLINE;
      },
      list_ollama_runtime_versions: () => {
        throw "backend busy";
      },
    });
    await ctx.mod.fetchOnlineVersionCatalog(() => {});
    expect(ctx.mod.getOllamaRuntimeVersionCatalog()).toEqual(ONLINE);

    fail = true;
    const statuses: Array<string | null> = [];
    await ctx.mod.fetchOnlineVersionCatalog((msg) => statuses.push(msg));

    expect(statuses[statuses.length - 1]).toBe("Could not fetch the version list: offline");
    expect(ctx.mod.getOllamaRuntimeVersionCatalog()).toEqual(ONLINE);
  });

  it("a second request while one is running is ignored, and the flag resets afterwards", async () => {
    let release!: (versions: OllamaRuntimeVersionInfo[]) => void;
    const pending = new Promise<OllamaRuntimeVersionInfo[]>((resolve) => {
      release = resolve;
    });
    const ctx = await load({ fetch_ollama_online_versions: () => pending });

    const first = ctx.mod.fetchOnlineVersionCatalog(() => {});
    expect(ctx.mod.isOnlineVersionFetchInProgress()).toBe(true);
    const secondStatuses: Array<string | null> = [];
    await ctx.mod.fetchOnlineVersionCatalog((msg) => secondStatuses.push(msg));
    expect(secondStatuses).toEqual([]);
    expect(ctx.callsOf("fetch_ollama_online_versions")).toHaveLength(1);

    release(ONLINE);
    await first;
    expect(ctx.mod.isOnlineVersionFetchInProgress()).toBe(false);
  });

  it("the first cached refresh triggers exactly one automatic online fetch per session", async () => {
    const ctx = await load({
      list_ollama_runtime_versions: () => CACHED,
      fetch_ollama_online_versions: () => ONLINE,
    });

    await ctx.mod.refreshOllamaRuntimeVersionCatalog(true);
    await vi.waitFor(() => expect(ctx.callsOf("fetch_ollama_online_versions")).toHaveLength(1));
    expect(ctx.callsOf("fetch_ollama_online_versions")[0].args).toEqual({ force: false });
    await vi.waitFor(() => expect(ctx.mod.getOllamaRuntimeVersionCatalog()).toEqual(ONLINE));

    await ctx.mod.refreshOllamaRuntimeVersionCatalog(true);
    await Promise.resolve();
    expect(ctx.callsOf("fetch_ollama_online_versions")).toHaveLength(1);
  });
});
