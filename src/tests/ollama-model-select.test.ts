/**
 * The tick boxes and the "Download selected" bar of the model list, component only: the selection
 * lives in the list module (not in the DOM, which is rebuilt on every render), the bar shows count
 * and size, and a click on a box never opens or closes its row.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  isModelRowSelected,
  renderModelList,
  resetModelListState,
  type ModelListHooks,
  type ModelListRow,
  type ModelListSelect,
} from "../ollama-model-list";

function row(name: string, select: ModelListSelect | null = { sizeGb: 6.58, blocked: false }, overrides: Partial<ModelListRow> = {}): ModelListRow {
  return {
    key: name.toLowerCase(),
    name,
    title: `Title ${name}`,
    state: "available",
    sourceLabel: "Ollama",
    sizeLabel: "6.6 GB",
    chips: [],
    actions: [],
    hints: [],
    info: { installed: false, links: [], detailSignature: "-|catalog" },
    ...(select ? { select } : {}),
    ...overrides,
  };
}

const installedRow = (name: string) =>
  row(name, null, { state: "installed", info: { installed: true, links: [], detailSignature: "d|installed" } });

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
  return { onAction: vi.fn(), onDownloadSelected: vi.fn(), ...overrides };
}

/** The app clears the container before each render; the list module keeps the selection. */
function draw(rows: ModelListRow[], h: ModelListHooks = hooks()) {
  container.innerHTML = "";
  renderModelList(container, rows, h);
  return h;
}

const rowEl = (name: string) => container.querySelector<HTMLElement>(`.ollama-model-row[data-model="${name}"]`)!;
const boxOf = (name: string) => rowEl(name).querySelector<HTMLInputElement>('input[data-role="select"]');
const bar = () => container.querySelector<HTMLElement>(".ollama-model-bulk");
const summary = () => container.querySelector<HTMLElement>(".ollama-model-bulk-summary")!.textContent;
const bulkButton = () => container.querySelector<HTMLButtonElement>(".ollama-model-bulk-btn")!;
const toggleOf = (name: string) => rowEl(name).querySelector<HTMLButtonElement>(".ollama-model-toggle")!;

describe("tick boxes", () => {
  it("are native checkboxes with an accessible name, only on rows that can be fetched", () => {
    draw([row("a:1"), installedRow("b:1"), row("c:1", { sizeGb: 1, blocked: true, title: "Needs Ollama ≥ 9.9.9: update the runtime first." })]);
    const a = boxOf("a:1")!;
    expect(a.type).toBe("checkbox");
    expect(a.getAttribute("aria-label")).toBe("Select Title a:1 for download");
    expect(a.disabled).toBe(false);
    expect(boxOf("b:1")).toBeNull();
    const c = boxOf("c:1")!;
    expect(c.disabled).toBe(true);
    expect(c.title).toBe("Needs Ollama ≥ 9.9.9: update the runtime first.");
  });

  it("keep every row of a list with boxes aligned: rows without one get an empty slot", () => {
    draw([row("a:1"), installedRow("b:1")]);
    expect(container.querySelector(".ollama-model-rows")!.classList.contains("ollama-model-rows--selectable")).toBe(true);
    expect(rowEl("b:1").querySelector(".ollama-model-select--empty")).not.toBeNull();

    draw([installedRow("a:1"), installedRow("b:1")]);
    expect(container.querySelector(".ollama-model-rows")!.classList.contains("ollama-model-rows--selectable")).toBe(false);
    expect(container.querySelector(".ollama-model-select")).toBeNull();
  });

  it("can be reached and toggled with the keyboard (Space on a focused checkbox is a native click)", () => {
    draw([row("a:1")]);
    const box = boxOf("a:1")!;
    box.focus();
    expect(document.activeElement).toBe(box);
    box.click(); // what Space does on a checkbox
    expect(box.checked).toBe(true);
    expect(isModelRowSelected("a:1")).toBe(true);
  });

  it("never open or close the row: not on the box, not on its label", () => {
    draw([row("a:1")]);
    const box = boxOf("a:1")!;
    box.click();
    expect(rowEl("a:1").dataset.expanded).toBe("false");
    box.closest<HTMLElement>(".ollama-model-select")!.click(); // the label forwards a click to the box
    expect(rowEl("a:1").dataset.expanded).toBe("false");
    expect(box.checked).toBe(false); // second click un-ticked it
    // A click elsewhere on the head still opens the row.
    toggleOf("a:1").click();
    expect(rowEl("a:1").dataset.expanded).toBe("true");
    boxOf("a:1")!.click();
    expect(rowEl("a:1").dataset.expanded).toBe("true");
  });

  it("a blocked box cannot be ticked", () => {
    draw([row("a:1", { sizeGb: 6.58, blocked: true })]);
    boxOf("a:1")!.click();
    expect(isModelRowSelected("a:1")).toBe(false);
    expect(summary()).toBe("0 selected · 0.0 GB");
  });
});

describe("the selection outlives a render", () => {
  it("is still ticked after the list was rebuilt from scratch, with count and size", () => {
    draw([row("a:1", { sizeGb: 6.58, blocked: false }), row("b:1", { sizeGb: 3.32, blocked: false })]);
    boxOf("a:1")!.click();
    boxOf("b:1")!.click();
    expect(summary()).toBe("2 selected · 9.9 GB");

    // New row objects, new DOM: the same models.
    draw([row("a:1", { sizeGb: 6.58, blocked: false }), row("b:1", { sizeGb: 3.32, blocked: false })]);
    expect(boxOf("a:1")!.checked).toBe(true);
    expect(boxOf("b:1")!.checked).toBe(true);
    expect(summary()).toBe("2 selected · 9.9 GB");
  });

  it("is kept when another row is opened (the list rebuilds itself) and keeps the focus on the box", () => {
    draw([row("a:1"), row("b:1")]);
    boxOf("a:1")!.focus();
    boxOf("a:1")!.click();
    toggleOf("b:1").click();
    expect(rowEl("b:1").dataset.expanded).toBe("true");
    expect(boxOf("a:1")!.checked).toBe(true);
    expect(document.activeElement).toBe(boxOf("a:1"));
  });

  it("drops a model that was installed or started downloading, and does not bring the tick back", () => {
    draw([row("a:1"), row("b:1")]);
    boxOf("a:1")!.click();
    boxOf("b:1")!.click();
    expect(summary()).toBe("2 selected · 13.2 GB");

    // a:1 is downloading now (no tick box), b:1 got installed.
    draw([
      row("a:1", null, { state: "downloading" }),
      installedRow("b:1"),
      row("c:1"),
    ]);
    expect(isModelRowSelected("a:1")).toBe(false);
    expect(isModelRowSelected("b:1")).toBe(false);
    expect(summary()).toBe("0 selected · 0.0 GB");

    // The model is available again (deleted, or the download failed): not ticked.
    draw([row("a:1"), row("b:1"), row("c:1")]);
    expect(boxOf("a:1")!.checked).toBe(false);
    expect(boxOf("b:1")!.checked).toBe(false);
  });

  it("drops a ticked model that became blocked", () => {
    draw([row("a:1"), row("b:1")]);
    boxOf("a:1")!.click();
    draw([row("a:1", { sizeGb: 6.58, blocked: true }), row("b:1")]);
    expect(boxOf("a:1")!.checked).toBe(false);
    expect(summary()).toBe("0 selected · 0.0 GB");
    expect(bulkButton().disabled).toBe(true);
  });
});

describe("the bar", () => {
  it("exists only while at least one model can still be fetched", () => {
    draw([installedRow("a:1"), row("b:1", null, { state: "downloading" })]);
    expect(bar()).toBeNull();

    draw([installedRow("a:1"), row("b:1")]);
    expect(bar()).not.toBeNull();
    expect(bar()!.getAttribute("role")).toBe("group");
    // Right under the list, outside it.
    expect(bar()!.previousElementSibling).toBe(container.querySelector(".ollama-model-rows"));

    draw([installedRow("a:1")]);
    expect(bar()).toBeNull();
  });

  it("says '0 selected' and disables the button while nothing is ticked", () => {
    draw([row("a:1")]);
    expect(summary()).toBe("0 selected · 0.0 GB");
    expect(bulkButton().disabled).toBe(true);
    expect(bulkButton().title).toContain("Select at least one");
    expect(bulkButton().textContent).toBe("Download selected");
  });

  it("counts the ticks live and sums the sizes with one decimal", () => {
    draw([row("a:1", { sizeGb: 6.58, blocked: false }), row("b:1", { sizeGb: 3.32, blocked: false }), row("c:1", { sizeGb: 4.59, blocked: false })]);
    boxOf("a:1")!.click();
    expect(summary()).toBe("1 selected · 6.6 GB");
    expect(bulkButton().disabled).toBe(false);
    boxOf("c:1")!.click();
    expect(summary()).toBe("2 selected · 11.2 GB");
    boxOf("a:1")!.click();
    expect(summary()).toBe("1 selected · 4.6 GB");
    boxOf("c:1")!.click();
    expect(summary()).toBe("0 selected · 0.0 GB");
    expect(bulkButton().disabled).toBe(true);
  });

  it("is disabled while a runtime action runs, with the reason, and is enabled again afterwards", () => {
    let busy: string | null = "The runtime is being changed. Wait until that is done.";
    const h = hooks({ downloadBlockedReason: () => busy });
    draw([row("a:1")], h);
    boxOf("a:1")!.click();
    expect(bulkButton().disabled).toBe(true);
    expect(bulkButton().title).toBe("The runtime is being changed. Wait until that is done.");
    bulkButton().click();
    expect(h.onDownloadSelected).not.toHaveBeenCalled();

    busy = null;
    draw([row("a:1")], h); // the app redraws when the runtime action ends
    expect(boxOf("a:1")!.checked).toBe(true);
    expect(bulkButton().disabled).toBe(false);
  });

  it("is disabled when everything that is ticked is blocked", () => {
    const h = hooks();
    draw([row("a:1", { sizeGb: 6.58, blocked: true }), row("b:1", { sizeGb: 3.32, blocked: true })], h);
    boxOf("a:1")!.click();
    boxOf("b:1")!.click();
    expect(summary()).toBe("0 selected · 0.0 GB");
    expect(bulkButton().disabled).toBe(true);
    bulkButton().click();
    expect(h.onDownloadSelected).not.toHaveBeenCalled();
  });

  it("hands the ticked models to the app in list order (not blocked ones) and spends the ticks", () => {
    const h = draw([
      row("a:1"),
      row("b:1", { sizeGb: 1, blocked: true }),
      row("c:1"),
      row("d:1"),
    ]);
    // Tick out of order; d:1 stays unticked.
    boxOf("c:1")!.click();
    boxOf("a:1")!.click();
    bulkButton().click();

    expect(h.onDownloadSelected).toHaveBeenCalledTimes(1);
    const handed = vi.mocked(h.onDownloadSelected!).mock.calls[0][0];
    expect(handed.map((r) => r.name)).toEqual(["a:1", "c:1"]);
    expect(summary()).toBe("0 selected · 0.0 GB");
    expect(boxOf("a:1")!.checked).toBe(false);
    expect(bulkButton().disabled).toBe(true);
  });

  it("keeps the keyboard focus on the button through a re-render while it stays enabled", () => {
    draw([row("a:1")]);
    boxOf("a:1")!.click();
    bulkButton().focus();
    const before = bulkButton();
    toggleOf("a:1").click(); // the list rebuilds itself
    expect(bulkButton()).toBe(before); // updated in place, not replaced
    expect(document.activeElement).toBe(before);
  });
});
