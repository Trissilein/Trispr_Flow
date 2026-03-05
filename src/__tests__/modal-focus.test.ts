import { describe, expect, it, beforeEach } from "vitest";
import {
  focusFirstElement,
  getFocusableElements,
  trapFocusInModal,
} from "../modal-focus";

describe("modal-focus", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("returns only enabled focusable elements", () => {
    document.body.innerHTML = `
      <div id="modal">
        <button id="a" type="button">A</button>
        <button id="b" type="button" disabled>B</button>
        <input id="c" type="text" />
      </div>
    `;
    const modal = document.getElementById("modal") as HTMLElement;
    const ids = getFocusableElements(modal).map((el) => el.id);
    expect(ids).toEqual(["a", "c"]);
  });

  it("focuses first element when modal opens", () => {
    document.body.innerHTML = `
      <div id="modal">
        <button id="first" type="button">First</button>
        <button id="second" type="button">Second</button>
      </div>
      <button id="fallback" type="button">Fallback</button>
    `;
    const modal = document.getElementById("modal") as HTMLElement;
    const first = document.getElementById("first") as HTMLButtonElement;
    focusFirstElement(modal, document.getElementById("fallback") as HTMLElement);
    expect(document.activeElement).toBe(first);
  });

  it("cycles Tab from last to first", () => {
    document.body.innerHTML = `
      <div id="modal">
        <button id="first" type="button">First</button>
        <button id="last" type="button">Last</button>
      </div>
    `;
    const modal = document.getElementById("modal") as HTMLElement;
    const first = document.getElementById("first") as HTMLButtonElement;
    const last = document.getElementById("last") as HTMLButtonElement;
    last.focus();

    const event = new KeyboardEvent("keydown", { key: "Tab", cancelable: true });
    trapFocusInModal(event, modal);

    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(first);
  });

  it("cycles Shift+Tab from first to last", () => {
    document.body.innerHTML = `
      <div id="modal">
        <button id="first" type="button">First</button>
        <button id="last" type="button">Last</button>
      </div>
    `;
    const modal = document.getElementById("modal") as HTMLElement;
    const first = document.getElementById("first") as HTMLButtonElement;
    const last = document.getElementById("last") as HTMLButtonElement;
    first.focus();

    const event = new KeyboardEvent("keydown", {
      key: "Tab",
      shiftKey: true,
      cancelable: true,
    });
    trapFocusInModal(event, modal);

    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(last);
  });
});

