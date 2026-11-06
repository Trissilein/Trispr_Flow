// Vocabulary settings rendering (R3 slice 1).
//
// Renders the custom vocabulary panel and learned/observed term chips.
//
// Exports:
//   Primary   - renderVocabulary()          called by renderSettings() in index.ts
//   Secondary - renderLearnedVocabChips()   called by vocab-auto-learn.ts (dynamic import)
//               addVocabRow()               called by ai-refinement.wire.ts
//
// Per Decision 6 (settings-decomposition.md): all other functions are private.

import * as dom from "../dom-refs";
import { settings } from "../state";
import {
    dismissLearnedPair,
    dismissLearnedTerm,
    dismissPendingSubstitution,
    PROMOTION_THRESHOLD,
} from "../vocab-auto-learn";
import { persistSettings } from "../settings-persist";

// Custom vocabulary panel

/**
 * Render both the learned-vocabulary chip list and the pending-corrections
 * list. Learned corrections show as "heard → intended" chips, terms without a
 * recorded correction as plain chips; every chip has a × that removes the term
 * together with the rules built from it. Pending chips show progress toward
 * the promotion threshold and have a × as well.
 */
export function renderLearnedVocabChips(): void {
    renderLearnedVocabChipsInternal();
    renderObservingCandidateChips();
}

function renderLearnedVocabChipsInternal(): void {
    const container = dom.vocabTermsList;
    if (!container) return;
    const subs = Array.isArray(settings?.edit_substitutions) ? settings!.edit_substitutions : [];
    const pairs = subs.filter((s) => s.promoted).sort((a, b) => b.last_seen_ms - a.last_seen_ms);
    const pairTargets = new Set(pairs.map((p) => p.to.toLowerCase()));
    const terms = (Array.isArray(settings?.vocab_terms) ? [...settings!.vocab_terms] : [])
        .filter((t) => !pairTargets.has(t.toLowerCase()))
        .sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));

    const pending = subs.filter((s) => !s.promoted).length;

    updateVocabCountBadge(pairs.length + terms.length, pending);
    container.innerHTML = "";

    for (const pair of pairs) {
        container.appendChild(buildLearnedPairChip(pair));
    }
    for (const term of terms) {
        container.appendChild(buildLearnedChip(term));
    }
}

function renderObservingCandidateChips(): void {
    const container = dom.vocabObservingList;
    if (!container) return;
    const subs = Array.isArray(settings?.edit_substitutions)
        ? settings!.edit_substitutions.filter((s) => !s.promoted)
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

function buildChip(
    labelText: string,
    title: string,
    dismissTitle: string,
    onDismiss: () => Promise<void>,
    extraClass = "",
): HTMLSpanElement {
    const chip = document.createElement("span");
    chip.className = `vocab-term-chip${extraClass ? ` ${extraClass}` : ""}`;
    chip.setAttribute("role", "listitem");
    chip.setAttribute("title", title);

    const label = document.createElement("span");
    label.textContent = labelText;
    chip.appendChild(label);

    const dismiss = document.createElement("button");
    dismiss.type = "button";
    dismiss.className = "vocab-term-chip-dismiss";
    dismiss.title = dismissTitle;
    dismiss.setAttribute("aria-label", `${dismissTitle}: ${labelText}`);
    dismiss.textContent = "×";
    dismiss.addEventListener("click", async (ev) => {
        ev.stopPropagation();
        await onDismiss();
        renderLearnedVocabChips();
    });
    chip.appendChild(dismiss);

    return chip;
}

function buildLearnedChip(term: string): HTMLSpanElement {
    const chip = buildChip(
        term,
        "Learned term — sent to Whisper and the refinement model",
        "Remove this term and the rules built from it",
        () => dismissLearnedTerm(term),
    );
    chip.dataset.term = term;
    return chip;
}

function buildLearnedPairChip(sub: { from: string; to: string; count: number }): HTMLSpanElement {
    const chip = buildChip(
        `${sub.from} → ${sub.to}`,
        `Learned from ${sub.count} corrections — heard "${sub.from}", meant "${sub.to}"`,
        "Remove this correction and its rules",
        () => dismissLearnedPair(sub.from, sub.to),
    );
    chip.dataset.term = sub.to;
    return chip;
}

function buildPendingSubstitutionChip(sub: {
    from: string;
    to: string;
    count: number;
}): HTMLSpanElement {
    const chip = buildChip(
        `${sub.from} → ${sub.to}`,
        `Corrected ${sub.count}× — learned after ${PROMOTION_THRESHOLD} separate corrections`,
        "Discard this pending correction",
        () => dismissPendingSubstitution(sub.from, sub.to),
        "observing",
    );

    const progress = document.createElement("span");
    progress.className = "vocab-chip-progress";
    progress.textContent = `×${sub.count}/${PROMOTION_THRESHOLD}`;
    chip.insertBefore(progress, chip.querySelector(".vocab-term-chip-dismiss"));

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

    // Don't rebuild while the user is editing inside the rows container.
    // Every input "change" persists settings, which triggers a backend
    // settings-changed event and a full settings re-render — without this
    // guard that roundtrip wipes a half-filled row the moment focus moves
    // from the first input to the second.
    if (
        document.activeElement instanceof HTMLElement &&
        dom.postprocVocabRows.contains(document.activeElement)
    ) {
        return;
    }

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
