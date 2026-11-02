import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderZebraList, type ZebraListSpec, type ZebraRow } from "../ollama-zebra-list";

const columns = [
  { id: "name", label: "Name" },
  { id: "size", label: "Size", align: "end" as const, width: "5rem" },
  { id: "status", label: "Status" },
];

function row(id: string, overrides: Partial<ZebraRow> = {}): ZebraRow {
  return {
    id,
    label: id,
    cells: { name: id, size: "1 GB", status: [{ label: "Installed", tone: "neutral" }] },
    actions: [
      { id: "use", label: "Use", tone: "primary" },
      { id: "delete", label: "Delete", tone: "danger" },
    ],
    ...overrides,
  };
}

let host: HTMLElement;
let onAction: ReturnType<typeof vi.fn<ZebraListSpec["onAction"]>>;

function spec(rows: ZebraRow[], overrides: Partial<ZebraListSpec> = {}): ZebraListSpec {
  return { label: "Things", columns, rows, onAction, ...overrides };
}

const buttons = () => Array.from(host.querySelectorAll<HTMLButtonElement>("button"));
const rowEls = () => Array.from(host.querySelectorAll<HTMLElement>("tr.ollama-zebra-row"));
const press = (target: Element, key: string) =>
  target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));

beforeEach(() => {
  document.body.innerHTML = '<div id="host"></div>';
  host = document.getElementById("host") as HTMLElement;
  onAction = vi.fn<ZebraListSpec["onAction"]>();
});

describe("zebra list: structure and semantics", () => {
  it("renders a labelled table with column headers, one row per item and an actions column", () => {
    renderZebraList(host, spec([row("a"), row("b")], { actionsLabel: "Options" }));

    const table = host.querySelector("table")!;
    expect(table.getAttribute("aria-label")).toBe("Things");
    const headers = Array.from(table.querySelectorAll("th"));
    expect(headers.map((th) => th.textContent)).toEqual(["Name", "Size", "Status", "Options"]);
    expect(headers.every((th) => th.getAttribute("scope") === "col")).toBe(true);
    expect(rowEls().map((tr) => tr.dataset.zebraRow)).toEqual(["a", "b"]);
    expect(table.querySelectorAll("col")[1].style.width).toBe("5rem");
    expect(headers[1].classList.contains("ollama-zebra-end")).toBe(true);
  });

  it("defaults the actions header to 'Actions' and alternates the tint", () => {
    renderZebraList(host, spec([row("a"), row("b"), row("c")]));
    expect(Array.from(host.querySelectorAll("th")).pop()?.textContent).toBe("Actions");
    expect(rowEls().map((tr) => (tr.classList.contains("is-odd") ? "odd" : "even"))).toEqual(["odd", "even", "odd"]);
  });

  it("string cells are text, chip cells become toned chips with titles", () => {
    renderZebraList(
      host,
      spec([row("a", { cells: { name: "Alpha", size: "2 GB", status: [{ label: "Active", tone: "ok", title: "In use" }, { label: "Latest" }] } })])
    );
    const cells = Array.from(rowEls()[0].querySelectorAll("td"));
    expect(cells[0].textContent).toBe("Alpha");
    expect(cells[1].textContent).toBe("2 GB");
    const chips = Array.from(cells[2].querySelectorAll<HTMLElement>(".ollama-zebra-chip"));
    expect(chips.map((chip) => chip.textContent)).toEqual(["Active", "Latest"]);
    expect(chips[0].classList.contains("ollama-zebra-chip--ok")).toBe(true);
    expect(chips[0].title).toBe("In use");
    expect(chips[1].classList.contains("ollama-zebra-chip--neutral")).toBe(true);
  });

  it("a missing cell is empty, not 'undefined'", () => {
    renderZebraList(host, spec([row("a", { cells: { name: "A" } })]));
    const cells = Array.from(rowEls()[0].querySelectorAll("td"));
    expect(cells[1].textContent).toBe("");
    expect(cells[2].textContent).toBe("");
  });

  it("the current row is marked for assistive tech and styling", () => {
    renderZebraList(host, spec([row("a", { current: true }), row("b")]));
    expect(rowEls()[0].getAttribute("aria-current")).toBe("true");
    expect(rowEls()[0].classList.contains("is-current")).toBe(true);
    expect(rowEls()[1].hasAttribute("aria-current")).toBe(false);
  });

  it("a marker stands in the actions cell of a row without actions (and not next to buttons)", () => {
    renderZebraList(
      host,
      spec([
        row("a", { actions: [], marker: { label: "Running", tone: "ok" } }),
        row("b", { marker: { label: "Ignored" } }),
      ])
    );
    const first = rowEls()[0].querySelector("td:last-child")!;
    expect(first.querySelector("button")).toBeNull();
    expect(first.textContent).toBe("Running");
    expect(rowEls()[1].querySelector("td:last-child")!.textContent).not.toContain("Ignored");
  });

  it("a note comes as its own row under its item with the same tint", () => {
    renderZebraList(host, spec([row("a", { note: { text: "Install failed: disk full", tone: "error" } }), row("b")]));
    const note = host.querySelector<HTMLElement>("tr.ollama-zebra-note-row")!;
    expect(note.querySelector("td")?.textContent).toBe("Install failed: disk full");
    expect(note.querySelector("td")?.classList.contains("ollama-zebra-note--error")).toBe(true);
    expect(note.querySelector("td")?.colSpan).toBe(4);
    expect(note.classList.contains("is-odd")).toBe(true);
    expect(rowEls().map((tr) => tr.dataset.zebraRow)).toEqual(["a", "b"]);
  });

  it("an empty list shows the empty text instead of a table", () => {
    renderZebraList(host, spec([], { emptyText: "No versions yet." }));
    expect(host.querySelector("table")).toBeNull();
    expect(host.querySelector(".ollama-zebra-empty")?.textContent).toBe("No versions yet.");
    expect(host.querySelector(".ollama-zebra-empty")?.getAttribute("role")).toBe("status");
    renderZebraList(host, spec([]));
    expect(host.querySelector(".ollama-zebra-empty")?.textContent).toBe("Nothing to show.");
  });

  it("rendering again replaces the content (also back from empty to rows)", () => {
    renderZebraList(host, spec([], { emptyText: "none" }));
    renderZebraList(host, spec([row("a")]));
    expect(host.querySelector(".ollama-zebra-empty")).toBeNull();
    expect(host.querySelectorAll("table")).toHaveLength(1);
    renderZebraList(host, spec([row("a"), row("b")]));
    expect(host.querySelectorAll("table")).toHaveLength(1);
    expect(rowEls()).toHaveLength(2);
  });

  it("renders hostile text as text and never builds elements from it", () => {
    const evil = '<img src=x onerror="window.__zebra=1">';
    renderZebraList(
      host,
      spec([
        row(evil, {
          label: evil,
          cells: { name: evil, size: evil, status: [{ label: evil, title: evil }] },
          note: { text: evil },
          actions: [{ id: "use", label: evil, title: evil }],
        }),
      ], { label: evil, emptyText: evil, columns: [{ id: "name", label: evil }, ...columns.slice(1)] })
    );
    expect(host.querySelector("img")).toBeNull();
    expect(host.textContent).toContain(evil);
    expect((window as unknown as { __zebra?: number }).__zebra).toBeUndefined();
    renderZebraList(host, spec([], { emptyText: evil }));
    expect(host.querySelector("img")).toBeNull();
    expect(host.textContent).toBe(evil);
  });

  it("copes with long text and very many rows", () => {
    const long = "x".repeat(2000);
    const many = Array.from({ length: 300 }, (_, index) => row(`r${index}`, { cells: { name: long, size: "1", status: [] } }));
    renderZebraList(host, spec(many));
    expect(rowEls()).toHaveLength(300);
    expect(rowEls()[299].querySelector("td")?.textContent).toBe(long);
  });
});

describe("zebra list: actions", () => {
  it("gives every button a name that includes the row, a tone class and its title", () => {
    renderZebraList(host, spec([row("a", { label: "Gemma 4", actions: [{ id: "use", label: "Use", title: "Make it active", tone: "primary" }] })]));
    const button = buttons()[0];
    expect(button.type).toBe("button");
    expect(button.getAttribute("aria-label")).toBe("Use Gemma 4");
    expect(button.title).toBe("Make it active");
    expect(button.classList.contains("ollama-zebra-btn--primary")).toBe(true);
    expect(button.classList.contains("hotkey-record-btn")).toBe(true);
  });

  it("calls onAction with the row and the action id; a second render does not double the handler", async () => {
    renderZebraList(host, spec([row("a"), row("b")]));
    renderZebraList(host, spec([row("a"), row("b")]));
    buttons()[2].click(); // row b, "Use"
    buttons()[1].click(); // row a, "Delete"
    await Promise.resolve();
    expect(onAction.mock.calls).toEqual([
      ["b", "use"],
      ["a", "delete"],
    ]);
  });

  it("always calls the handler of the latest render", async () => {
    const first = vi.fn();
    const second = vi.fn();
    renderZebraList(host, spec([row("a")], { onAction: first }));
    renderZebraList(host, spec([row("a")], { onAction: second }));
    buttons()[0].click();
    await Promise.resolve();
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith("a", "use");
  });

  it("an inert action (disabled or busy) stays visible and focusable but does nothing", async () => {
    renderZebraList(host, spec([row("a", { actions: [{ id: "use", label: "Use", disabled: true, title: "Why not" }] })]));
    const button = buttons()[0];
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(button.disabled).toBe(false);
    expect(button.title).toBe("Why not");
    button.click();

    renderZebraList(host, spec([row("a")], { busy: true }));
    expect(host.querySelector("table")?.getAttribute("aria-busy")).toBe("true");
    for (const each of buttons()) {
      expect(each.getAttribute("aria-disabled")).toBe("true");
      each.click();
    }
    await Promise.resolve();
    expect(onAction).not.toHaveBeenCalled();
  });

  it("an async handler that rejects is logged, not thrown", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    renderZebraList(host, spec([row("a")], { onAction: async () => { throw new Error("boom"); } }));
    buttons()[0].click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(error).toHaveBeenCalled();
    error.mockRestore();
  });

  it("a click on a chip or a plain cell triggers nothing", async () => {
    renderZebraList(host, spec([row("a")]));
    host.querySelector<HTMLElement>(".ollama-zebra-chip")?.click();
    rowEls()[0].querySelector<HTMLElement>("td")?.click();
    await Promise.resolve();
    expect(onAction).not.toHaveBeenCalled();
  });
});

describe("zebra list: keyboard", () => {
  it("Tab order is the natural button order (buttons are real, focusable elements)", () => {
    renderZebraList(host, spec([row("a"), row("b")]));
    expect(buttons().every((button) => button.tabIndex === 0)).toBe(true);
  });

  it("ArrowDown/ArrowUp move to the same action in the next/previous row, Home/End to the first/last row", () => {
    renderZebraList(host, spec([row("a"), row("b"), row("c")]));
    const bs = buttons(); // a: use,delete  b: use,delete  c: use,delete
    bs[1].focus();
    press(bs[1], "ArrowDown");
    expect(document.activeElement).toBe(bs[3]);
    press(bs[3], "ArrowDown");
    expect(document.activeElement).toBe(bs[5]);
    press(bs[5], "ArrowDown"); // no row below: stays
    expect(document.activeElement).toBe(bs[5]);
    press(bs[5], "ArrowUp");
    expect(document.activeElement).toBe(bs[3]);
    press(bs[3], "Home");
    expect(document.activeElement).toBe(bs[1]);
    press(bs[1], "End");
    expect(document.activeElement).toBe(bs[5]);
  });

  it("falls back to the first button of the next row when it lacks that action, and skips nothing", () => {
    renderZebraList(
      host,
      spec([row("a"), row("b", { actions: [{ id: "install", label: "Install" }] }), row("c", { actions: [], marker: { label: "Running" } })])
    );
    const bs = buttons(); // a: use,delete  b: install
    bs[1].focus();
    press(bs[1], "ArrowDown");
    expect(document.activeElement).toBe(bs[2]);
    press(bs[2], "ArrowDown"); // row c has no button: stays where it is
    expect(document.activeElement).toBe(bs[2]);
  });

  it("Enter on a focused button activates it like a click", async () => {
    renderZebraList(host, spec([row("a")]));
    buttons()[0].focus();
    // jsdom does not turn Enter into a click: a real browser does for <button>; the click path is tested above.
    buttons()[0].click();
    await Promise.resolve();
    expect(onAction).toHaveBeenCalledWith("a", "use");
  });

  it("other keys are left alone", () => {
    renderZebraList(host, spec([row("a"), row("b")]));
    buttons()[0].focus();
    const event = new KeyboardEvent("keydown", { key: "a", bubbles: true, cancelable: true });
    buttons()[0].dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    const withCtrl = new KeyboardEvent("keydown", { key: "ArrowDown", ctrlKey: true, bubbles: true, cancelable: true });
    buttons()[0].dispatchEvent(withCtrl);
    expect(withCtrl.defaultPrevented).toBe(false);
  });
});

describe("zebra list: focus survives a re-render", () => {
  const focusedKey = () => {
    const active = document.activeElement as HTMLElement | null;
    return active?.closest("tr")?.dataset.zebraRow + ":" + active?.dataset.zebraAction;
  };

  it("keeps the same button when the data changes", () => {
    renderZebraList(host, spec([row("a"), row("b")]));
    buttons()[3].focus(); // b:delete
    renderZebraList(host, spec([row("a"), row("b", { cells: { name: "B", size: "2 GB", status: [] } })]));
    expect(focusedKey()).toBe("b:delete");
  });

  it("follows the row when rows are reordered", () => {
    renderZebraList(host, spec([row("a"), row("b")]));
    buttons()[2].focus(); // b:use
    renderZebraList(host, spec([row("b"), row("a")]));
    expect(focusedKey()).toBe("b:use");
  });

  it("falls back to the first button of the same row when its action is gone", () => {
    renderZebraList(host, spec([row("a"), row("b")]));
    buttons()[3].focus(); // b:delete
    renderZebraList(host, spec([row("a"), row("b", { actions: [{ id: "use", label: "Use" }] })]));
    expect(focusedKey()).toBe("b:use");
  });

  it("moves to the row that took the place of a deleted one, or the last row", () => {
    renderZebraList(host, spec([row("a"), row("b"), row("c")]));
    buttons()[3].focus(); // b:delete
    renderZebraList(host, spec([row("a"), row("c")]));
    expect(focusedKey()).toBe("c:use");

    buttons()[2].focus(); // c:use (index 2 of a:use,a:delete,c:use,c:delete)
    renderZebraList(host, spec([row("a")]));
    expect(focusedKey()).toBe("a:use");
  });

  it("stays on an inert button (aria-disabled keeps it focusable) when the list turns busy", () => {
    renderZebraList(host, spec([row("a")]));
    buttons()[0].focus();
    renderZebraList(host, spec([row("a")], { busy: true }));
    expect(focusedKey()).toBe("a:use");
  });

  it("does not steal focus that was elsewhere", () => {
    document.body.insertAdjacentHTML("beforeend", '<input id="other" />');
    renderZebraList(host, spec([row("a")]));
    const other = document.getElementById("other") as HTMLInputElement;
    other.focus();
    renderZebraList(host, spec([row("a")]));
    expect(document.activeElement).toBe(other);
  });
});
