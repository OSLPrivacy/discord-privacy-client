/** Deployed-store grant projection: its complete field set is aud, exp, jti. */
export const DEPLOYED_VOUCHER_GRANT_FIELDS = ["aud", "exp", "jti"] as const;

export type DeployedVoucherGrant = Readonly<{
  aud: "osl-capacity-voucher";
  exp: number;
  jti: string;
}>;

export function deployedVoucherGrantColumns(grant: DeployedVoucherGrant): readonly [string, number, string] {
  if (Object.keys(grant).sort().join(",") !== DEPLOYED_VOUCHER_GRANT_FIELDS.join(",")) throw new Error("voucher grant columns refused");
  return [grant.aud, grant.exp, grant.jti];
}
