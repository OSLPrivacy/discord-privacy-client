export const REFUNDED_CODE_MESSAGE = "this code was refunded";
export const TAKEN_BACK_CODE_MESSAGE = "this code was taken back";

export interface RevokedLicenseReason {
  revoked_reason: string | null;
  terminal_event_type: string | null;
}

export function revokedLicenseMessage(row: RevokedLicenseReason): string | undefined {
  if (row.terminal_event_type === "charge.refunded") return REFUNDED_CODE_MESSAGE;
  if (
    row.terminal_event_type === "charge.dispute.created" ||
    row.revoked_reason === "chargeback"
  ) {
    return TAKEN_BACK_CODE_MESSAGE;
  }
  return undefined;
}
