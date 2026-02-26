import type { RefinementPromptPreset } from "./types";

export const DEFAULT_REFINEMENT_PROMPT_PRESET: RefinementPromptPreset = "wording";

function isGermanLanguage(language: string | null | undefined): boolean {
  const normalized = (language || "").trim().toLowerCase();
  return normalized === "de" || normalized === "german" || normalized.startsWith("de-");
}

const PRESET_PROMPTS: Record<
  Exclude<RefinementPromptPreset, "custom">,
  { en: string; de: string }
> = {
  wording: {
    en:
      "You are editing a speech-to-text transcript. Improve grammar, punctuation, capitalization, and wording clarity while preserving meaning and tone. Do not summarize. Do not add facts. Do not remove technical details. Keep names, numbers, units, and file paths unchanged unless clearly wrong. Return only the final corrected transcript.",
    de:
      "Du bearbeitest ein Speech-to-Text-Transkript. Verbessere Grammatik, Zeichensetzung, Groß- und Kleinschreibung sowie Formulierungen, ohne Bedeutung oder Ton zu veraendern. Nicht zusammenfassen. Keine Fakten erfinden. Keine technischen Details entfernen. Namen, Zahlen, Einheiten und Dateipfade unveraendert lassen, wenn sie nicht eindeutig falsch sind. Gib nur den finalen korrigierten Text zurueck.",
  },
  summary: {
    en:
      "Summarize this transcript into 3 to 6 concise bullet points. Preserve key facts, numbers, names, and decisions. Do not invent information. If something is uncertain, state it cautiously. Return only the bullet list.",
    de:
      "Fasse dieses Transkript in 3 bis 6 praegnanten Stichpunkten zusammen. Behalte wichtige Fakten, Zahlen, Namen und Entscheidungen bei. Keine Informationen erfinden. Unsichere Inhalte vorsichtig formulieren. Gib nur die Stichpunktliste zurueck.",
  },
  technical_specs: {
    en:
      "Rewrite this transcript in technical specification style. Keep exact numbers, units, versions, APIs, constraints, and file paths. Structure output with short sections: Goal, Inputs, Outputs, Constraints, Open Questions. Do not invent missing values. Return only the structured result.",
    de:
      "Formuliere dieses Transkript als technische Spezifikation um. Behalte exakte Zahlen, Einheiten, Versionen, APIs, Rahmenbedingungen und Dateipfade. Strukturiere die Ausgabe mit kurzen Abschnitten: Ziel, Eingaben, Ausgaben, Constraints, Offene Fragen. Keine fehlenden Werte erfinden. Gib nur das strukturierte Ergebnis zurueck.",
  },
  action_items: {
    en:
      "Convert this transcript into actionable tasks. Use bullets with format: [Action] [Owner?] [Due?] [Notes]. Preserve technical wording and constraints. If owner or due date is missing, mark it as unknown. Return only the action list.",
    de:
      "Wandle dieses Transkript in konkrete Aufgaben um. Nutze Stichpunkte im Format: [Aktion] [Owner?] [Faellig?] [Hinweise]. Behalte technische Begriffe und Rahmenbedingungen bei. Wenn Owner oder Datum fehlen, mit unknown markieren. Gib nur die Aufgabenliste zurueck.",
  },
};

export function normalizeRefinementPromptPreset(
  preset: string | null | undefined
): RefinementPromptPreset {
  if (preset === "summary") return "summary";
  if (preset === "technical_specs") return "technical_specs";
  if (preset === "action_items") return "action_items";
  if (preset === "custom") return "custom";
  return "wording";
}

export function resolveRefinementPresetPrompt(
  preset: RefinementPromptPreset,
  language: string | null | undefined
): string | null {
  if (preset === "custom") return null;
  const locale = isGermanLanguage(language) ? "de" : "en";
  return PRESET_PROMPTS[preset][locale];
}

export function resolveEffectiveRefinementPrompt(
  preset: RefinementPromptPreset,
  language: string | null | undefined,
  customPrompt: string | null | undefined
): string {
  if (preset === "custom") {
    const normalized = (customPrompt || "").trim();
    if (normalized.length > 0) return normalized;
    return resolveRefinementPresetPrompt(DEFAULT_REFINEMENT_PROMPT_PRESET, language) || "";
  }
  return resolveRefinementPresetPrompt(preset, language) || "";
}

