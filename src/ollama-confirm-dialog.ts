export type ConfirmDialogOptions = {
  title: string;
  /** One paragraph per entry (plain text). */
  body: string[];
  confirmLabel: string;
  cancelLabel?: string;
  /** Focus "Cancel" first (data is removed or replaced). */
  careful?: boolean;
};

let dialogCounter = 0;

/**
 * In-page confirmation (same `auth-modal` look as the other dialogs of the app, built on demand).
 * Resolves true on confirm, false on cancel / Escape / backdrop click.
 */
export function confirmDialog(options: ConfirmDialogOptions): Promise<boolean> {
  return new Promise((resolve) => {
    const id = `confirm-dialog-${++dialogCounter}`;
    const root = document.createElement("div");
    root.className = "auth-modal ollama-confirm-dialog";
    root.id = id;

    const backdrop = document.createElement("div");
    backdrop.className = "auth-modal-backdrop";

    const card = document.createElement("div");
    card.className = "auth-modal-card";
    card.setAttribute("role", "dialog");
    card.setAttribute("aria-modal", "true");
    card.setAttribute("aria-labelledby", `${id}-title`);

    const header = document.createElement("div");
    header.className = "auth-modal-header";
    const title = document.createElement("h3");
    title.id = `${id}-title`;
    title.className = "ai-refine-section-title";
    title.textContent = options.title;
    header.appendChild(title);
    card.appendChild(header);

    for (const text of options.body) {
      const paragraph = document.createElement("p");
      paragraph.className = "field-hint";
      paragraph.textContent = text;
      card.appendChild(paragraph);
    }

    const actions = document.createElement("div");
    actions.className = "auth-modal-actions";
    const confirm = document.createElement("button");
    confirm.type = "button";
    confirm.className = "hotkey-record-btn is-recommended";
    confirm.textContent = options.confirmLabel;
    confirm.dataset.dialogAction = "confirm";
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "hotkey-record-btn";
    cancel.textContent = options.cancelLabel ?? "Cancel";
    cancel.dataset.dialogAction = "cancel";
    actions.append(confirm, cancel);
    card.appendChild(actions);

    root.append(backdrop, card);

    let settled = false;
    const finish = (result: boolean) => {
      if (settled) return;
      settled = true;
      document.removeEventListener("keydown", onKeyDown, true);
      root.remove();
      resolve(result);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        finish(false);
      }
    };

    confirm.addEventListener("click", () => finish(true));
    cancel.addEventListener("click", () => finish(false));
    backdrop.addEventListener("click", () => finish(false));
    document.addEventListener("keydown", onKeyDown, true);

    document.body.appendChild(root);
    (options.careful ? cancel : confirm).focus();
  });
}
