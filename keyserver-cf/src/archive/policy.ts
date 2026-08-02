/**
 * Portable retained-pool policy. The Durable Object supplies serialization and
 * scheduling; these decisions deliberately do not depend on Cloudflare APIs.
 */
export interface ArchiveEntry {
  id: string;
  objectKey: string;
  receivedAt: number;
  expiresAt: number;
  byteLength: number;
}

function oldestFirst(a: ArchiveEntry, b: ArchiveEntry): number {
  return a.receivedAt - b.receivedAt || a.id.localeCompare(b.id);
}

function assertByteBudget(byteBudget: number): void {
  if (!Number.isSafeInteger(byteBudget) || byteBudget < 0) {
    throw new RangeError("archive byte budget must be a non-negative safe integer");
  }
}

/** Returns the entries to keep after evicting the oldest records first. */
export function enforceArchiveByteBudget(entries: readonly ArchiveEntry[], byteBudget: number): ArchiveEntry[] {
  assertByteBudget(byteBudget);
  let total = 0;
  for (const entry of entries) {
    if (!Number.isSafeInteger(entry.byteLength) || entry.byteLength < 0) {
      throw new RangeError("archive entry byte length must be a non-negative safe integer");
    }
    total += entry.byteLength;
  }

  const evicted = new Set<string>();
  for (const entry of [...entries].sort(oldestFirst)) {
    if (total <= byteBudget) break;
    total -= entry.byteLength;
    evicted.add(entry.id);
  }
  return entries.filter((entry) => !evicted.has(entry.id));
}

/** Returns precisely the entries whose expiration deadline has arrived. */
export function expiredArchiveEntries(entries: readonly ArchiveEntry[], now: number): ArchiveEntry[] {
  return entries.filter((entry) => entry.expiresAt <= now);
}

/** Returns the next precise wake-up deadline, or null when the pool is empty. */
export function nextArchiveWakeAt(entries: readonly ArchiveEntry[]): number | null {
  let next: number | null = null;
  for (const entry of entries) {
    if (next === null || entry.expiresAt < next) next = entry.expiresAt;
  }
  return next;
}
