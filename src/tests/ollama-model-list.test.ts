/**
 * The model list component on its own: collapsible zebra rows, details on first expansion only,
 * state that survives re-renders, focus, and untrusted text.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  captureModelListFocus,
  collectModelFacts,
  formatContextLength,
  formatParameterCount,
  getCachedModelDetails,
  renderModelList,
  resetModelListState,
  restoreModelListFocus,
  sanitizeModelDetails,
  type ModelListHooks,
  type ModelListRow,
} from "../ollama-model-list";

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function row(name: string, overrides: Partial<ModelListRow> = {}): ModelListRow {
  return {
    key: name.toLowerCase(),
    name,
    title: `Title ${name}`,
    state: "available",
    sourceLabel: "Ollama",
    sizeLabel: "6.6 GB",
    chips: [{ label: "Recommended", className: "qwen-card-badge qwen-card-badge--curated" }],
    actions: [{ action: "download", label: "Download", className: "qwen-card-btn" }],
    hints: [],
    info: { installed: false, links: [], detailSignature: "-|catalog" },
    ...overrides,
  };
}

const DETAILS = {
  model: "a:1",
  local: {
    license: "apache-2.0",
    capabilities: ["completion", "tools"],
    parameter_size: "7.5B",
    quantization_level: "Q4_K_M",
    format: "gguf",
    context_length: 131072,
    architecture: "gemma4",
    parameter_count: 7_518_069_290,
    base_model: "Gemma 4 E4B (Google)",
    requires: "0.30.9",
    modified_at: "2026-11-02T11:02:26+01:00",
  },
  remote: { source: "ollama", updated_at: "2026-10-12T12:00:00Z", downloads_text: "28.4M", description: "From the library page." },
  remote_note: null,
  remote_cached: false,
  remote_stale: false,
};

let container: HTMLElement;
beforeEach(() => {
  resetModelListState();
  document.body.innerHTML = '<div id="host"></div>';
  container = document.getElementById("host")!;
});
afterEach(() => {
  document.body.innerHTML = "";
});

function hooks(overrides: Partial<ModelListHooks> = {}): ModelListHooks {
  return { onAction: vi.fn(), loadDetails: vi.fn(async () => DETAILS), openLink: vi.fn(), ...overrides };
}
const rowEl = (name: string) => container.querySelector<HTMLElement>(`.ollama-model-row[data-model="${name}"]`)!;
const toggleOf = (name: string) => rowEl(name).querySelector<HTMLButtonElement>(".ollama-model-toggle")!;

describe("collapsed rows", () => {
  it("draw one compact zebra row per entry, in the given order, without details", () => {
    const h = hooks();
    renderModelList(container, [row("a:1"), row("b:1"), row("c:1")], h);
    const rows = Array.from(container.querySelectorAll<HTMLElement>(".ollama-model-row"));
    expect(rows.map((r) => r.dataset.model)).toEqual(["a:1", "b:1", "c:1"]);
    expect(rows.map((r) => (r.classList.contains("is-odd") ? "odd" : "even"))).toEqual(["odd", "even", "odd"]);
    for (const r of rows) {
      expect(r.dataset.expanded).toBe("false");
      expect(r.querySelector(".ollama-model-panel")).toBeNull();
      expect(r.querySelector(".ollama-model-toggle")!.getAttribute("aria-expanded")).toBe("false");
    }
    expect(rows[0].querySelector(".qwen-card-name")!.textContent).toBe("Title a:1");
    expect(rows[0].querySelector(".qwen-card-tag")!.textContent).toBe("a:1");
    expect(rows[0].textContent).toContain("Ollama");
    expect(rows[0].textContent).toContain("6.6 GB");
    expect(rows[0].textContent).toContain("Recommended");
    expect(h.loadDetails).not.toHaveBeenCalled();
  });

  it("use a real button as toggle (Enter and Space work natively) and name the list for assistive tech", () => {
    renderModelList(container, [row("a:1")], hooks());
    const toggle = toggleOf("a:1");
    expect(toggle.tagName).toBe("BUTTON");
    expect(toggle.type).toBe("button");
    expect(container.querySelector(".ollama-model-rows")!.getAttribute("role")).toBe("list");
  });
});

describe("expanding", () => {
  it("opens on a click on the row head, asks for the details once and closes again", async () => {
    const h = hooks();
    renderModelList(container, [row("a:1"), row("b:1")], h);

    // The chip area counts as "the row", not only the name.
    rowEl("a:1").querySelector<HTMLElement>(".ollama-model-chips")!.click();
    expect(rowEl("a:1").dataset.expanded).toBe("true");
    expect(toggleOf("a:1").getAttribute("aria-expanded")).toBe("true");
    const panel = rowEl("a:1").querySelector(".ollama-model-panel")!;
    expect(toggleOf("a:1").getAttribute("aria-controls")).toBe(panel.id);
    expect(h.loadDetails).toHaveBeenCalledTimes(1);
    expect(h.loadDetails).toHaveBeenCalledWith("a:1");
    expect(panel.textContent).toContain("Loading details");
    expect(rowEl("b:1").dataset.expanded).toBe("false");

    await flush();
    expect(rowEl("a:1").textContent).toContain("131,072 tokens (128K)");

    toggleOf("a:1").click();
    expect(rowEl("a:1").dataset.expanded).toBe("false");
    expect(rowEl("a:1").querySelector(".ollama-model-panel")).toBeNull();
    toggleOf("a:1").click();
    await flush();
    expect(h.loadDetails).toHaveBeenCalledTimes(1);
    expect(rowEl("a:1").textContent).toContain("131,072 tokens (128K)");
  });

  it("does not toggle on a click in the opened panel (text selection) and does not toggle on a button", async () => {
    const h = hooks();
    renderModelList(container, [row("a:1")], h);
    toggleOf("a:1").click();
    await flush();
    rowEl("a:1").querySelector<HTMLElement>(".ollama-model-panel")!.click();
    expect(rowEl("a:1").dataset.expanded).toBe("true");

    rowEl("a:1").querySelector<HTMLButtonElement>('button[data-action="download"]')!.click();
    expect(rowEl("a:1").dataset.expanded).toBe("true");
    expect(h.onAction).toHaveBeenCalledTimes(1);
    const [action, clicked, data] = vi.mocked(h.onAction).mock.calls[0];
    expect(action).toBe("download");
    expect(clicked.name).toBe("a:1");
    expect(data.model).toBe("a:1");
  });

  it("ignores disabled actions and passes extra data attributes", () => {
    const h = hooks();
    renderModelList(
      container,
      [
        row("a:1", {
          actions: [
            { action: "replace", label: "Replace", className: "qwen-card-btn", data: { target: "b:1" } },
            { action: "delete", label: "Delete", className: "qwen-card-btn", disabled: true },
          ],
        }),
      ],
      h
    );
    rowEl("a:1").querySelector<HTMLButtonElement>('button[data-action="delete"]')!.click();
    expect(h.onAction).not.toHaveBeenCalled();
    rowEl("a:1").querySelector<HTMLButtonElement>('button[data-action="replace"]')!.click();
    expect(vi.mocked(h.onAction).mock.calls[0][2].target).toBe("b:1");
  });

  it("without a loader the panel shows what the row knows and never says 'loading'", () => {
    renderModelList(
      container,
      [row("a:1", { info: { installed: true, sizeText: "6.6 GB", digest: "sha256:c6eb396dbd5992bb", detailSignature: "x", links: [] } })],
      { onAction: vi.fn() }
    );
    toggleOf("a:1").click();
    const text = rowEl("a:1").textContent!;
    expect(text).toContain("Size on disk");
    expect(text).toContain("c6eb396dbd59");
    expect(text).not.toContain("Loading");
  });
});

describe("state across renders", () => {
  it("keeps rows open and the loaded details when the list is drawn again", async () => {
    const h = hooks();
    renderModelList(container, [row("a:1"), row("b:1")], h);
    toggleOf("b:1").click();
    await flush();

    container.innerHTML = "";
    renderModelList(container, [row("a:1"), row("b:1", { sizeLabel: "7.0 GB" })], h);
    expect(rowEl("b:1").dataset.expanded).toBe("true");
    expect(rowEl("b:1").textContent).toContain("7.0 GB");
    expect(rowEl("b:1").textContent).toContain("131,072 tokens (128K)");
    expect(h.loadDetails).toHaveBeenCalledTimes(1);
    expect(rowEl("a:1").dataset.expanded).toBe("false");
  });

  it("forgets a row that is gone, so it does not come back open", async () => {
    const h = hooks();
    renderModelList(container, [row("a:1")], h);
    toggleOf("a:1").click();
    container.innerHTML = "";
    renderModelList(container, [], h);
    container.innerHTML = "";
    renderModelList(container, [row("a:1")], h);
    expect(rowEl("a:1").dataset.expanded).toBe("false");
  });

  it("asks again when the model changed (new digest)", async () => {
    const h = hooks();
    const info = (digest: string) => ({ installed: true, detailSignature: `${digest}|installed`, links: [] });
    renderModelList(container, [row("a:1", { info: info("old") })], h);
    toggleOf("a:1").click();
    await flush();
    container.innerHTML = "";
    renderModelList(container, [row("a:1", { info: info("new") })], h);
    await flush();
    expect(h.loadDetails).toHaveBeenCalledTimes(2);
  });

  it("keeps keyboard focus on the toggle and on an action button through a re-render", async () => {
    const h = hooks();
    renderModelList(container, [row("a:1"), row("b:1")], h);
    toggleOf("b:1").focus();
    toggleOf("b:1").click(); // rebuilds the list in place
    expect(document.activeElement).toBe(toggleOf("b:1"));

    const download = rowEl("a:1").querySelector<HTMLButtonElement>('button[data-action="download"]')!;
    download.focus();
    const token = captureModelListFocus(container);
    expect(token).toEqual({ key: "a:1", target: "action:download" });
    container.innerHTML = "";
    renderModelList(container, [row("a:1"), row("b:1")], h);
    expect(document.activeElement).not.toBe(rowEl("a:1").querySelector('button[data-action="download"]'));
    restoreModelListFocus(container, token);
    expect(document.activeElement).toBe(rowEl("a:1").querySelector('button[data-action="download"]'));
  });

  it("falls back to the toggle when the focused action disappeared", () => {
    const h = hooks();
    renderModelList(container, [row("a:1")], h);
    rowEl("a:1").querySelector<HTMLButtonElement>('button[data-action="download"]')!.focus();
    const token = captureModelListFocus(container);
    container.innerHTML = "";
    renderModelList(container, [row("a:1", { actions: [] })], h);
    restoreModelListFocus(container, token);
    expect(document.activeElement).toBe(toggleOf("a:1"));
  });

  it("captures nothing when focus is outside the list", () => {
    renderModelList(container, [row("a:1")], hooks());
    expect(captureModelListFocus(container)).toBeNull();
    restoreModelListFocus(container, null);
  });
});

describe("details", () => {
  const installedInfo = (extra: Record<string, unknown> = {}) => ({
    installed: true,
    sizeText: "6.6 GB",
    digest: "dc35e8d9c6061baa6f0fa870975ab6932e2542b579b13ea0f199fa4bb7300c9c",
    downloadedAt: "2026-07-23T10:12:00+02:00",
    detailSignature: "x|installed",
    links: [],
    ...extra,
  });

  it("show released (with its source), downloaded, digest, parameters, quantization, context, capabilities, license, runtime", async () => {
    const h = hooks();
    renderModelList(
      container,
      [
        row("a:1", {
          info: installedInfo({
            released: "2026-03-02",
            releasedFrom: "google/gemma-4-E4B-it",
            description: "Catalog text.",
            requiresText: "This model needs Ollama 0.30.9 or newer. Running: 0.40.13.",
            verifiedText: "Passed a functional check.",
          }),
        }),
      ],
      h
    );
    toggleOf("a:1").click();
    await flush();
    const text = rowEl("a:1").textContent!;
    for (const expected of [
      "Released2 Mar 2026",
      "Downloaded23 Jul 2026",
      "Size on disk6.6 GB",
      "Digestdc35e8d9c606",
      "Parameters7.5B (7,518,069,290)",
      "QuantizationQ4_K_M",
      "Context length131,072 tokens (128K)",
      "Architecturegemma4 (gguf)",
      "completion",
      "tools",
      "Licenseapache-2.0",
      "Base modelGemma 4 E4B (Google)",
      "RuntimeThis model needs Ollama 0.30.9 or newer",
      "VerificationPassed a functional check.",
      "Updated on ollama.com12 Oct 2026",
      "Ollama library28.4M downloads",
      "Catalog text.",
      "From the library page.",
    ]) {
      expect(text, expected).toContain(expected);
    }
    const released = Array.from(rowEl("a:1").querySelectorAll(".ollama-model-fact-value")).find((e) => e.textContent === "2 Mar 2026")!;
    expect((released as HTMLElement).title).toContain("huggingface.co/google/gemma-4-E4B-it");
  });

  it("leave the Released line out when nothing proves a date (no invented data)", async () => {
    renderModelList(container, [row("a:1", { info: installedInfo() })], hooks());
    toggleOf("a:1").click();
    await flush();
    expect(rowEl("a:1").textContent).not.toContain("Released");
  });

  it("use the Hugging Face repository creation date as Released for a Hugging Face model", async () => {
    const h = hooks({
      loadDetails: vi.fn(async () => ({
        ...DETAILS,
        remote: { source: "huggingface", released_at: "2026-04-01T14:42:04.000Z", updated_at: "2026-07-17T12:49:21.000Z", downloads: 581312, likes: 675, languages: ["en", "de"], pipeline_tag: "image-text-to-text" },
      })),
    });
    renderModelList(container, [row("hf.co/a/b:Q4", { info: installedInfo() })], h);
    toggleOf("hf.co/a/b:Q4").click();
    await flush();
    const text = rowEl("hf.co/a/b:Q4").textContent!;
    expect(text).toContain("Released1 Apr 2026");
    expect(text).toContain("Updated on Hugging Face17 Jul 2026");
    expect(text).toContain("581,312 downloads · 675 likes");
    expect(text).toContain("Languagesen, de");
    expect(text).toContain("Taskimage-text-to-text");
  });

  it("show the known facts plus a quiet note when the lookup fails, and try again later", async () => {
    let now = 1_000_000;
    const loadDetails = vi.fn().mockRejectedValueOnce(new Error("timed out")).mockResolvedValue(DETAILS);
    const h = hooks({ loadDetails, now: () => now });
    renderModelList(container, [row("a:1", { info: installedInfo() })], h);
    toggleOf("a:1").click();
    await flush();
    const text = rowEl("a:1").textContent!;
    expect(text).toContain("More details could not be loaded (timed out)");
    expect(text).toContain("Size on disk6.6 GB");
    expect(rowEl("a:1").querySelector(".ollama-model-note")!.getAttribute("role")).toBe("status");

    // A re-render right away does not hammer the backend...
    container.innerHTML = "";
    renderModelList(container, [row("a:1", { info: installedInfo() })], h);
    expect(loadDetails).toHaveBeenCalledTimes(1);
    // ...a later one does.
    now += 31_000;
    container.innerHTML = "";
    renderModelList(container, [row("a:1", { info: installedInfo() })], h);
    await flush();
    expect(loadDetails).toHaveBeenCalledTimes(2);
    expect(rowEl("a:1").textContent).toContain("131,072 tokens (128K)");
  });

  it("never throws when the loader throws synchronously", async () => {
    const h = hooks({
      loadDetails: () => {
        throw new Error("boom");
      },
    });
    renderModelList(container, [row("a:1")], h);
    toggleOf("a:1").click();
    await flush();
    expect(rowEl("a:1").textContent).toContain("More details could not be loaded (boom)");
  });

  it("report offline and strict-local notes from the backend without an error look", async () => {
    const h = hooks({
      loadDetails: vi.fn(async () => ({
        model: "a:1",
        local: DETAILS.local,
        remote: null,
        remote_note: "Strict local mode is on: only data from this computer is shown.",
      })),
    });
    renderModelList(container, [row("a:1", { info: installedInfo() })], h);
    toggleOf("a:1").click();
    await flush();
    const text = rowEl("a:1").textContent!;
    expect(text).toContain("Strict local mode is on");
    expect(text).toContain("131,072 tokens (128K)");
    expect(text).not.toContain("could not be loaded");
  });

  it("show an install-time error of the local part only for installed rows", async () => {
    const h = hooks({
      loadDetails: vi.fn(async () => ({ model: "a:1", local: null, local_error: "'a:1' is not installed.", remote: DETAILS.remote })),
    });
    renderModelList(container, [row("a:1")], h);
    toggleOf("a:1").click();
    await flush();
    expect(rowEl("a:1").textContent).not.toContain("not installed");
    expect(rowEl("a:1").textContent).toContain("28.4M downloads");

    resetModelListState();
    container.innerHTML = "";
    renderModelList(container, [row("a:1", { info: installedInfo() })], h);
    toggleOf("a:1").click();
    await flush();
    expect(rowEl("a:1").textContent).toContain("Local details unavailable: 'a:1' is not installed.");
  });

  it("render hostile texts as text only", async () => {
    const IMG = '<img src=x onerror="window.__pwn=1">';
    const h = hooks({
      loadDetails: vi.fn(async () => ({
        model: "a:1",
        local: { license: IMG, capabilities: [IMG], base_model: IMG, architecture: IMG, family: IMG },
        remote: { source: "huggingface", description: IMG, pipeline_tag: IMG, languages: [IMG], license: IMG, base_model: IMG, released_at: IMG, updated_at: IMG },
        remote_note: IMG,
      })),
    });
    renderModelList(
      container,
      [row("a:1", { title: IMG, info: { installed: true, description: IMG, catalogNote: IMG, license: IMG, detailSignature: "x", links: [{ label: IMG, url: "https://ollama.com/library/a" }] } })],
      h
    );
    toggleOf("a:1").click();
    await flush();
    expect(container.querySelector("img, script, iframe, svg")).toBeNull();
    expect(container.textContent).toContain(IMG);
    expect(Array.from(container.querySelectorAll("*")).flatMap((e) => e.getAttributeNames().filter((n) => n.startsWith("on")))).toEqual([]);
  });

  it("offer links only for https URLs and hand a click to the opener", async () => {
    const h = hooks();
    renderModelList(
      container,
      [
        row("a:1", {
          info: {
            installed: false,
            detailSignature: "x",
            links: [
              { label: "Open on Ollama", url: "https://ollama.com/library/a" },
              { label: "Evil", url: "javascript:alert(1)" },
              { label: "Plain", url: "http://ollama.com/library/a" },
            ],
          },
        }),
      ],
      h
    );
    toggleOf("a:1").click();
    await flush();
    const links = Array.from(rowEl("a:1").querySelectorAll<HTMLButtonElement>("[data-open-url]"));
    expect(links.map((l) => l.textContent)).toEqual(["Open on Ollama"]);
    links[0].click();
    expect(h.openLink).toHaveBeenCalledWith("https://ollama.com/library/a");
    expect(rowEl("a:1").dataset.expanded).toBe("true");
  });

  it("never loads details for rows that were never opened (ten rows, nothing requested)", () => {
    const h = hooks();
    renderModelList(container, Array.from({ length: 10 }, (_, i) => row(`m:${i}`)), h);
    expect(h.loadDetails).not.toHaveBeenCalled();
    expect(getCachedModelDetails("m:1")).toBeUndefined();
  });

  it("asks once per model even when toggled quickly and re-rendered while the answer is pending", async () => {
    let resolve!: (value: unknown) => void;
    const loadDetails = vi.fn(() => new Promise((r) => (resolve = r)));
    const h = hooks({ loadDetails });
    renderModelList(container, [row("a:1")], h);
    toggleOf("a:1").click();
    toggleOf("a:1").click();
    toggleOf("a:1").click();
    container.innerHTML = "";
    renderModelList(container, [row("a:1")], h);
    expect(loadDetails).toHaveBeenCalledTimes(1);
    resolve(DETAILS);
    await flush();
    expect(rowEl("a:1").textContent).toContain("131,072 tokens (128K)");
  });

  it("a late answer for a list that was replaced does not touch the new list", async () => {
    let resolve!: (value: unknown) => void;
    const h = hooks({ loadDetails: vi.fn(() => new Promise((r) => (resolve = r))) });
    renderModelList(container, [row("a:1")], h);
    toggleOf("a:1").click();
    const old = container.firstElementChild!;
    container.innerHTML = "";
    const other = document.createElement("div");
    document.body.appendChild(other);
    renderModelList(other, [row("z:1")], h);
    resolve(DETAILS);
    await flush();
    expect(old.isConnected).toBe(false);
    expect(other.querySelectorAll(".ollama-model-row")).toHaveLength(1);
  });
});

describe("progress", () => {
  it("draws a plain width and the status text", () => {
    renderModelList(container, [row("a:1", { progress: { percent: 45.6, text: "45% (1.5 GB / 3.3 GB)" } })], hooks());
    expect(rowEl("a:1").querySelector(".qwen-card-progress-fill")!.getAttribute("style")).toBe("width: 45.6%");
    expect(rowEl("a:1").querySelector(".qwen-card-progress-text")!.textContent).toBe("45% (1.5 GB / 3.3 GB)");
    container.innerHTML = "";
    renderModelList(container, [row("a:1", { progress: { percent: null, text: "Starting..." } })], hooks());
    expect(rowEl("a:1").querySelector(".qwen-card-progress-fill")!.getAttribute("style")).toBe("width: 0%");
    container.innerHTML = "";
    renderModelList(container, [row("a:1", { progress: { percent: 400, text: "x" } })], hooks());
    expect(rowEl("a:1").querySelector(".qwen-card-progress-fill")!.getAttribute("style")).toBe("width: 100%");
  });
});

describe("sanitizeModelDetails", () => {
  it("accepts the backend shape", () => {
    const view = sanitizeModelDetails(DETAILS);
    expect(view.local?.contextLength).toBe(131072);
    expect(view.local?.capabilities).toEqual(["completion", "tools"]);
    expect(view.remote?.source).toBe("ollama");
    expect(view.remote?.downloadsText).toBe("28.4M");
  });

  it("turns anything else into missing values", () => {
    for (const raw of [null, undefined, 5, "x", [], {}, { local: 5, remote: "x" }, { local: { context_length: "big", capabilities: "x", parameter_count: -1 } }]) {
      const view = sanitizeModelDetails(raw);
      expect(view.remote).toBeNull();
      expect(view.local?.contextLength).toBeUndefined();
      expect(view.local?.capabilities ?? []).toEqual([]);
    }
    expect(sanitizeModelDetails({ remote: { source: "evil" } }).remote?.source).toBe("ollama");
  });

  it("caps lists and text length", () => {
    const view = sanitizeModelDetails({
      local: { capabilities: Array.from({ length: 50 }, (_, i) => `c${i}`), license: "L".repeat(1000) },
      remote: { description: "D".repeat(5000), languages: Array.from({ length: 50 }, (_, i) => `l${i}`) },
    });
    expect(view.local?.capabilities).toHaveLength(12);
    expect(view.local?.license).toHaveLength(80);
    expect(view.remote?.description).toHaveLength(500);
    expect(view.remote?.languages).toHaveLength(8);
  });
});

describe("formatting and facts", () => {
  it("formats context lengths and parameter counts", () => {
    expect(formatContextLength(131072)).toBe("131,072 tokens (128K)");
    expect(formatContextLength(8192)).toBe("8,192 tokens (8K)");
    expect(formatContextLength(1000)).toBe("1,000 tokens");
    expect(formatParameterCount(7_518_069_290)).toBe("7.5B");
    expect(formatParameterCount(135_000_000)).toBe("135M");
    expect(formatParameterCount(900)).toBe("900");
  });

  it("collects no facts for a row that knows nothing", () => {
    expect(collectModelFacts(row("a:1"), undefined)).toEqual([]);
  });

  it("shows a gated Hugging Face repository as such", () => {
    const entry = { signature: "x", state: "ready" as const, at: 1, view: sanitizeModelDetails({ remote: { source: "huggingface", gated: true } }) };
    expect(collectModelFacts(row("a:1"), entry).map((f) => f.label)).toContain("Access");
  });
});
