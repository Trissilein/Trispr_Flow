/**
 * Vocabulary suggestion banner and review dialog.
 *
 * Shows a banner in the history panel when new vocabulary corrections have
 * been observed enough times to become suggestions. The user can open a
 * review dialog to confirm or dismiss each suggestion individually.
 */

import * as dom from "./dom-refs";
import { escapeHtml } from "./utils";
import {
  getSuggestedCandidates,
  getAllCandidates,
  getPendingSuggestionCount,
  removeCandidateByPair,
  clearAllCandidates,
  VOCAB_SUGGESTION_THRESHOLD,
} from "./vocab-candidates";
import { settings } from "./state";
import { addVocabEntryFromSuggestion } from "./event-listeners";
import type { VocabCandidate } from "./types";

// ---------------------------------------------------------------------------
// Banner
// ---------------------------------------------------------------------------

/** Re-render the suggestion banner based on current pending candidate count. */
export function renderVocabSuggestionBanner(): void {
  const banner = dom.vocabSuggestionBanner;
  if (!banner) return;

  const count = getPendingSuggestionCount();
  if (count === 0) {
    banner.style.display = "none";
    banner.innerHTML = "";
    return;
  }

  const label = count === 1
    ? "1 recurring correction detected"
    : `${count} recurring corrections detected`;

  banner.style.display = "flex";
  banner.innerHTML = `
    <span class="vocab-banner-icon">💡</span>
    <span class="vocab-banner-text">${label}. Add to vocabulary?</span>
    <button class="vocab-banner-btn" id="vocab-banner-review-btn" title="Review and confirm vocabulary suggestions">Review</button>
    <button class="vocab-banner-dismiss" id="vocab-banner-dismiss-btn" title="Dismiss all suggestions">×</button>
  `;

  document.getElementById("vocab-banner-review-btn")?.addEventListener("click", () => {
    openVocabReviewDialog();
  });

  document.getElementById("vocab-banner-dismiss-btn")?.addEventListener("click", () => {
    clearAllCandidates();
    renderVocabSuggestionBanner();
  });
}

// ---------------------------------------------------------------------------
// Review dialog
// ---------------------------------------------------------------------------

let dialogEl: HTMLDialogElement | null = null;

function buildDialog(): HTMLDialogElement {
  const dialog = document.createElement("dialog");
  dialog.id = "vocab-review-dialog";
  dialog.className = "vocab-review-dialog";
  dialog.setAttribute("aria-label", "Review vocabulary suggestions");
  document.body.appendChild(dialog);
  return dialog;
}

function getDialog(): HTMLDialogElement {
  if (!dialogEl || !document.body.contains(dialogEl)) {
    dialogEl = buildDialog();
  }
  return dialogEl;
}

function renderDialogContent(
  dialog: HTMLDialogElement,
  candidates: VocabCandidate[]
): void {
  if (candidates.length === 0) {
    dialog.close();
    renderVocabSuggestionBanner();
    return;
  }

  dialog.innerHTML = `
    <div class="vocab-review-header">
      <h3 class="vocab-review-title">Vocabulary Suggestions</h3>
      <p class="vocab-review-subtitle">These corrections were detected ${candidates[0]?.count ?? 3}+ times. Confirm to add them to your custom vocabulary.</p>
    </div>
    <div class="vocab-review-list" id="vocab-review-list"></div>
    <div class="vocab-review-footer">
      <button class="btn-secondary vocab-review-close" id="vocab-review-close-btn">Close</button>
    </div>
  `;

  document.getElementById("vocab-review-close-btn")?.addEventListener("click", () => {
    dialog.close();
    renderVocabSuggestionBanner();
  });

  dialog.addEventListener("close", () => {
    renderVocabSuggestionBanner();
  }, { once: true });

  const list = document.getElementById("vocab-review-list");
  if (!list) return;

  for (const candidate of candidates) {
    const row = document.createElement("div");
    row.className = "vocab-review-row";
    row.dataset.from = candidate.from;
    row.dataset.to = candidate.to;

    row.innerHTML = `
      <span class="vocab-review-pair">
        <span class="vocab-review-from">${escapeHtml(candidate.from)}</span>
        <span class="vocab-review-arrow">→</span>
        <span class="vocab-review-to">${escapeHtml(candidate.to)}</span>
        <span class="vocab-review-count" title="Observed ${candidate.count} times">${candidate.count}×</span>
      </span>
      <span class="vocab-review-actions">
        <button class="vocab-review-confirm" title="Add to vocabulary">Add</button>
        <button class="vocab-review-dismiss" title="Dismiss suggestion">Dismiss</button>
      </span>
    `;

    row.querySelector(".vocab-review-confirm")?.addEventListener("click", async () => {
      await addVocabEntryFromSuggestion(candidate.from, candidate.to);
      removeCandidateByPair(candidate.from, candidate.to);
      row.remove();
      const remaining = getSuggestedCandidates();
      if (remaining.length === 0) {
        dialog.close();
        renderVocabSuggestionBanner();
      }
    });

    row.querySelector(".vocab-review-dismiss")?.addEventListener("click", () => {
      removeCandidateByPair(candidate.from, candidate.to);
      row.remove();
      const remaining = getSuggestedCandidates();
      if (remaining.length === 0) {
        dialog.close();
        renderVocabSuggestionBanner();
      }
    });

    list.appendChild(row);
  }
}

export function openVocabReviewDialog(): void {
  const candidates = getSuggestedCandidates();
  if (candidates.length === 0) return;

  const dialog = getDialog();
  renderDialogContent(dialog, candidates);
  if (!dialog.open) {
    dialog.showModal();
  }
}

/** Call on app startup to restore banner state from persisted candidates. */
export function initVocabSuggestionBanner(): void {
  renderVocabSuggestionBanner();
  renderVocabCandidatesStatus();
}

/**
 * Render the candidate status line inside the Vocabulary Learning settings section.
 * Shows how many corrections are tracked and how many are ready to suggest.
 */
export function renderVocabCandidatesStatus(): void {
  const el = dom.vocabCandidatesStatus;
  if (!el) return;
  const all = getAllCandidates();
  const threshold = settings?.vocab_suggestion_threshold ?? VOCAB_SUGGESTION_THRESHOLD;
  const ready = all.filter((c) => c.count >= threshold);
  if (all.length === 0) {
    el.textContent = "No corrections tracked yet.";
  } else {
    el.textContent = `${all.length} tracked · ${ready.length} ready to suggest`;
  }
}
