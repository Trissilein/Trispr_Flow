import * as dom from "../dom-refs";
import { settings } from "../state";
import { renderLearnedVocabChips, renderVocabulary } from "./vocabulary.settings";

function derivedPostprocLanguageLabel(postprocLanguage: "en" | "de" | "multi"): string {
  if (postprocLanguage === "en") {
    return "Derived: English rules (ASR language pinned to English).";
  }
  if (postprocLanguage === "de") {
    return "Derived: German rules (ASR language pinned to German).";
  }
  return "Derived: Multilingual rules (ASR auto-detect or non EN/DE language).";
}

export function renderPostProcessingSettings(): void {
  if (!settings) return;
  if (dom.postprocEnabled) {
    dom.postprocEnabled.checked = settings.postproc_enabled;
  }
  if (dom.postprocSettings) {
    dom.postprocSettings.style.display = settings.postproc_enabled ? "grid" : "none";
  }
  if (dom.postprocLanguageDerived) {
    dom.postprocLanguageDerived.textContent = derivedPostprocLanguageLabel(
      settings.postproc_language as "en" | "de" | "multi"
    );
  }
  if (dom.postprocPunctuation) {
    dom.postprocPunctuation.checked = settings.postproc_punctuation_enabled;
  }
  if (dom.postprocCapitalization) {
    dom.postprocCapitalization.checked = settings.postproc_capitalization_enabled;
  }
  if (dom.postprocNumbers) {
    dom.postprocNumbers.checked = settings.postproc_numbers_enabled;
  }
  if (dom.postprocCustomVocabEnabled) {
    dom.postprocCustomVocabEnabled.checked = settings.postproc_custom_vocab_enabled;
  }
  if (dom.postprocCustomVocabConfig) {
    dom.postprocCustomVocabConfig.style.display = settings.postproc_custom_vocab_enabled ? "block" : "none";
  }
  renderVocabulary();
  renderLearnedVocabChips();
}