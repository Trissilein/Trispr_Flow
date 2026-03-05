export function normalizeModelTag(name: string | null | undefined): string {
  return (name ?? "").trim().toLowerCase();
}

export function isExactModelTagMatch(
  left: string | null | undefined,
  right: string | null | undefined
): boolean {
  const normalizedLeft = normalizeModelTag(left);
  if (!normalizedLeft) return false;
  return normalizedLeft === normalizeModelTag(right);
}
