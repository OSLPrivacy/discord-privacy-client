export type BurnConfirmationStep = "ignored" | "armed" | "confirmed";

/** In-memory confirmation only. It resets after confirmation or ten seconds. */
export class TwoStepBurnConfirmation {
  private armedUntil = 0;

  step(now: number, trusted: boolean): BurnConfirmationStep {
    if (!trusted || !Number.isFinite(now) || now < 0) return "ignored";
    if (this.armedUntil >= now && this.armedUntil !== 0) {
      this.armedUntil = 0;
      return "confirmed";
    }
    this.armedUntil = now + 10_000;
    return "armed";
  }

  expire(now: number): boolean {
    if (this.armedUntil === 0 || now < this.armedUntil) return false;
    this.armedUntil = 0;
    return true;
  }

  reset(): void { this.armedUntil = 0; }
}
