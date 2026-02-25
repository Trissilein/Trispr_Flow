type TooltipContent = {
  title?: string;
  body: string;
  consequence?: string;
};

const TOOLTIP_TARGET_SELECTOR = "[data-tooltip-body], [data-tooltip-native]";

let initialized = false;
let activeTarget: HTMLElement | null = null;
let observer: MutationObserver | null = null;

let tooltipEl: HTMLDivElement | null = null;
let tooltipTitleEl: HTMLDivElement | null = null;
let tooltipBodyEl: HTMLDivElement | null = null;
let tooltipConsequenceEl: HTMLDivElement | null = null;

function clean(value: string | undefined): string {
  return value?.trim() ?? "";
}

function ensureTooltipElement(): void {
  if (tooltipEl || typeof document === "undefined") return;

  tooltipEl = document.createElement("div");
  tooltipEl.className = "app-tooltip";
  tooltipEl.setAttribute("role", "tooltip");
  tooltipEl.setAttribute("aria-hidden", "true");

  tooltipTitleEl = document.createElement("div");
  tooltipTitleEl.className = "app-tooltip-title";
  tooltipEl.appendChild(tooltipTitleEl);

  tooltipBodyEl = document.createElement("div");
  tooltipBodyEl.className = "app-tooltip-body";
  tooltipEl.appendChild(tooltipBodyEl);

  tooltipConsequenceEl = document.createElement("div");
  tooltipConsequenceEl.className = "app-tooltip-consequence";
  tooltipEl.appendChild(tooltipConsequenceEl);

  document.body.appendChild(tooltipEl);
}

function resolveTooltipContent(target: HTMLElement): TooltipContent | null {
  const body = clean(target.dataset.tooltipBody) || clean(target.dataset.tooltipNative);
  if (!body) return null;

  const title = clean(target.dataset.tooltipTitle);
  const consequence = clean(target.dataset.tooltipConsequence);

  return {
    title: title || undefined,
    body,
    consequence: consequence || undefined,
  };
}

function convertTitleElement(element: HTMLElement): void {
  const title = element.getAttribute("title");
  if (!title) return;

  const trimmed = title.trim();
  if (!trimmed) {
    element.removeAttribute("title");
    return;
  }

  element.dataset.tooltipNative = trimmed;
  if (!element.getAttribute("aria-label")) {
    element.setAttribute("aria-label", trimmed);
  }
  element.removeAttribute("title");
}

function convertNativeTitles(root: ParentNode): void {
  if (root instanceof HTMLElement && root.hasAttribute("title")) {
    convertTitleElement(root);
  }

  if (!("querySelectorAll" in root)) return;
  root.querySelectorAll<HTMLElement>("[title]").forEach((element) => {
    convertTitleElement(element);
  });
}

function findTooltipTarget(target: EventTarget | null): HTMLElement | null {
  if (!(target instanceof Element)) return null;
  return target.closest<HTMLElement>(TOOLTIP_TARGET_SELECTOR);
}

function positionTooltip(target: HTMLElement): void {
  if (!tooltipEl) return;

  const margin = 10;
  const viewportMargin = 8;

  tooltipEl.style.left = "0px";
  tooltipEl.style.top = "0px";
  tooltipEl.style.visibility = "hidden";
  tooltipEl.classList.add("is-visible");

  const targetRect = target.getBoundingClientRect();
  const tooltipRect = tooltipEl.getBoundingClientRect();

  let top = targetRect.bottom + margin;
  const fitsBelow = top + tooltipRect.height <= window.innerHeight - viewportMargin;
  if (!fitsBelow) {
    const topAbove = targetRect.top - tooltipRect.height - margin;
    if (topAbove >= viewportMargin) {
      top = topAbove;
    }
  }

  let left = targetRect.left + (targetRect.width - tooltipRect.width) / 2;
  left = Math.max(viewportMargin, Math.min(left, window.innerWidth - tooltipRect.width - viewportMargin));

  tooltipEl.style.left = `${Math.round(left + window.scrollX)}px`;
  tooltipEl.style.top = `${Math.round(top + window.scrollY)}px`;
  tooltipEl.style.visibility = "visible";
}

function showTooltip(target: HTMLElement): void {
  ensureTooltipElement();
  if (!tooltipEl || !tooltipBodyEl || !tooltipTitleEl || !tooltipConsequenceEl) return;

  const content = resolveTooltipContent(target);
  if (!content) {
    hideTooltip();
    return;
  }

  activeTarget = target;

  tooltipTitleEl.textContent = content.title ?? "";
  tooltipTitleEl.style.display = content.title ? "block" : "none";

  tooltipBodyEl.textContent = content.body;

  tooltipConsequenceEl.textContent = content.consequence ?? "";
  tooltipConsequenceEl.style.display = content.consequence ? "block" : "none";

  tooltipEl.classList.add("is-visible");
  tooltipEl.setAttribute("aria-hidden", "false");
  positionTooltip(target);
}

function hideTooltip(): void {
  activeTarget = null;
  if (!tooltipEl) return;
  tooltipEl.classList.remove("is-visible");
  tooltipEl.setAttribute("aria-hidden", "true");
}

function onMouseOver(event: MouseEvent): void {
  const target = findTooltipTarget(event.target);
  if (!target) {
    hideTooltip();
    return;
  }
  if (target === activeTarget) return;
  showTooltip(target);
}

function onMouseOut(event: MouseEvent): void {
  if (!activeTarget) return;
  if (!(event.target instanceof Node) || !activeTarget.contains(event.target)) return;

  const related = event.relatedTarget;
  if (related instanceof Node && activeTarget.contains(related)) return;

  hideTooltip();
}

function onFocusIn(event: FocusEvent): void {
  const target = findTooltipTarget(event.target);
  if (target) {
    showTooltip(target);
  }
}

function onFocusOut(event: FocusEvent): void {
  if (!activeTarget) return;

  const related = event.relatedTarget;
  if (related instanceof Node && activeTarget.contains(related)) return;

  if (document.activeElement instanceof Node && activeTarget.contains(document.activeElement)) return;
  hideTooltip();
}

function onViewportChanged(): void {
  if (activeTarget) {
    positionTooltip(activeTarget);
  }
}

export function refreshUnifiedTooltips(root: ParentNode = document): void {
  if (typeof document === "undefined") return;
  convertNativeTitles(root);
}

export function initUnifiedTooltips(): void {
  if (initialized || typeof document === "undefined") return;
  initialized = true;

  ensureTooltipElement();
  refreshUnifiedTooltips(document);

  document.addEventListener("mouseover", onMouseOver, true);
  document.addEventListener("mouseout", onMouseOut, true);
  document.addEventListener("focusin", onFocusIn, true);
  document.addEventListener("focusout", onFocusOut, true);
  window.addEventListener("scroll", onViewportChanged, true);
  window.addEventListener("resize", onViewportChanged, true);

  if (!document.body) return;

  observer = new MutationObserver((mutations) => {
    mutations.forEach((mutation) => {
      if (mutation.type === "attributes" && mutation.target instanceof HTMLElement) {
        convertTitleElement(mutation.target);
        return;
      }

      mutation.addedNodes.forEach((node) => {
        if (!(node instanceof HTMLElement || node instanceof DocumentFragment)) return;
        convertNativeTitles(node);
      });
    });
  });

  observer.observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["title"],
  });
}
