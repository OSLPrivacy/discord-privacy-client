export type BurnConfirmationStep = "ignored" | "armed" | "confirmed";

export type BurnGuaranteeId =
  | "local_osl_copy"
  | "osl_server_copy"
  | "other_person_app"
  | "connected_service_message"
  | "already_opened_copies";

export type BurnGuaranteeState = "available" | "request_only" | "unavailable" | "not_possible";

export interface BurnGuaranteeCopyItem {
  id: BurnGuaranteeId;
  title: string;
  state: BurnGuaranteeState;
  body: string;
}

export interface BurnGuaranteeCopyContract {
  summary: string;
  intro: string;
  items: readonly BurnGuaranteeCopyItem[];
  limit: string;
}

export const BurnGuaranteeCopy: BurnGuaranteeCopyContract = Object.freeze({
  summary: "Burn cleans up. It does not un-send.",
  intro: "OSL reports each cleanup step separately because one success does not prove the others.",
  items: Object.freeze([
    Object.freeze({
      id: "local_osl_copy",
      title: "This computer",
      state: "available",
      body: "OSL removes its local copy, local settings, and cached data for the selected scope.",
    }),
    Object.freeze({
      id: "osl_server_copy",
      title: "OSL server copy",
      state: "request_only",
      body: "OSL requests cleanup for protected copies it controls and reports whether that cleanup was acknowledged.",
    }),
    Object.freeze({
      id: "other_person_app",
      title: "Other person's app",
      state: "unavailable",
      body: "A peer cleanup request requires prior consent and a supported workflow. That workflow is unavailable in this build.",
    }),
    Object.freeze({
      id: "connected_service_message",
      title: "Connected service message",
      state: "request_only",
      body: "OSL can ask the connected service to remove its message only where that action is supported. The service decides.",
    }),
    Object.freeze({
      id: "already_opened_copies",
      title: "Already opened copies",
      state: "not_possible",
      body: "Burn cannot take back access someone already had, undo screenshots, remove exports, erase backups, or stop a camera.",
    }),
  ]),
  limit: "If OSL cannot verify a cleanup step, the UI must show that step as unknown, failed, unsupported, or unavailable.",
});

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
