function normalizeLanguageModeValue(languageMode: string | null | undefined): string {
  const normalized = (languageMode || "auto").trim().toLowerCase();
  if (!normalized) return "auto";
  return normalized;
}

export function resolveEffectiveAsrLanguageHint(
  languageMode: string | null | undefined,
  languagePinned: boolean | null | undefined
): string {
  const normalized = normalizeLanguageModeValue(languageMode);
  return languagePinned ? normalized : "auto";
}

export function derivePostprocLanguageFromAsr(
  languageMode: string | null | undefined,
  languagePinned: boolean | null | undefined
): "en" | "de" | "multi" {
  if (!languagePinned) return "multi";
  const normalized = normalizeLanguageModeValue(languageMode);
  if (normalized === "en") return "en";
  if (normalized === "de") return "de";
  return "multi";
}