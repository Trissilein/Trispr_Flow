import { describe, expect, it } from "vitest";
import type { OllamaRuntimeVersionInfo } from "../types";
import {
  DEFAULT_RUNTIME_VERSION,
  buildRuntimeVersionRows,
  formatDownloadSize,
  formatReleaseDate,
  healStaleRuntimeTarget,
  syncRuntimeTarget,
} from "../ollama-runtime-versions";

const base: OllamaRuntimeVersionInfo = {
  version: "0.40.7",
  source: "online",
  selected: false,
  installed: false,
  recommended: false,
  prerelease: false,
  installable: true,
};

const e = (version: string, overrides: Partial<OllamaRuntimeVersionInfo> = {}): OllamaRuntimeVersionInfo => ({
  ...base,
  version,
  ...overrides,
});

const ids = (rows: ReturnType<typeof buildRuntimeVersionRows>) => rows.map((row) => row.id);
const actionIds = (rows: ReturnType<typeof buildRuntimeVersionRows>, id: string) =>
  rows.find((row) => row.id === id)?.actions?.map((action) => action.id);
const chipLabels = (rows: ReturnType<typeof buildRuntimeVersionRows>, id: string) => {
  const cell = rows.find((row) => row.id === id)?.cells.status;
  return Array.isArray(cell) ? cell.map((chip) => chip.label) : [];
};

describe("ollama runtime version display", () => {
  it("formats release dates and decimal download sizes", () => {
    expect(formatReleaseDate("2026-10-18T00:28:57Z")).toBe("18 Oct 2026");
    expect(formatReleaseDate("garbage")).toBe("");
    expect(formatReleaseDate(null)).toBe("");
    expect(formatDownloadSize(1_582_514_985)).toBe("1.6 GB");
    expect(formatDownloadSize(450_000_000)).toBe("450 MB");
    expect(formatDownloadSize(undefined)).toBe("");
  });
});

describe("buildRuntimeVersionRows", () => {
  const catalog = [
    e("0.30.4", { installed: true, installable: false }),
    e("0.40.7", { installed: true }),
    e("0.40.13", { installed: true, recommended: true, latest: true }),
    e("0.35.1"),
    e("0.41.0-rc1", { prerelease: true }),
    e("0.99.0", { installable: false, installable_reason: "No checksum." }),
  ];
  const rows = (overrides: Partial<Parameters<typeof buildRuntimeVersionRows>[0]> = {}) =>
    buildRuntimeVersionRows({
      catalog,
      runningVersion: "0.40.13",
      targetVersion: "0.40.13",
      recordedVersion: "0.40.13",
      busy: false,
      ...overrides,
    });

  it("puts the running version first, then newest first (numeric, not textual)", () => {
    expect(ids(rows())).toEqual(["0.40.13", "0.99.0", "0.41.0-rc1", "0.40.7", "0.35.1", "0.30.4"]);
    expect(ids(rows({ runningVersion: "0.30.4", targetVersion: "0.30.4", recordedVersion: "0.30.4" }))[0]).toBe("0.30.4");
    // 0.9.0 sorts after 0.40.13 as text
    expect(ids(buildRuntimeVersionRows({ catalog: [e("0.9.0"), e("0.40.13")], runningVersion: null, targetVersion: "", busy: false }))).toEqual(["0.40.13", "0.9.0"]);
  });

  it("offers Install, Switch to and Delete only where they make sense", () => {
    const all = rows();
    expect(actionIds(all, "0.40.13")).toEqual([]); // running
    expect(actionIds(all, "0.40.7")).toEqual(["switch", "delete"]); // installed
    expect(actionIds(all, "0.30.4")).toEqual(["switch", "delete"]); // installed, not in the catalog
    expect(actionIds(all, "0.35.1")).toEqual(["install"]); // verified download
    expect(actionIds(all, "0.99.0")).toEqual([]); // not installable
    expect(all.find((row) => row.id === "0.99.0")?.marker?.title).toBe("No checksum.");
    expect(all.find((row) => row.id === "0.40.13")?.marker?.label).toBe("Running");
  });

  it("never offers Delete for the target or the recorded binary", () => {
    const all = rows({ targetVersion: "0.40.7", recordedVersion: "0.30.4" });
    expect(actionIds(all, "0.40.7")).toEqual(["switch"]);
    expect(actionIds(all, "0.30.4")).toEqual(["switch"]);
    expect(chipLabels(all, "0.40.7")).toContain("Target");
  });

  it("with nothing running every installed row can be switched to, and none is marked current", () => {
    const all = rows({ runningVersion: null });
    expect(all.some((row) => row.current)).toBe(false);
    expect(actionIds(all, "0.40.13")).toEqual(["switch"]);
  });

  it("chips: Recommended, Latest, Prerelease, Installed; no Target chip on the running row", () => {
    const all = rows();
    expect(chipLabels(all, "0.40.13")).toEqual(["Recommended", "Latest"]);
    expect(chipLabels(all, "0.41.0-rc1")).toEqual(["Prerelease"]);
    expect(chipLabels(all, "0.40.7")).toEqual(["Installed"]);
  });

  it("lists a running or selected version the catalog does not know, and ignores 'unknown' and a v prefix", () => {
    const all = buildRuntimeVersionRows({
      catalog: [e("0.40.13", { installed: true })],
      runningVersion: "v0.38.2",
      targetVersion: "0.50.0",
      busy: false,
    });
    expect(ids(all)).toEqual(["0.38.2", "0.50.0", "0.40.13"]);
    expect(actionIds(all, "0.38.2")).toEqual([]);
    expect(actionIds(all, "0.50.0")).toEqual([]); // unverified: no Install

    expect(ids(buildRuntimeVersionRows({ catalog: [], runningVersion: "unknown", targetVersion: "", busy: false }))).toEqual([]);
  });

  it("an empty catalog with nothing running gives an empty list", () => {
    expect(buildRuntimeVersionRows({ catalog: [], runningVersion: null, targetVersion: "", busy: false })).toEqual([]);
  });

  it("keeps the first entry of a duplicated version", () => {
    const all = buildRuntimeVersionRows({
      catalog: [e("0.40.7", { installed: true }), e("0.40.7", { installed: false })],
      runningVersion: null,
      targetVersion: "",
      busy: false,
    });
    expect(ids(all)).toEqual(["0.40.7"]);
    expect(actionIds(all, "0.40.7")).toEqual(["switch", "delete"]);
  });

  it("busy keeps the buttons but makes them inert with the reason as title", () => {
    const all = rows({ busy: true });
    for (const row of all) {
      for (const action of row.actions ?? []) {
        expect(action.disabled).toBe(true);
        expect(action.title).toBe("Another runtime operation is running.");
      }
    }
    expect(all.find((row) => row.id === "0.35.1")?.actions?.[0]?.id).toBe("install");
  });

  it("texts of the Install button mention the download size when known", () => {
    const all = buildRuntimeVersionRows({
      catalog: [e("0.35.1", { size_bytes: 1_400_000_000 })],
      runningVersion: null,
      targetVersion: "",
      busy: false,
    });
    expect(all[0].actions?.[0].title).toContain("1.4 GB");
  });
});

describe("runtime target healing", () => {
  const fields = (overrides: Record<string, unknown> = {}) => ({
    runtime_source: "per_user_zip",
    runtime_version: "0.40.13",
    runtime_target_version: "0.30.4",
    runtime_target_explicit: false,
    ...overrides,
  });

  it("lifts a stale implicit target to the running runtime", () => {
    const ollama = fields();
    expect(healStaleRuntimeTarget(ollama)).toBe(true);
    expect(ollama.runtime_target_version).toBe("0.40.13");
    expect(healStaleRuntimeTarget(ollama)).toBe(false);
  });

  it("prefers the detected version over the recorded one", () => {
    const ollama = fields({ runtime_version: "0.30.4", runtime_target_version: "0.35.1" });
    healStaleRuntimeTarget(ollama, "0.40.13");
    expect(ollama.runtime_target_version).toBe("0.40.13");
  });

  it("leaves an explicit target, a newer or equal target, other sources and odd versions alone", () => {
    const cases: Array<[Record<string, unknown>, string | undefined]> = [
      [fields({ runtime_target_explicit: true }), undefined],
      [fields({ runtime_target_version: "0.40.13" }), undefined],
      [fields({ runtime_target_version: "0.50.0" }), undefined],
      [fields({ runtime_source: "system" }), undefined],
      [fields({ runtime_source: "manual" }), undefined],
      [fields({ runtime_version: "" }), undefined],
      [fields({ runtime_version: "0.41.0-rc1" }), undefined],
      [fields({ runtime_target_version: "garbage" }), undefined],
    ];
    for (const [ollama, running] of cases) {
      const before = ollama.runtime_target_version;
      expect(healStaleRuntimeTarget(ollama as never, running)).toBe(false);
      expect(ollama.runtime_target_version).toBe(before);
    }
  });

  it("syncRuntimeTarget: the legacy default becomes the recommended runtime, a pick of it stays", () => {
    const legacy = fields({ runtime_target_version: "0.20.2" });
    syncRuntimeTarget(legacy);
    expect(legacy.runtime_target_version).toBe(DEFAULT_RUNTIME_VERSION);

    const picked = fields({ runtime_target_version: "0.20.2", runtime_target_explicit: true });
    syncRuntimeTarget(picked);
    expect(picked.runtime_target_version).toBe("0.20.2");
  });

  it("syncRuntimeTarget: an empty target follows the installed runtime, else the recommended one", () => {
    const withInstalled = fields({ runtime_target_version: "  ", runtime_target_explicit: true });
    syncRuntimeTarget(withInstalled);
    expect(withInstalled.runtime_target_version).toBe("0.40.13");
    expect(withInstalled.runtime_target_explicit).toBe(false);

    const nothing = fields({ runtime_target_version: "", runtime_version: "" });
    syncRuntimeTarget(nothing);
    expect(nothing.runtime_target_version).toBe(DEFAULT_RUNTIME_VERSION);
  });

  it("syncRuntimeTarget: a legacy target below the running runtime ends at the recommended one, not at the running one", () => {
    const ollama = fields({ runtime_version: "0.30.4", runtime_target_version: "0.20.2" });
    syncRuntimeTarget(ollama);
    expect(ollama.runtime_target_version).toBe(DEFAULT_RUNTIME_VERSION);
  });
});
