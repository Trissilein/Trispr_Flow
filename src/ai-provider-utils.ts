// Shared AI provider normalisation helpers.
// This module imports only from "./types" so it can be safely imported by
// both settings.ts and event-listeners.ts without creating a circular dependency.

import type {
  AIFallbackProvider,
  CloudAIFallbackProvider,
  AIExecutionMode,
  AIProviderAuthMethodPreference,
} from "./types";

export const AI_FALLBACK_PROVIDER_IDS: AIFallbackProvider[] = ["claude", "openai", "gemini", "ollama"];
export const CLOUD_PROVIDER_IDS: CloudAIFallbackProvider[] = ["claude", "openai", "gemini"];
export const CLOUD_PROVIDER_LABELS: Record<CloudAIFallbackProvider, string> = {
  claude: "Claude (Anthropic)",
  openai: "OpenAI",
  gemini: "Gemini (Google)",
};

export function isCloudProvider(provider?: string | null): provider is CloudAIFallbackProvider {
  if (!provider) return false;
  return CLOUD_PROVIDER_IDS.includes(provider as CloudAIFallbackProvider);
}

export function normalizeCloudProvider(provider?: string | null): CloudAIFallbackProvider | null {
  if (!provider) return null;
  const normalized = provider.trim().toLowerCase();
  return isCloudProvider(normalized) ? (normalized as CloudAIFallbackProvider) : null;
}

export function normalizeExecutionMode(mode?: string | null): AIExecutionMode {
  return mode === "online_fallback" ? "online_fallback" : "local_primary";
}

export function normalizeAuthMethodPreference(
  method?: string | null
): AIProviderAuthMethodPreference {
  return method === "oauth" ? "oauth" : "api_key";
}

export function isVerifiedAuthStatus(status?: string | null): boolean {
  return status === "verified_api_key" || status === "verified_oauth";
}

export function normalizeAIFallbackProvider(provider?: string): AIFallbackProvider {
  if (provider && AI_FALLBACK_PROVIDER_IDS.includes(provider as AIFallbackProvider)) {
    return provider as AIFallbackProvider;
  }
  return "ollama";
}
