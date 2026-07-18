export const MAX_PROTECTED_DRAFT_BYTES = 1_000;

const encoder = new TextEncoder();

export function utf8Length(value: string): number {
  return encoder.encode(value).length;
}

export function boundedProtectedDraft(value: string): string {
  const sanitized = value.replace(/[\u0000\u007f]/gu, "");
  if (utf8Length(sanitized) <= MAX_PROTECTED_DRAFT_BYTES) return sanitized;
  let bounded = "";
  let bytes = 0;
  for (const character of sanitized) {
    const next = utf8Length(character);
    if (bytes + next > MAX_PROTECTED_DRAFT_BYTES) break;
    bounded += character;
    bytes += next;
  }
  return bounded;
}
