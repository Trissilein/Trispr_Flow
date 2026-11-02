/**
 * Zebra list: one row per item, alternating tint, a few fixed columns, chips for states and a
 * handful of row actions (Install / Switch to / Delete, Download / Delete / Activate, ...).
 * Used for the Ollama runtime versions and, next, for the models. Pure DOM, no innerHTML, so
 * every string is text (no escaping needed, nothing from a registry or the disk can inject markup).
 *
 * Usage:
 *
 *   renderZebraList(host, {
 *     label: "Runtime versions",                      // accessible name of the table
 *     columns: [
 *       { id: "version", label: "Version" },
 *       { id: "size", label: "Size", align: "end", width: "5rem" },   // width = CSS grid/table width
 *       { id: "status", label: "Status" },
 *     ],
 *     actionsLabel: "Actions",                        // header of the last column (default "Actions")
 *     emptyText: "No versions yet.",                  // shown instead of the table when rows is []
 *     busy: false,                                    // true: every action is inert (aria-disabled)
 *     rows: [{
 *       id: "0.40.13",                                // stable key; focus is restored by it
 *       label: "0.40.13",                             // human name, used in aria-labels
 *       cells: { version: "0.40.13", size: "1.6 GB",
 *                status: [{ label: "Latest", tone: "ok" }] },   // string or chips
 *       actions: [{ id: "install", label: "Install", tone: "primary" }],
 *       marker: { label: "Running", tone: "ok" },     // chip in the actions cell, for a row without actions
 *       current: true,                                // the row in use: accent bar + aria-current
 *       note: { text: "Install failed: ...", tone: "error" },   // line under the row
 *     }],
 *     onAction: (rowId, actionId) => { ... },         // may be async; errors are logged, not thrown
 *   });
 *
 * Call it again with new data whenever something changes: the table is rebuilt and the keyboard
 * focus goes back to the same button (same row and action; else the same row; else the row that
 * took its place). Keyboard: Tab / Shift+Tab walk the buttons, Enter/Space activate, ArrowUp /
 * ArrowDown jump to the same action one row up/down, Home/End to the first/last row.
 * Semantics: table / row / columnheader / cell; actions are real buttons, an inert one keeps
 * `aria-disabled="true"` (focusable, with its title as the reason) instead of `disabled`.
 * Styling: `ollama-zebra-*` in styles.css (rows alternate `is-odd` / `is-even`).
 */

export type ZebraTone = "ok" | "accent" | "info" | "warn" | "danger" | "neutral";

export type ZebraChip = { label: string; tone?: ZebraTone; title?: string };

export type ZebraColumn = {
  id: string;
  label: string;
  /** CSS width of the column (`"5rem"`, `"30%"`); the rest shares what is left. */
  width?: string;
  align?: "start" | "end";
};

export type ZebraCell = string | ZebraChip[];

export type ZebraAction = {
  id: string;
  label: string;
  /** Tooltip; for an inert action it should say why. */
  title?: string;
  tone?: "default" | "primary" | "danger";
  /** Shown but inert (`aria-disabled`). */
  disabled?: boolean;
};

export type ZebraRowNote = { text: string; tone?: "info" | "warn" | "error" };

export type ZebraRow = {
  id: string;
  label: string;
  cells: Record<string, ZebraCell>;
  actions?: ZebraAction[];
  marker?: ZebraChip;
  current?: boolean;
  note?: ZebraRowNote;
};

export type ZebraListSpec = {
  label: string;
  columns: ZebraColumn[];
  actionsLabel?: string;
  rows: ZebraRow[];
  emptyText?: string;
  busy?: boolean;
  onAction: (rowId: string, actionId: string) => void | Promise<void>;
};

const CLS = "ollama-zebra";

/** The handler of the latest render per host (the click/keydown listeners are bound once). */
const handlers = new WeakMap<HTMLElement, Pick<ZebraListSpec, "onAction">>();

type FocusTarget = { rowId: string; actionId: string; rowIndex: number };

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function chipEl(chip: ZebraChip): HTMLSpanElement {
  const node = el("span", `${CLS}-chip ${CLS}-chip--${chip.tone ?? "neutral"}`, chip.label);
  if (chip.title) node.title = chip.title;
  return node;
}

function readFocus(host: HTMLElement): FocusTarget | null {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement) || !host.contains(active)) return null;
  const button = active.closest<HTMLElement>(`button[data-zebra-action]`);
  const row = active.closest<HTMLElement>(`tr[data-zebra-row]`);
  if (!button || !row) return null;
  const rows = Array.from(host.querySelectorAll<HTMLElement>(`tr[data-zebra-row]:not(.${CLS}-note-row)`));
  return {
    rowId: row.dataset.zebraRow ?? "",
    actionId: button.dataset.zebraAction ?? "",
    rowIndex: Math.max(0, rows.indexOf(row)),
  };
}

function restoreFocus(host: HTMLElement, target: FocusTarget | null): void {
  if (!target) return;
  const rows = Array.from(host.querySelectorAll<HTMLElement>(`tr.${CLS}-row`));
  const buttonsOf = (row: HTMLElement | undefined) =>
    row ? Array.from(row.querySelectorAll<HTMLElement>("button[data-zebra-action]")) : [];
  const sameRow = rows.find((row) => row.dataset.zebraRow === target.rowId);
  const replacement = rows[Math.min(target.rowIndex, rows.length - 1)];
  const candidates = [
    buttonsOf(sameRow).find((button) => button.dataset.zebraAction === target.actionId),
    buttonsOf(sameRow)[0],
    buttonsOf(replacement)[0],
  ];
  candidates.find((button) => button)?.focus();
}

function buildTable(spec: ZebraListSpec): HTMLTableElement {
  const table = el("table", `${CLS}-table`);
  table.setAttribute("aria-label", spec.label);
  if (spec.busy) table.setAttribute("aria-busy", "true");

  const colgroup = el("colgroup");
  for (const column of spec.columns) {
    const col = el("col");
    if (column.width) col.style.width = column.width;
    colgroup.appendChild(col);
  }
  const actionsCol = el("col");
  actionsCol.className = `${CLS}-col-actions`;
  colgroup.appendChild(actionsCol);
  table.appendChild(colgroup);

  const headRow = el("tr", `${CLS}-head-row`);
  for (const column of spec.columns) {
    const th = el("th", `${CLS}-th${column.align === "end" ? ` ${CLS}-end` : ""}`, column.label);
    th.scope = "col";
    headRow.appendChild(th);
  }
  const actionsTh = el("th", `${CLS}-th ${CLS}-end`, spec.actionsLabel ?? "Actions");
  actionsTh.scope = "col";
  headRow.appendChild(actionsTh);
  table.appendChild(el("thead")).appendChild(headRow);

  const body = el("tbody");
  spec.rows.forEach((row, index) => {
    const parity = index % 2 === 0 ? "is-odd" : "is-even";
    const tr = el("tr", `${CLS}-row ${parity}${row.current ? " is-current" : ""}${row.note ? " has-note" : ""}`);
    tr.dataset.zebraRow = row.id;
    if (row.current) tr.setAttribute("aria-current", "true");

    for (const column of spec.columns) {
      const td = el("td", `${CLS}-cell${column.align === "end" ? ` ${CLS}-end` : ""}`);
      td.dataset.zebraColumn = column.id;
      const cell = row.cells[column.id];
      if (Array.isArray(cell)) {
        const chips = el("span", `${CLS}-chips`);
        cell.forEach((chip) => chips.appendChild(chipEl(chip)));
        td.appendChild(chips);
      } else {
        td.textContent = cell ?? "";
      }
      tr.appendChild(td);
    }

    const actionsTd = el("td", `${CLS}-cell ${CLS}-actions ${CLS}-end`);
    const actions = row.actions ?? [];
    if (actions.length > 0) {
      for (const action of actions) {
        const button = el(
          "button",
          `hotkey-record-btn ${CLS}-btn ${CLS}-btn--${action.tone ?? "default"}${action.disabled || spec.busy ? " is-disabled" : ""}`,
          action.label
        );
        button.type = "button";
        button.dataset.zebraAction = action.id;
        button.setAttribute("aria-label", `${action.label} ${row.label}`);
        if (action.title) button.title = action.title;
        if (action.disabled || spec.busy) button.setAttribute("aria-disabled", "true");
        actionsTd.appendChild(button);
      }
    } else if (row.marker) {
      actionsTd.appendChild(chipEl(row.marker));
    }
    tr.appendChild(actionsTd);
    body.appendChild(tr);

    if (row.note) {
      const noteRow = el("tr", `${CLS}-note-row ${parity}${row.current ? " is-current" : ""}`);
      noteRow.dataset.zebraRow = row.id;
      const noteCell = el("td", `${CLS}-note ${CLS}-note--${row.note.tone ?? "info"}`, row.note.text);
      noteCell.colSpan = spec.columns.length + 1;
      noteRow.appendChild(noteCell);
      body.appendChild(noteRow);
    }
  });
  table.appendChild(body);
  return table;
}

function focusableButtons(row: HTMLElement): HTMLElement[] {
  return Array.from(row.querySelectorAll<HTMLElement>("button[data-zebra-action]"));
}

function bindOnce(host: HTMLElement): void {
  if (host.dataset.zebraBound === "true") return;
  host.dataset.zebraBound = "true";

  host.addEventListener("click", (event) => {
    const button = (event.target as Element | null)?.closest<HTMLElement>("button[data-zebra-action]");
    if (!button || !host.contains(button)) return;
    if (button.getAttribute("aria-disabled") === "true") {
      event.preventDefault();
      return;
    }
    const rowId = button.closest<HTMLElement>("tr[data-zebra-row]")?.dataset.zebraRow;
    const actionId = button.dataset.zebraAction;
    const handler = handlers.get(host);
    if (!rowId || !actionId || !handler) return;
    void Promise.resolve()
      .then(() => handler.onAction(rowId, actionId))
      .catch((error) => console.error(`Zebra list action ${actionId} on ${rowId} failed:`, error));
  });

  host.addEventListener("keydown", (event) => {
    const keys = ["ArrowUp", "ArrowDown", "Home", "End"];
    if (!keys.includes(event.key) || event.altKey || event.ctrlKey || event.metaKey) return;
    const button = (event.target as Element | null)?.closest<HTMLElement>("button[data-zebra-action]");
    const row = button?.closest<HTMLElement>(`tr.${CLS}-row`);
    if (!button || !row) return;
    const rows = Array.from(host.querySelectorAll<HTMLElement>(`tr.${CLS}-row`));
    const index = rows.indexOf(row);
    const nextIndex =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? rows.length - 1
          : index + (event.key === "ArrowDown" ? 1 : -1);
    if (nextIndex < 0 || nextIndex >= rows.length) return;
    // Rows differ in their buttons: the same action when the next row has it, else its first button.
    const wanted = button.dataset.zebraAction;
    const candidates = focusableButtons(rows[nextIndex]);
    const target = candidates.find((candidate) => candidate.dataset.zebraAction === wanted) ?? candidates[0];
    if (!target) return;
    event.preventDefault();
    target.focus();
  });
}

/** Renders (or re-renders) the list into `host` and keeps the keyboard focus on the same button. */
export function renderZebraList(host: HTMLElement, spec: ZebraListSpec): void {
  const focus = readFocus(host);
  handlers.set(host, { onAction: spec.onAction });
  bindOnce(host);
  host.classList.add(`${CLS}-host`);

  if (spec.rows.length === 0) {
    const empty = el("p", `${CLS}-empty`, spec.emptyText ?? "Nothing to show.");
    empty.setAttribute("role", "status");
    host.replaceChildren(empty);
    return;
  }
  host.replaceChildren(buildTable(spec));
  restoreFocus(host, focus);
}
