export type Visibility = "basic" | "expert";

export interface SettingsVisibilityDefinition {
  key: string;
  label: string;
  group: string;
  defaultVisibility: Visibility;
}

export interface SettingsVisibilityProfile {
  version: 1;
  overrides: Record<string, Visibility>;
  removalCandidates: Record<string, { reason: string }>;
}

export interface VisibilityProfileStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export const SETTINGS_VISIBILITY_STORAGE_KEY = "trispr-settings-visibility-v1";
export const SETTINGS_VISIBILITY_EXPORT_FILENAME = "trispr-settings-visibility.json";
export const MAX_REMOVAL_REASON_LENGTH = 500;

const VISIBILITIES: readonly Visibility[] = ["basic", "expert"];

function isRecord(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function hasOwn(value: object, key: PropertyKey): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function isVisibility(value: unknown): value is Visibility {
  return value === "basic" || value === "expert";
}

function definitionMap(
  definitions: readonly SettingsVisibilityDefinition[],
): Map<string, SettingsVisibilityDefinition> {
  const result = new Map<string, SettingsVisibilityDefinition>();
  for (const definition of definitions) {
    if (
      typeof definition.key !== "string" ||
      definition.key.trim() === "" ||
      typeof definition.label !== "string" ||
      typeof definition.group !== "string" ||
      !isVisibility(definition.defaultVisibility)
    ) {
      continue;
    }
    if (!result.has(definition.key)) result.set(definition.key, definition);
  }
  return result;
}

function defaultStorage(): VisibilityProfileStorage | null {
  try {
    return typeof globalThis.localStorage === "undefined" ? null : globalThis.localStorage;
  } catch {
    return null;
  }
}

function requiredStorage(storage?: VisibilityProfileStorage): VisibilityProfileStorage {
  const resolved = storage ?? defaultStorage();
  if (!resolved) throw new Error("Settings visibility storage is unavailable.");
  return resolved;
}

function cloneProfile(profile: SettingsVisibilityProfile): SettingsVisibilityProfile {
  return {
    version: 1,
    overrides: Object.fromEntries(Object.entries(profile.overrides)),
    removalCandidates: Object.fromEntries(
      Object.entries(profile.removalCandidates).map(([key, candidate]) => [key, { reason: candidate.reason }]),
    ),
  };
}

function sanitizeProfile(
  value: unknown,
  definitions: readonly SettingsVisibilityDefinition[],
): SettingsVisibilityProfile {
  void definitions;
  const result = emptyVisibilityProfile();
  if (!isRecord(value) || value.version !== 1) return result;

  const overrides = value.overrides;
  if (isRecord(overrides)) {
    for (const key of Object.keys(overrides)) {
      // Keep structurally valid unknown keys. Dynamic settings can be absent
      // during one render and must not be deleted by a later save.
      if (isVisibility(overrides[key])) {
        result.overrides[key] = overrides[key];
      }
    }
  }

  const removalCandidates = value.removalCandidates;
  if (isRecord(removalCandidates)) {
    for (const key of Object.keys(removalCandidates)) {
      const candidate = removalCandidates[key];
      if (!isRecord(candidate) || typeof candidate.reason !== "string") continue;
      const reason = candidate.reason.trim();
      if (reason.length === 0) continue;
      result.removalCandidates[key] = {
        reason: reason.slice(0, MAX_REMOVAL_REASON_LENGTH),
      };
    }
  }

  return result;
}

export function emptyVisibilityProfile(): SettingsVisibilityProfile {
  return {
    version: 1,
    overrides: Object.create(null) as Record<string, Visibility>,
    removalCandidates: Object.create(null) as Record<string, { reason: string }>,
  };
}

export function readVisibilityProfile(
  definitions: readonly SettingsVisibilityDefinition[],
  storage?: VisibilityProfileStorage,
): SettingsVisibilityProfile {
  const resolved = storage ?? defaultStorage();
  if (!resolved) return emptyVisibilityProfile();

  let raw: string | null;
  try {
    raw = resolved.getItem(SETTINGS_VISIBILITY_STORAGE_KEY);
  } catch {
    return emptyVisibilityProfile();
  }
  if (!raw) return emptyVisibilityProfile();

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw) as unknown;
  } catch {
    return emptyVisibilityProfile();
  }
  return sanitizeProfile(parsed, definitions);
}

export function saveVisibilityProfile(
  profile: SettingsVisibilityProfile,
  definitions: readonly SettingsVisibilityDefinition[],
  storage?: VisibilityProfileStorage,
): SettingsVisibilityProfile {
  const sanitized = sanitizeProfile(profile, definitions);
  requiredStorage(storage).setItem(SETTINGS_VISIBILITY_STORAGE_KEY, JSON.stringify(sanitized));
  return cloneProfile(sanitized);
}

export function effectiveVisibility(
  definition: SettingsVisibilityDefinition,
  profile: SettingsVisibilityProfile,
): Visibility {
  const override =
    profile.version === 1 && hasOwn(profile.overrides, definition.key)
      ? profile.overrides[definition.key]
      : undefined;
  return isVisibility(override) ? override : definition.defaultVisibility;
}

/**
 * Export only visibility metadata. DOM values, prompts, secrets, and settings
 * values never enter this payload.
 */
export function exportVisibilityProfile(
  profile: SettingsVisibilityProfile,
  definitions: readonly SettingsVisibilityDefinition[],
): string {
  const sanitized = sanitizeProfile(profile, definitions);
  const defaults = Object.fromEntries(
    Array.from(definitionMap(definitions).entries()).map(([key, definition]) => [
      key,
      definition.defaultVisibility,
    ]),
  ) as Record<string, Visibility>;
  const known = definitionMap(definitions);
  const overrides = Object.fromEntries(
    Object.entries(sanitized.overrides).filter(([key]) => known.has(key)),
  );
  const unresolvedOverrides = Object.fromEntries(
    Object.entries(sanitized.overrides).filter(([key]) => !known.has(key)),
  );
  const removalCandidates = Object.fromEntries(
    Object.entries(sanitized.removalCandidates).filter(([key]) => known.has(key)),
  );
  const unresolvedRemovalCandidates = Object.fromEntries(
    Object.entries(sanitized.removalCandidates).filter(([key]) => !known.has(key)),
  );
  const exported: Record<string, unknown> = {
    version: 1,
    defaults,
    overrides,
    removalCandidates,
  };
  if (Object.keys(unresolvedOverrides).length > 0) {
    exported.unresolvedOverrides = unresolvedOverrides;
  }
  if (Object.keys(unresolvedRemovalCandidates).length > 0) {
    exported.unresolvedRemovalCandidates = unresolvedRemovalCandidates;
  }
  return JSON.stringify(exported, null, 2);
}

export function isVisibilityValue(value: unknown): value is Visibility {
  return VISIBILITIES.includes(value as Visibility);
}
