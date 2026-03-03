const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "textarea:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

export function getFocusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (element) => {
      if (element.hasAttribute("disabled")) return false;
      if (element.getAttribute("aria-hidden") === "true") return false;
      return true;
    }
  );
}

export function focusFirstElement(container: HTMLElement, fallback?: HTMLElement | null): void {
  const focusable = getFocusableElements(container);
  if (focusable.length > 0) {
    focusable[0].focus();
    return;
  }
  fallback?.focus();
}

export function trapFocusInModal(event: KeyboardEvent, container: HTMLElement): void {
  if (event.key !== "Tab") return;

  const focusable = getFocusableElements(container);
  if (focusable.length === 0) return;

  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement as HTMLElement | null;
  const activeInside = !!active && container.contains(active);

  if (event.shiftKey) {
    if (!activeInside || active === first) {
      event.preventDefault();
      last.focus();
    }
    return;
  }

  if (!activeInside || active === last) {
    event.preventDefault();
    first.focus();
  }
}

