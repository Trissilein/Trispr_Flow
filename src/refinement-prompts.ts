import type { RefinementPromptPreset } from "./types";

export const DEFAULT_REFINEMENT_PROMPT_PRESET: RefinementPromptPreset = "wording";

function isGermanLanguage(language: string | null | undefined): boolean {
  const normalized = (language || "").trim().toLowerCase();
  return normalized === "de" || normalized === "german" || normalized.startsWith("de-");
}

function isAutoLanguage(language: string | null | undefined): boolean {
  const normalized = (language || "").trim().toLowerCase();
  return normalized === "" || normalized === "auto";
}

function languageLockInstruction(language: string | null | undefined): string {
  if (isAutoLanguage(language)) {
    return "Detect the input language and keep it unchanged. Do not translate unless explicitly asked to do so. If the input is mixed-language, preserve each segment in its original language.";
  }
  return isGermanLanguage(language)
    ? "Behalte die Ausgabe in derselben Sprache wie die Eingabe. Nicht uebersetzen."
    : "Keep the output in the same language as the input. Do not translate unless explicitly asked to do so.";
}

function withLanguageLockGuard(
  prompt: string,
  language: string | null | undefined,
  preserveSourceLanguage: boolean
): string {
  const normalized = prompt.trim();
  if (!normalized) return normalized;
  if (!preserveSourceLanguage) return normalized;
  return `${normalized}\n\n${languageLockInstruction(language)}`;
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
  llm_prompt: {
    en:
      "You are an expert prompt engineer. Convert the following spoken dictation into a precise, high-quality prompt for a large language model.\n\nA well-structured prompt must include:\n1. A clear role or persona (e.g. \"You are a...\")\n2. An unambiguous task description\n3. Relevant context, background, or constraints\n4. Desired output format (if applicable)\n\nRules:\n- Always write the resulting prompt in English, regardless of the input language.\n- Do not explain, comment, or add preamble.\n- Do not address the speaker or reference this conversation.\n- Return only the final ready-to-use prompt, nothing else.",
    de:
      "Du bist ein erfahrener Prompt-Engineer. Wandle die folgende gesprochene Eingabe in einen praezisen, einsatzbereiten Prompt fuer ein grosses Sprachmodell um.\n\nEin guter Prompt enthaelt:\n1. Eine klare Rolle oder Persona (z.B. \"You are a...\")\n2. Eine eindeutige Aufgabenbeschreibung\n3. Relevanten Kontext, Hintergrund oder Einschraenkungen\n4. Das gewuenschte Ausgabeformat (falls zutreffend)\n\nRegeln:\n- Schreibe den fertigen Prompt immer auf Englisch, unabhaengig von der Eingabesprache.\n- Keine Erklaerungen, Kommentare oder Vorbemerkungen.\n- Den Sprecher nicht adressieren und nicht auf dieses Gespraech verweisen.\n- Gib nur den fertigen Prompt zurueck, nichts weiteres.",
  },
};

export function normalizeRefinementPromptPreset(
  preset: string | null | undefined
): RefinementPromptPreset {
  if (preset === "summary") return "summary";
  if (preset === "technical_specs") return "technical_specs";
  if (preset === "action_items") return "action_items";
  if (preset === "llm_prompt") return "llm_prompt";
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
  customPrompt: string | null | undefined,
  preserveSourceLanguage: boolean
): string {
  if (preset === "custom") {
    const normalized = (customPrompt || "").trim();
    if (normalized.length > 0) {
      return normalized;
    }
    return resolveRefinementPresetPrompt(DEFAULT_REFINEMENT_PROMPT_PRESET, language) || "";
  }

  const base = resolveRefinementPresetPrompt(preset, language) || "";
  // llm_prompt always outputs in English, so skip language guard
  if (preset === "llm_prompt") {
    return base;
  }
  return withLanguageLockGuard(base, language, preserveSourceLanguage);
}
