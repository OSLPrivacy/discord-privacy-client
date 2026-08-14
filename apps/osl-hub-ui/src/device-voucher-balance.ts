/**
 * A data balance is not an account property.  It is the capacity still held
 * by bearer vouchers in this local wallet.  The relay-facing grant deliberately
 * carries only aud, exp, and jti; capacity and consumption never leave here.
 */
export type VoucherMeter = "stored" | "moved";

export const BEARER_LOSS_DISCLOSURE =
  "Voucher capacity is stored as a bearer credential on this device. If the credential or device is lost before the capacity is used, OSL cannot recover or refund it.";

export interface RelayGrant {
  readonly aud: "osl-capacity-voucher";
  readonly exp: number;
  readonly jti: string;
}

export interface BearerVoucher {
  readonly token: string;
  readonly grant: RelayGrant;
  readonly capacity: Readonly<Record<VoucherMeter, number>>;
  readonly spent: Readonly<Record<VoucherMeter, number>>;
}

export interface LocalVoucherWallet {
  readonly vouchers: readonly BearerVoucher[];
}

export interface VoucherBalance {
  readonly stored: number;
  readonly moved: number;
}

const meters: readonly VoucherMeter[] = ["stored", "moved"];

function exactKeys(value: object, expected: readonly string[], label: string): void {
  const actual = Object.keys(value).sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== [...expected].sort()[index])) {
    throw new Error(`${label} fields must be exactly ${expected.join(", ")}`);
  }
}

function units(value: number, label: string): number {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`${label} must be a non-negative safe integer`);
  return value;
}

function validateGrant(grant: RelayGrant): void {
  exactKeys(grant, ["aud", "exp", "jti"], "relay grant");
  if (grant.aud !== "osl-capacity-voucher" || !Number.isSafeInteger(grant.exp) || grant.exp < 1 || !/^[A-Za-z0-9_-]{16,}$/.test(grant.jti)) {
    throw new Error("relay grant refused");
  }
}

function validateVoucher(voucher: BearerVoucher): void {
  if (!/^[A-Za-z0-9_-]{32,}$/.test(voucher.token)) throw new Error("blind voucher token refused");
  validateGrant(voucher.grant);
  for (const meter of meters) {
    const capacity = units(voucher.capacity[meter], `${meter} capacity`);
    const spent = units(voucher.spent[meter], `${meter} spent`);
    if (spent > capacity) throw new Error(`${meter} spend exceeds voucher capacity`);
  }
}

/** Opens the encrypted local record after a process restart; it never consults a relay. */
export function openLocalVoucherWallet(serialized: string): LocalVoucherWallet {
  const parsed: unknown = JSON.parse(serialized);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) throw new Error("local voucher wallet refused");
  const value = parsed as { vouchers?: unknown };
  if (!Array.isArray(value.vouchers)) throw new Error("local voucher wallet has no vouchers");
  const vouchers = value.vouchers as BearerVoucher[];
  const identifiers = new Set<string>();
  for (const voucher of vouchers) {
    validateVoucher(voucher);
    if (identifiers.has(voucher.grant.jti)) throw new Error("duplicate local voucher");
    identifiers.add(voucher.grant.jti);
  }
  return Object.freeze({ vouchers: Object.freeze(vouchers.map((voucher) => Object.freeze({
    ...voucher,
    grant: Object.freeze({ ...voucher.grant }),
    capacity: Object.freeze({ ...voucher.capacity }),
    spent: Object.freeze({ ...voucher.spent }),
  }))) });
}

export function serializeLocalVoucherWallet(wallet: LocalVoucherWallet): string {
  for (const voucher of wallet.vouchers) validateVoucher(voucher);
  return JSON.stringify({ vouchers: wallet.vouchers });
}

/** Redemption deposits a new blind bearer voucher locally; there is no balance request. */
export function redeemVoucher(wallet: LocalVoucherWallet, voucher: Omit<BearerVoucher, "spent">): LocalVoucherWallet {
  const redeemed: BearerVoucher = { ...voucher, spent: { stored: 0, moved: 0 } };
  validateVoucher(redeemed);
  if (wallet.vouchers.some((existing) => existing.grant.jti === redeemed.grant.jti)) throw new Error("voucher already redeemed locally");
  return openLocalVoucherWallet(JSON.stringify({ vouchers: [...wallet.vouchers, redeemed] }));
}

/** Spend is terminal local arithmetic against one voucher and one meter. */
export function spendVoucherCapacity(wallet: LocalVoucherWallet, jti: string, meter: VoucherMeter, amount: number): LocalVoucherWallet {
  units(amount, `${meter} spend`);
  let found = false;
  const vouchers = wallet.vouchers.map((voucher) => {
    if (voucher.grant.jti !== jti) return voucher;
    found = true;
    if (voucher.spent[meter] + amount > voucher.capacity[meter]) throw new Error(`${meter} voucher capacity exhausted`);
    return { ...voucher, spent: { ...voucher.spent, [meter]: voucher.spent[meter] + amount } };
  });
  if (!found) throw new Error("local voucher absent");
  return openLocalVoucherWallet(JSON.stringify({ vouchers }));
}

/** Independent per-meter sum over every unspent voucher in this local wallet. */
export function dataBalance(wallet: LocalVoucherWallet): VoucherBalance {
  const result: Record<VoucherMeter, number> = { stored: 0, moved: 0 };
  for (const voucher of wallet.vouchers) {
    validateVoucher(voucher);
    for (const meter of meters) {
      const unspent = voucher.capacity[meter] - voucher.spent[meter];
      if (!Number.isSafeInteger(result[meter] + unspent)) throw new Error(`${meter} balance exceeds safe integer range`);
      result[meter] += unspent;
    }
  }
  return Object.freeze(result);
}

export function deviceVoucherBalanceSettingsMarkup(wallet: LocalVoucherWallet): string {
  const balance = dataBalance(wallet);
  return `<section class="settings-section" data-device-voucher-balance><h2>Data balance on this device</h2><p>${BEARER_LOSS_DISCLOSURE}</p><dl><dt>Stored</dt><dd data-voucher-balance-meter="stored">${balance.stored}</dd><dt>Moved</dt><dd data-voucher-balance-meter="moved">${balance.moved}</dd></dl></section>`;
}
