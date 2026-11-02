import { describe, expect, it, vi } from "vitest";
import type { OllamaModelRequirement, OllamaRuntimeVersionInfo } from "../types";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

vi.mock("../toast", () => ({
  showToast: vi.fn(),
}));

import {
  buildInstalledOllamaInventory,
  buildModelRuntimeGate,
  describePullProgress,
  functionalProofChip,
  parseRuntimeTooOldError,
  type OllamaModelUpdateInfo,
} from "../ollama-models";
import { parseFunctionalProof, parseModelCatalog } from "../ollama-model-catalog";
import { compareVersions, parseVersionTriple, pickRuntimeVersionForRequirement } from "../ollama-runtime-versions";

function runtime(version: string, overrides: Partial<OllamaRuntimeVersionInfo> = {}): OllamaRuntimeVersionInfo {
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

const CATALOG = [
  runtime("0.40.13", { latest: true }),
  runtime("0.40.7", { source: "pinned", recommended: true, installed: true }),
  runtime("0.35.2"),
  runtime("0.30.4", { installable: false, installed: true }),
  runtime("0.41.0-rc1", { prerelease: true }),
  runtime("0.29.0", { installable: false }), // unverified: neither installable nor on disk
];

const requirement = (overrides: Partial<OllamaModelRequirement> = {}): OllamaModelRequirement => ({
  name: "lfm3:1.7b",
  requires: "0.40.13",
  runtime_version: "0.40.7",
  compatible: false,
  ...overrides,
});

describe("version helpers", () => {
  it("parses strict x.y.z (with optional v and pre-release suffix) and rejects the rest", () => {
    expect(parseVersionTriple("0.40.13")).toEqual([0, 40, 13]);
    expect(parseVersionTriple("v0.40.7")).toEqual([0, 40, 7]);
    expect(parseVersionTriple("0.40.7-rc1")).toEqual([0, 40, 7]);
    expect(parseVersionTriple("0.40")).toBeNull();
    expect(parseVersionTriple("unknown")).toBeNull();
    expect(parseVersionTriple(null)).toBeNull();
  });

  it("compares numerically, not as text (0.40.13 > 0.40.7)", () => {
    expect(compareVersions("0.40.13", "0.40.7")).toBe(1);
    expect(compareVersions("0.40.7", "0.40.13")).toBe(-1);
    expect(compareVersions("0.40.7", "v0.40.7")).toBe(0);
    expect(compareVersions("1.0.0", "0.99.99")).toBe(1);
    expect(compareVersions("0.40.7", "unknown")).toBeNull();
  });
});

describe("pickRuntimeVersionForRequirement", () => {
  it("takes the only verified version that is new enough", () => {
    expect(pickRuntimeVersionForRequirement(CATALOG, "0.40.13")?.version).toBe("0.40.13");
    expect(pickRuntimeVersionForRequirement(CATALOG, "0.40.9")?.version).toBe("0.40.13");
  });

  it("prefers the pinned/recommended version when it is new enough, even if newer ones exist", () => {
    expect(pickRuntimeVersionForRequirement(CATALOG, "0.30.9")?.version).toBe("0.40.7");
    expect(pickRuntimeVersionForRequirement(CATALOG, "0.40.6")?.version).toBe("0.40.7");
  });

  it("falls back to the newest candidate when nothing recommended qualifies", () => {
    const catalog = [runtime("0.50.1"), runtime("0.50.3"), runtime("0.40.7", { recommended: true })];
    expect(pickRuntimeVersionForRequirement(catalog, "0.50.0")?.version).toBe("0.50.3");
  });

  it("ignores prereleases and unverified versions", () => {
    expect(pickRuntimeVersionForRequirement(CATALOG, "0.41.0")).toBeNull();
    expect(pickRuntimeVersionForRequirement([runtime("0.29.0", { installable: false })], "0.28.0")).toBeNull();
  });

  it("returns null when the catalog has nothing that new", () => {
    expect(pickRuntimeVersionForRequirement(CATALOG, "9.0.0")).toBeNull();
    expect(pickRuntimeVersionForRequirement([], "0.40.13")).toBeNull();
  });

  it("accepts versions that are only on disk", () => {
    const catalog = [runtime("0.40.9", { installable: false, installed: true })];
    expect(pickRuntimeVersionForRequirement(catalog, "0.40.8")?.version).toBe("0.40.9");
  });
});

describe("buildModelRuntimeGate", () => {
  it("blocks and offers the update when the runtime is too old", () => {
    const gate = buildModelRuntimeGate(requirement(), "0.40.7", CATALOG)!;
    expect(gate.blocked).toBe(true);
    expect(gate.chipLabel).toBe("Needs Ollama ≥ 0.40.13");
    expect(gate.chipTitle).toContain("Running: 0.40.7");
    expect(gate.target?.version).toBe("0.40.13");
    expect(gate.targetLabel).toBe("Update runtime to 0.40.13");
    expect(gate.hint).toBeNull();
  });

  it("says 'Switch runtime to' when the fixing version is already on disk", () => {
    const catalog = [runtime("0.40.13", { installed: true }), runtime("0.40.7")];
    expect(buildModelRuntimeGate(requirement(), "0.40.7", catalog)?.targetLabel).toBe("Switch runtime to 0.40.13");
  });

  it("shows the chip but does not block when the runtime is new enough", () => {
    const gate = buildModelRuntimeGate(requirement({ requires: "0.30.9", compatible: true }), "0.40.7", CATALOG)!;
    expect(gate.blocked).toBe(false);
    expect(gate.chipLabel).toBe("Needs Ollama ≥ 0.30.9");
    expect(gate.target).toBeNull();
  });

  it("recomputes from the running version, so the cached backend answer cannot go stale", () => {
    // Backend said "incompatible" for 0.40.7; the runtime has been updated since.
    expect(buildModelRuntimeGate(requirement({ compatible: false }), "0.40.13", CATALOG)?.blocked).toBe(false);
    // And the other way round.
    expect(buildModelRuntimeGate(requirement({ compatible: true }), "0.40.6", CATALOG)?.blocked).toBe(true);
  });

  it("uses the runtime version the backend reported when none is known locally, else its verdict", () => {
    const reported = buildModelRuntimeGate(requirement({ requires: "0.40.13", compatible: true }), null, CATALOG);
    expect(reported?.running).toBe("0.40.7");
    expect(reported?.blocked).toBe(true); // 0.40.7 < 0.40.13 beats the row's own `compatible`
    const unknown = buildModelRuntimeGate(
      requirement({ requires: "nightly", runtime_version: null, compatible: false }),
      null,
      CATALOG
    );
    expect(unknown?.blocked).toBe(true);
  });

  it("gives a hint and no button when the catalog has no suitable runtime", () => {
    const gate = buildModelRuntimeGate(requirement({ requires: "0.99.0" }), "0.40.7", CATALOG)!;
    expect(gate.blocked).toBe(true);
    expect(gate.target).toBeNull();
    expect(gate.targetLabel).toBeNull();
    expect(gate.hint).toContain("0.99.0");
    expect(gate.hint).toContain("Get versions");
  });

  it("has no gate for models without requirement info (Hugging Face, failed check)", () => {
    expect(buildModelRuntimeGate(null, "0.40.7", CATALOG)).toBeNull();
    expect(buildModelRuntimeGate(requirement({ requires: null, compatible: null }), "0.40.7", CATALOG)).toBeNull();
  });

  it("a refused pull without a version still blocks and targets something newer than the running runtime", () => {
    const gate = buildModelRuntimeGate(null, "0.40.7", CATALOG, { requires: null, runtime: "0.40.7" })!;
    expect(gate.blocked).toBe(true);
    expect(gate.chipLabel).toBe("Needs a newer Ollama");
    expect(gate.target?.version).toBe("0.40.13");
  });

  it("a refusal never offers the runtime that is already running", () => {
    const catalog = [runtime("0.40.7", { recommended: true, installed: true })];
    const gate = buildModelRuntimeGate(null, "0.40.7", catalog, { requires: null, runtime: "0.40.7" })!;
    expect(gate.blocked).toBe(true);
    expect(gate.target).toBeNull();
    expect(gate.hint).toBeTruthy();
  });
});

describe("parseRuntimeTooOldError", () => {
  it("(a) older runtime: 412 without a version number -> requires null (the registry check has to say)", () => {
    expect(
      parseRuntimeTooOldError(
        "pull model manifest: 412: The model you are attempting to pull requires a newer version of Ollama. Please download the latest version at: https://ollama.com/download"
      )
    ).toEqual({ requires: null });
    expect(parseRuntimeTooOldError("Connection failed: http://127.0.0.1:11434/api/pull: status code 412")).toEqual({ requires: null });
    expect(parseRuntimeTooOldError("http status: 412")).toEqual({ requires: null });
  });

  it("(b) newer runtime: reads the version from the real 0.40.7 text and drops the leading v", () => {
    expect(parseRuntimeTooOldError("model requires ollama version v0.40.13 or newer (current version is v0.40.7)")).toEqual({
      requires: "0.40.13",
    });
    // Same text with the manifest prefix, and without the v.
    expect(
      parseRuntimeTooOldError("pull model manifest: 412: model requires ollama version 0.40.13 or newer (current version is 0.40.7)")
    ).toEqual({ requires: "0.40.13" });
  });

  it("never mistakes the running version for the required one", () => {
    expect(parseRuntimeTooOldError("model requires ollama version v0.40.13 or newer (current version is v0.40.7)")?.requires).not.toBe(
      "0.40.7"
    );
  });

  it("(c) the app's own refusal before the pull names the version too", () => {
    expect(
      parseRuntimeTooOldError("Model 'lfm3:1.7b' needs Ollama >= 0.40.13 (running 0.40.7). Update the runtime first.")
    ).toEqual({ requires: "0.40.13" });
  });

  it("returns null for every other error", () => {
    for (const message of [
      "Connection failed: connection refused",
      "file does not exist (error 4120)",
      "pull model manifest: file does not exist",
      "Pull already in progress for 'x'",
      "pulling 4120abc: stream reset by peer",
      "",
      null,
      undefined,
    ]) {
      expect(parseRuntimeTooOldError(message)).toBeNull();
    }
  });
});

describe("describePullProgress", () => {
  it("describes the whole model, not the newest layer", () => {
    expect(
      describePullProgress({ percent: 42, completed: 2_800_000_000, total: 6_600_000_000, status: "pulling 8b1d2e3f4a5b", finalizing: false }, undefined)
    ).toEqual({ percent: 42, text: "42% (2.8 GB / 6.6 GB) - pulling 8b1d2e3f4a5b" });
    expect(
      describePullProgress({ percent: 7, completed: 40_000_000, total: 570_000_000, status: "pulling x", finalizing: false }, undefined).text
    ).toBe("7% (40 MB / 570 MB) - pulling x");
  });

  it("shows only the status while no size is known, and falls back to a single line without a tracker", () => {
    expect(describePullProgress({ percent: null, completed: 0, total: 0, status: "pulling manifest", finalizing: false }, undefined)).toEqual({
      percent: null,
      text: "pulling manifest",
    });
    expect(describePullProgress(null, { model: "m", status: "pulling manifest" })).toEqual({ percent: null, text: "pulling manifest" });
    expect(describePullProgress(null, undefined)).toEqual({ percent: null, text: "Starting..." });
  });
});

describe("functional proof (catalog field 'functional')", () => {
  const proof = { verified_at: "2026-11-02", runtime: "0.40.13", latency_p50_ms: 840, note: "German dictation, 12 sentences." };

  it("accepts a complete proof and normalises it", () => {
    expect(parseFunctionalProof({ ...proof, runtime: "v0.40.13", latency_p50_ms: 840.4 })).toEqual({
      verified_at: "2026-11-02",
      runtime: "0.40.13",
      latency_p50_ms: 840,
      note: "German dictation, 12 sentences.",
    });
    expect(parseFunctionalProof({ verified_at: "2026-11-02", runtime: "0.40.13", latency_p50_ms: 1200 })).toEqual({
      verified_at: "2026-11-02",
      runtime: "0.40.13",
      latency_p50_ms: 1200,
    });
  });

  it.each([
    ["not an object", "yes"],
    ["null", null],
    ["missing date", { runtime: "0.40.13", latency_p50_ms: 1 }],
    ["bad date", { ...proof, verified_at: "02.11.2026" }],
    ["missing runtime", { ...proof, runtime: undefined }],
    ["runtime not a version", { ...proof, runtime: "latest" }],
    ["missing latency", { ...proof, latency_p50_ms: undefined }],
    ["zero latency", { ...proof, latency_p50_ms: 0 }],
    ["text latency", { ...proof, latency_p50_ms: "840" }],
    ["NaN latency", { ...proof, latency_p50_ms: Number.NaN }],
  ])("drops a malformed proof (%s)", (_label, raw) => {
    expect(parseFunctionalProof(raw)).toBeUndefined();
  });

  it("keeps the catalog entry itself when only its proof is malformed", () => {
    const base = {
      name: "gemma4:e4b",
      label: "Gemma",
      source: "ollama",
      family: "gemma4",
      status: "recommended",
      profile: "x",
      size_gb: 6.58,
      description: "d",
    };
    const [good, bad, none] = parseModelCatalog([
      { ...base, name: "a:1", functional: proof },
      { ...base, name: "b:1", functional: { ...proof, latency_p50_ms: -5 } },
      { ...base, name: "c:1" },
    ]);
    expect(good.functional?.latency_p50_ms).toBe(840);
    expect(bad.name).toBe("b:1");
    expect(bad.functional).toBeUndefined();
    expect(none.functional).toBeUndefined();
  });

  it("builds the 'Verified working' chip with the proof in the tooltip", () => {
    const chip = functionalProofChip(parseFunctionalProof(proof));
    expect(chip?.label).toBe("Verified working");
    expect(chip?.title).toBe(
      "Passed a functional check on Ollama 0.40.13 (2 Nov 2026): typical response 840 ms. German dictation, 12 sentences."
    );
    expect(functionalProofChip({ ...proof, latency_p50_ms: 2500, note: undefined })?.title).toContain("typical response 2.5 s.");
    expect(functionalProofChip(undefined)).toBeNull();
  });
});

describe("duplicate builds (Replace with ...)", () => {
  const SHARED = "dc35e8d9c6061baa6f0fa870975ab6932e2542b579b13ea0f199fa4bb7300c9c";
  const OTHER = "ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12";
  const installed = [
    { name: "gemma4:e4b-it-q4_K_M", size_bytes: 9_608_350_718, digest: "c6eb396dbd59".padEnd(64, "0") },
    { name: "gemma4:e4b", size_bytes: 6_583_658_527, digest: SHARED },
    { name: "qwen3.5:4b", size_bytes: 3_400_000_000, digest: "f".repeat(64) },
  ];
  const update = (name: string, status: OllamaModelUpdateInfo["status"], remote: string): [string, OllamaModelUpdateInfo] => [
    name.toLowerCase(),
    { name, status, remote_digest: remote, remote_size_bytes: 6_583_658_527 },
  ];

  it("offers the current model as replacement for an outdated one with the same registry build", () => {
    const items = buildInstalledOllamaInventory(
      installed,
      "gemma4:e4b-it-q4_K_M",
      new Map([
        update("gemma4:e4b-it-q4_K_M", "outdated", SHARED),
        update("gemma4:e4b", "current", SHARED),
        update("qwen3.5:4b", "current", OTHER),
      ])
    );
    const old = items.find((item) => item.name === "gemma4:e4b-it-q4_K_M")!;
    expect(old.replace_with).toBe("gemma4:e4b");
    expect(old.can_update).toBe(false);
    expect(items.find((item) => item.name === "gemma4:e4b")!.replace_with).toBeNull();
    expect(items.find((item) => item.name === "qwen3.5:4b")!.can_update).toBe(false);
  });

  it("offers a plain update when no installed model carries the current build", () => {
    const items = buildInstalledOllamaInventory(
      installed.slice(0, 1),
      "",
      new Map([update("gemma4:e4b-it-q4_K_M", "outdated", SHARED)])
    );
    expect(items[0].replace_with).toBeNull();
    expect(items[0].can_update).toBe(true);
  });

  it("does not match different builds, short digests or non-current candidates", () => {
    const items = buildInstalledOllamaInventory(
      installed,
      "",
      new Map([
        update("gemma4:e4b-it-q4_K_M", "outdated", SHARED),
        update("gemma4:e4b", "outdated", SHARED), // also outdated: not a valid replacement
        update("qwen3.5:4b", "current", "abc"), // too short to compare
      ])
    );
    expect(items.find((item) => item.name === "gemma4:e4b-it-q4_K_M")!.replace_with).toBeNull();
    expect(items.find((item) => item.name === "gemma4:e4b-it-q4_K_M")!.can_update).toBe(true);
  });

  it("compares the registry build case-insensitively and ignores a sha256 prefix", () => {
    const items = buildInstalledOllamaInventory(
      installed,
      "",
      new Map([
        update("gemma4:e4b-it-q4_K_M", "outdated", `sha256:${SHARED.toUpperCase()}`),
        update("gemma4:e4b", "current", SHARED),
      ])
    );
    expect(items.find((item) => item.name === "gemma4:e4b-it-q4_K_M")!.replace_with).toBe("gemma4:e4b");
  });
});
