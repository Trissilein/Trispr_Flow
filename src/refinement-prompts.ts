import type {
  PromptPresetOverrides,
  RefinementPromptPreset,
  UserRefinementPromptPreset,
} from "./types";

export const DEFAULT_REFINEMENT_PROMPT_PRESET: RefinementPromptPreset = "wording";
export const NEW_REFINEMENT_PROMPT_OPTION_ID = "__new_preset__" as const;
export type BuiltInRefinementPromptPreset = Exclude<RefinementPromptPreset, "custom">;
export type RefinementPromptPresetOptionId =
  | RefinementPromptPreset
  | `user:${string}`
  | typeof NEW_REFINEMENT_PROMPT_OPTION_ID;
export type PersistedRefinementPromptPresetOptionId =
  Exclude<RefinementPromptPresetOptionId, typeof NEW_REFINEMENT_PROMPT_OPTION_ID>;

export const BUILT_IN_REFINEMENT_PROMPT_PRESET_OPTIONS: ReadonlyArray<{
  id: BuiltInRefinementPromptPreset;
  label: string;
}> = [
  { id: "wording", label: "Wording (Recommended)" },
  { id: "summary", label: "Summary" },
  { id: "technical_specs", label: "Technical Specs" },
  { id: "action_items", label: "Action Items" },
  { id: "llm_prompt", label: "LLM Prompt Engineer" },
];

export const CUSTOM_REFINEMENT_PROMPT_OPTION: {
  id: "custom";
  label: string;
} = {
  id: "custom",
  label: "Custom Prompt",
};

export const NEW_REFINEMENT_PROMPT_OPTION: {
  id: typeof NEW_REFINEMENT_PROMPT_OPTION_ID;
  label: string;
} = {
  id: NEW_REFINEMENT_PROMPT_OPTION_ID,
  label: "New Preset…",
};

const USER_PRESET_OPTION_PREFIX = "user:";

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
  BuiltInRefinementPromptPreset,
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

function normalizeUserPresetId(rawId: string | null | undefined): string {
  return (rawId || "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function normalizeUserRefinementPromptPresets(
  presets: UserRefinementPromptPreset[] | null | undefined
): UserRefinementPromptPreset[] {
  if (!Array.isArray(presets) || presets.length === 0) return [];
  const out: UserRefinementPromptPreset[] = [];
  const seenIds = new Set<string>();
  for (const preset of presets) {
    if (!preset || typeof preset !== "object") continue;
    const id = normalizeUserPresetId(preset.id);
    const name = String(preset.name || "").trim();
    const prompt = String(preset.prompt || "").trim();
    if (!id || !name || !prompt || seenIds.has(id)) continue;
    seenIds.add(id);
    out.push({ id, name, prompt });
  }
  return out;
}

export function toUserRefinementPromptOptionId(presetId: string): `user:${string}` {
  return `${USER_PRESET_OPTION_PREFIX}${presetId}`;
}

export function parseUserRefinementPromptOptionId(
  optionId: string | null | undefined
): string | null {
  const normalized = String(optionId || "").trim();
  if (!normalized.startsWith(USER_PRESET_OPTION_PREFIX)) return null;
  const id = normalizeUserPresetId(normalized.slice(USER_PRESET_OPTION_PREFIX.length));
  return id || null;
}

export function findUserRefinementPromptPresetByOptionId(
  presets: UserRefinementPromptPreset[] | null | undefined,
  optionId: string | null | undefined
): UserRefinementPromptPreset | null {
  const presetId = parseUserRefinementPromptOptionId(optionId);
  if (!presetId) return null;
  const normalized = normalizeUserRefinementPromptPresets(presets);
  return normalized.find((preset) => preset.id === presetId) || null;
}

export function normalizeActiveRefinementPromptPresetId(
  activePresetId: string | null | undefined,
  promptProfile: string | null | undefined,
  presets: UserRefinementPromptPreset[] | null | undefined
): RefinementPromptPresetOptionId {
  const normalizedActive = String(activePresetId || "").trim();
  if (normalizedActive === NEW_REFINEMENT_PROMPT_OPTION_ID) {
    return NEW_REFINEMENT_PROMPT_OPTION_ID;
  }
  const normalizedPresets = normalizeUserRefinementPromptPresets(presets);
  if (normalizeRefinementPromptPreset(normalizedActive) === normalizedActive) {
    return normalizedActive as RefinementPromptPreset;
  }

  const userPreset = findUserRefinementPromptPresetByOptionId(normalizedPresets, normalizedActive);
  if (userPreset) {
    return toUserRefinementPromptOptionId(userPreset.id);
  }

  const normalizedProfile = normalizeRefinementPromptPreset(promptProfile);
  return normalizedProfile;
}

export function normalizePersistedRefinementPromptPresetId(
  activePresetId: string | null | undefined,
  promptProfile: string | null | undefined,
  presets: UserRefinementPromptPreset[] | null | undefined
): PersistedRefinementPromptPresetOptionId {
  const normalizedActive = normalizeActiveRefinementPromptPresetId(
    activePresetId,
    promptProfile,
    presets
  );
  if (normalizedActive === NEW_REFINEMENT_PROMPT_OPTION_ID) {
    return normalizeRefinementPromptPreset(promptProfile);
  }
  return normalizedActive;
}

/** Detects the model family from an Ollama model tag for prompt adaptation. */
function detectModelFamily(model: string | null | undefined): "gemma" | "qwen" | "generic" {
  const m = (model || "").toLowerCase().trim();
  if (m.startsWith("gemma")) return "gemma";
  if (m.startsWith("qwen")) return "qwen";
  return "generic";
}

/**
 * Model-family-specific prompt additions appended after the base prompt.
 * Gemma 4 tends to silently remove anglicisms and foreign-language terms unless
 * explicitly told to preserve them.
 */
const MODEL_FAMILY_PROMPT_ADDONS: Partial<Record<
  ReturnType<typeof detectModelFamily>,
  Partial<Record<BuiltInRefinementPromptPreset, string>>
>> = {
  gemma: {
    wording:
      "Do not remove, replace, or translate anglicisms, brand names, technical jargon, or foreign-language terms intentionally used by the speaker. Preserve them exactly as spoken, even if they appear in a different language than the surrounding text.",
  },
};

export function getFactoryPresetPrompt(
  preset: BuiltInRefinementPromptPreset,
  locale: "en" | "de"
): string {
  return PRESET_PROMPTS[preset][locale];
}

export function hasPresetOverride(
  overrides: PromptPresetOverrides | null | undefined,
  preset: RefinementPromptPreset
): boolean {
  if (preset === "custom") return false;
  if (!overrides) return false;
  const value = overrides[preset as BuiltInRefinementPromptPreset];
  return typeof value === "string" && value.trim().length > 0;
}

export function setPresetOverride(
  overrides: PromptPresetOverrides,
  preset: BuiltInRefinementPromptPreset,
  value: string
): PromptPresetOverrides {
  const trimmed = (value || "").trim();
  if (!trimmed) {
    delete overrides[preset];
    return overrides;
  }
  overrides[preset] = trimmed;
  return overrides;
}

export function removePresetOverride(
  overrides: PromptPresetOverrides,
  preset: BuiltInRefinementPromptPreset
): PromptPresetOverrides {
  delete overrides[preset];
  return overrides;
}

export function resolveRefinementPresetPrompt(
  preset: RefinementPromptPreset,
  language: string | null | undefined,
  overrides?: PromptPresetOverrides | null
): string | null {
  if (preset === "custom") return null;
  const override = overrides?.[preset as BuiltInRefinementPromptPreset];
  if (typeof override === "string" && override.trim().length > 0) {
    return override;
  }
  const locale = isGermanLanguage(language) ? "de" : "en";
  return PRESET_PROMPTS[preset][locale];
}

export function resolveEffectiveRefinementPrompt(
  preset: RefinementPromptPreset,
  language: string | null | undefined,
  customPrompt: string | null | undefined,
  preserveSourceLanguage: boolean,
  model?: string | null,
  overrides?: PromptPresetOverrides | null
): string {
  if (preset === "custom") {
    const normalized = (customPrompt || "").trim();
    if (normalized.length > 0) {
      return normalized;
    }
    return resolveRefinementPresetPrompt(DEFAULT_REFINEMENT_PROMPT_PRESET, language, overrides) || "";
  }

  const base = resolveRefinementPresetPrompt(preset, language, overrides) || "";

  // llm_prompt always outputs in English, so skip language guard and model addons
  if (preset === "llm_prompt") {
    return base;
  }

  const withLanguageGuard = withLanguageLockGuard(base, language, preserveSourceLanguage);

  // Append model-family-specific additions (e.g. Gemma anglicism preservation)
  const family = detectModelFamily(model);
  const addon = MODEL_FAMILY_PROMPT_ADDONS[family]?.[preset as BuiltInRefinementPromptPreset];
  if (addon) {
    return `${withLanguageGuard}\n\n${addon}`;
  }

  return withLanguageGuard;
}
