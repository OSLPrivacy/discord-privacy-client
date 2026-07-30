/**
 * Paid-code issuance must stay dark until explicit prepaid-code redemption is
 * implemented. This is deliberately source-owned: there is no environment
 * variable, secret, or operator switch that can turn checkout back on.
 *
 * The redemption design is not implemented here. The future redemption change
 * must replace this false result in the same reviewed source closure that adds
 * the explicit redemption implementation and its acceptance proof.
 */
export function prepaidRedemptionReady(): boolean {
  return false;
}

/**
 * Dependency seam for dormant-path tests only. Production dispatch never
 * supplies an override, so every deployed call uses the source-owned false
 * readiness result above.
 */
export type PrepaidRedemptionReadiness = () => boolean;

export const PREPAID_REDEMPTION_UNAVAILABLE =
  "paid checkout is unavailable until prepaid-code redemption is ready";
