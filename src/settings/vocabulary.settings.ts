import * as dom from "../dom-refs";
import { settings } from "../state";
import { dismissLearnedTerm, PROMOTION_THRESHOLD } from "../vocab-auto-learn";
import { persistSettings } from "../settings-persist";

// Custom vocabulary panel

/**
 * Render both the learned-vocabulary chip list and the observing-candidates
 * list. Learned chips that originated from a multi-variant cluster expose an
 * expand affordance revealing the individual variant spellings (each with its
 * own × to split it off). Observing chips show progress toward the promotion
 * threshold.
 */
export function renderLearnedVocabChips(): void {
  renderLearnedVocabChipsInternal();
  renderObservingCandidateChips();
}

function renderLearnedVocabChipsInternal(): void {
  const container = dom.vocabTermsList;
  if (!container) return;
  const terms = Array.isArray(settings?.vocab_terms) ? [...settings!.vocab_terms] : [];
  terms.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));

  const pending = Array.isArray(settings?.edit_substitutions)
    ? settings!.edit_substitutions.length
    : 0;

  updateVocabCountBadge(terms.length, pending);
  container.innerHTML = "";

  for (const term of terms) {
    const chip = buildLearnedChip(term);
    container.appendChild(chip);
  }
}

function renderObservingCandidateChips(): void {
  const container = dom.vocabObservingList;
  if (!container) return;
  const subs = Array.isArray(settings?.edit_substitutions)
    ? [...settings!.edit_substitutions]
    : [];
  // Newest-first so recently-corrected pairs surface at the top.
  subs.sort((a, b) => b.last_seen_ms - a.last_seen_ms);

  container.innerHTML = "";
  for (const sub of subs) {
    container.appendChild(buildPendingSubstitutionChip(sub));
  }
}

function updateVocabCountBadge(learned: number, observed: number): void {
  if (!dom.vocabTermsCount) return;
  if (learned === 0 && observed === 0) {
    dom.vocabTermsCount.textContent = "";
    return;
  }
  const parts: string[] = [];
  if (learned > 0) parts.push(`${learned} learned`);
  if (observed > 0) parts.push(`${observed} observed`);
  dom.vocabTermsCount.textContent = parts.join(" · ");
}

function buildLearnedChip(term: string): DocumentFragment {
  const frag = document.createDocumentFragment();

  const chip = document.createElement("span");
  chip.className = "vocab-term-chip";
  chip.dataset.term = term;
  chip.setAttribute("role", "listitem");

  const label = document.createElement("span");
  label.textContent = term;
  chip.appendChild(label);

  const dismiss = document.createElement("button");
  dismiss.type = "button";
  dismiss.className = "vocab-term-chip-dismiss";
  dismiss.title = "Remove this term";
  dismiss.setAttribute("aria-label", `Remove learned term ${term}`);
  dismiss.textContent = "×";
  dismiss.addEventListener("click", async (ev) => {
    ev.stopPropagation();
    await dismissLearnedTerm(term);
    renderLearnedVocabChips();
  });
  chip.appendChild(dismiss);
  frag.appendChild(chip);

  return frag;
}

function buildPendingSubstitutionChip(sub: {
  from: string;
  to: string;
  count: number;
}): HTMLSpanElement {
  const chip = document.createElement("span");
  chip.className = "vocab-term-chip observing";
  chip.setAttribute("role", "listitem");
  chip.setAttribute("title", `Corrected ${sub.count}× — will auto-learn after ${PROMOTION_THRESHOLD} corrections`);

  const label = document.createElement("span");
  label.textContent = `${sub.from} → ${sub.to}`;
  chip.appendChild(label);

  const progress = document.createElement("span");
  progress.className = "vocab-chip-progress";
  progress.textContent = `×${sub.count}/${PROMOTION_THRESHOLD}`;
  chip.appendChild(progress);

  return chip;
}

export function addVocabRow(original: string, replacement: string) {
  if (!dom.postprocVocabRows) return;

  const row = document.createElement("div");
  row.className = "vocab-row";

  const originalInput = document.createElement("input");
  originalInput.type = "text";
  originalInput.value = original;
  originalInput.placeholder = "api";
  originalInput.className = "vocab-input";
  originalInput.title = "Word or phrase to find in transcripts";

  const replacementInput = document.createElement("input");
  replacementInput.type = "text";
  replacementInput.value = replacement;
  replacementInput.placeholder = "API";
  replacementInput.className = "vocab-input";
  replacementInput.title = "Text to substitute for the matched word or phrase";

  const removeBtn = document.createElement("button");
  removeBtn.textContent = "×";
  removeBtn.className = "vocab-remove";
  removeBtn.title = "Remove entry";

  // Update settings when inputs change
  const updateVocab = async () => {
    if (!settings) return;
    const rows = dom.postprocVocabRows?.querySelectorAll(".vocab-row");
    const vocab: Record<string, string> = {};
    rows?.forEach((r) => {
      const inputs = r.querySelectorAll("input");
      const orig = inputs[0]?.value.trim();
      const repl = inputs[1]?.value.trim();
      if (orig && repl) {
        vocab[orig] = repl;
      }
    });
    settings.postproc_custom_vocab = vocab;
    await persistSettings();
  };

  originalInput.addEventListener("change", updateVocab);
  replacementInput.addEventListener("change", updateVocab);

  removeBtn.addEventListener("click", async () => {
    row.remove();
    await updateVocab();
  });

  const arrowSpan = document.createElement("span");
  arrowSpan.className = "vocab-row-arrow";
  arrowSpan.textContent = "→";
  arrowSpan.setAttribute("aria-hidden", "true");

  row.appendChild(originalInput);
  row.appendChild(arrowSpan);
  row.appendChild(replacementInput);
  row.appendChild(removeBtn);
  dom.postprocVocabRows.appendChild(row);
}

export function renderVocabulary() {
  if (!settings || !dom.postprocVocabRows) return;

  // Clear existing rows
  dom.postprocVocabRows.innerHTML = "";

  // Check if vocabulary is empty
  const vocabEntries = Object.entries(settings.postproc_custom_vocab || {});

  if (vocabEntries.length === 0) {
    // Show empty state
    const emptyState = document.createElement("div");
    emptyState.className = "vocab-empty-state";
    emptyState.innerHTML = `
      <div class="vocab-empty-icon">◻</div>
      <div class="vocab-empty-text">No vocabulary entries yet</div>
      <div class="vocab-empty-hint">Click "Add Entry" to define custom word replacements</div>
    `;
    dom.postprocVocabRows.appendChild(emptyState);
  } else {
    // Add rows from settings
    for (const [original, replacement] of vocabEntries) {
      addVocabRow(original, replacement);
    }
  }
}
