/**
 * Current-draft aggregate only. No interval sequence or exact per-key cadence
 * is retained, persisted, transmitted, or reused across messages.
 */
export class CoarseTypingRate {
  private startedAt: number | null = null;
  private lastInputAt: number | null = null;
  private maximumCharacters = 0;
  private trustedInputCount = 0;

  recordTrustedInput(now: number, trusted: boolean, currentCharacters: number): void {
    if (!trusted || !Number.isFinite(now) || now < 0
      || !Number.isSafeInteger(currentCharacters) || currentCharacters < 0 || currentCharacters > 1_000) return;
    if (this.startedAt === null) this.startedAt = now;
    this.lastInputAt = now;
    this.maximumCharacters = Math.max(this.maximumCharacters, currentCharacters);
    this.trustedInputCount += 1;
  }

  /** Returns an even 2–16 chars/sec bucket, or zero without enough evidence. */
  charsPerSecond(): number {
    if (this.startedAt === null || this.lastInputAt === null || this.trustedInputCount < 2
      || this.maximumCharacters < 2) return 0;
    const elapsedSeconds = Math.max((this.lastInputAt - this.startedAt) / 1_000, 0.5);
    const raw = this.maximumCharacters / elapsedSeconds;
    return Math.max(2, Math.min(16, Math.round(raw / 2) * 2));
  }

  reset(): void {
    this.startedAt = null;
    this.lastInputAt = null;
    this.maximumCharacters = 0;
    this.trustedInputCount = 0;
  }
}
