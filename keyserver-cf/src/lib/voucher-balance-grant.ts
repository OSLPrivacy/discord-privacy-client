/** Relay contract for capacity redemption.  Nothing identifies the holder. */
export const RELAY_VOUCHER_GRANT_FIELDS = ["aud", "exp", "jti"] as const;

export interface RelayVoucherGrant {
  readonly aud: "osl-capacity-voucher";
  readonly exp: number;
  readonly jti: string;
}

export function validateRelayVoucherGrant(grant: RelayVoucherGrant): RelayVoucherGrant {
  const keys = Object.keys(grant).sort();
  if (keys.join(",") !== RELAY_VOUCHER_GRANT_FIELDS.join(",") || grant.aud !== "osl-capacity-voucher" || !Number.isSafeInteger(grant.exp) || !/^[A-Za-z0-9_-]{16,}$/.test(grant.jti)) {
    throw new Error("voucher grant refused");
  }
  return Object.freeze({ ...grant });
}
