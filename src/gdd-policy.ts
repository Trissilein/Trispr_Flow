export const ONE_CLICK_ROUTE_CONFIDENCE_THRESHOLD = 0.75;

export function requiresOneClickPublishConfirmation(
  confidence: number | null | undefined,
  threshold = ONE_CLICK_ROUTE_CONFIDENCE_THRESHOLD
): boolean {
  if (!Number.isFinite(confidence)) return true;
  return Number(confidence) < threshold;
}
