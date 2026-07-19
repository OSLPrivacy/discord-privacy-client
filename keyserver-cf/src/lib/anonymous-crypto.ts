import type { Env } from "../env.js";
import { signedWatcherRequestHeaders, sha256Hex } from "./crypto-watcher-auth.js";

export type CryptoAsset = "btc" | "xmr";
export type CryptoPlan = "pro";

export function cryptoAssetEnabled(env: Env, asset: CryptoAsset): boolean {
  return asset === "btc"
    ? env.CRYPTO_BTC_ENABLED === "true"
    : env.CRYPTO_XMR_ENABLED === "true";
}

export interface AnonymousInvoiceRow {
  invoice_id: string;
  claim_hash: string;
  payment_method: CryptoAsset;
  plan: CryptoPlan;
  amount_usd_cents: number;
  amount_atomic: string;
  confirmations_required: number;
  price_locked_at: number;
  delivery_public_key_spki: string;
  status: "pending" | "paid" | "expired" | "delivery_ready";
  settlement_event_id: string | null;
  encrypted_license: string | null;
  created_at: number;
  expires_at: number;
  resolved_at: number | null;
  cleanup_at: number;
  delivered_at: number | null;
  acknowledged_at: number | null;
}

export function nativeToAtomic(native: string, asset: CryptoAsset): string {
  const decimals = asset === "btc" ? 8 : 12;
  if (!/^\d+\.\d+$/.test(native)) throw new Error("native amount is not decimal");
  const [whole, fraction = ""] = native.split(".");
  if (fraction.length > decimals) throw new Error("native amount exceeds asset precision");
  const atomic = `${whole}${fraction.padEnd(decimals, "0")}`.replace(/^0+(?=\d)/, "");
  if (!/^\d+$/.test(atomic) || atomic === "0") throw new Error("native amount is zero");
  return atomic;
}

/** Quote in integer atomic units and round upward by at most one atomic unit,
 * avoiding floating-point undercharges. */
export function usdCentsToAtomic(
  usdCents: number,
  priceUsdPerCoin: string,
  asset: CryptoAsset,
): { amountAtomic: string; amountNative: string } {
  if (!Number.isSafeInteger(usdCents) || usdCents <= 0 ||
      !/^\d+(?:\.\d+)?$/.test(priceUsdPerCoin)) {
    throw new Error("price input malformed");
  }
  const [whole, fraction = ""] = priceUsdPerCoin.split(".");
  const priceScale = 10n ** BigInt(fraction.length);
  const priceUnits = BigInt(`${whole}${fraction}`);
  if (priceUnits <= 0n) throw new Error("price must be positive");
  const assetDecimals = asset === "btc" ? 8 : 12;
  const numerator = BigInt(usdCents) * (10n ** BigInt(assetDecimals)) * priceScale;
  const denominator = 100n * priceUnits;
  const atomic = (numerator + denominator - 1n) / denominator;
  const digits = atomic.toString().padStart(assetDecimals + 1, "0");
  return {
    amountAtomic: atomic.toString(),
    amountNative: `${digits.slice(0, -assetDecimals)}.${digits.slice(-assetDecimals)}`,
  };
}

export function newClaimToken(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

export async function validateDeliveryPublicKey(spkiBase64: string): Promise<CryptoKey> {
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(spkiBase64) || spkiBase64.length > 1024) {
    throw new Error("delivery public key malformed");
  }
  const binary = atob(spkiBase64);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return crypto.subtle.importKey(
    "spki",
    bytes,
    { name: "RSA-OAEP", hash: "SHA-256" },
    false,
    ["encrypt"],
  );
}

export async function encryptLicenseForDelivery(
  publicKeySpki: string,
  license: string,
): Promise<string> {
  const key = await validateDeliveryPublicKey(publicKeySpki);
  const encrypted = await crypto.subtle.encrypt(
    { name: "RSA-OAEP" },
    key,
    new TextEncoder().encode(license),
  );
  let binary = "";
  for (const byte of new Uint8Array(encrypted)) binary += String.fromCharCode(byte);
  return btoa(binary);
}

export async function createWatcherInvoice(
  env: Env,
  body: {
    invoice_id: string;
    payment_method: CryptoAsset;
    amount_atomic: string;
    expires_at: number;
  },
  fetcher: typeof fetch = fetch,
): Promise<{ address: string }> {
  if (!env.CRYPTO_WATCHER_URL || !env.CRYPTO_WATCHER_REQUEST_SECRET) {
    throw new Error("crypto watcher is not configured");
  }
  const watcherUrl = new URL(env.CRYPTO_WATCHER_URL);
  if (watcherUrl.protocol !== "https:") {
    throw new Error("crypto watcher URL must use HTTPS");
  }
  const payload = JSON.stringify(body);
  const response = await fetcher(`${watcherUrl.href.replace(/\/$/, "")}/v1/invoices`, {
    method: "POST",
    headers: await signedWatcherRequestHeaders(
      env.CRYPTO_WATCHER_REQUEST_SECRET,
      "POST",
      "/v1/invoices",
      payload,
    ),
    body: payload,
    // The isolated watcher allows its loopback wallet RPC up to eight seconds.
    // Keep the Worker deadline longer so a successfully allocated address is
    // not abandoned only because the outer request timed out first.
    signal: AbortSignal.timeout(12_000),
  });
  if (!response.ok) throw new Error(`crypto watcher returned ${response.status}`);
  const result = (await response.json()) as { invoice_id?: unknown; address?: unknown };
  if (result.invoice_id !== body.invoice_id) {
    throw new Error("crypto watcher returned a mismatched invoice id");
  }
  if (typeof result.address !== "string" || !validPaymentAddress(result.address, body.payment_method)) {
    throw new Error("crypto watcher returned an invalid address");
  }
  return { address: result.address };
}

function validPaymentAddress(address: string, asset: CryptoAsset): boolean {
  if (asset === "btc") {
    // Production invoices use Bitcoin mainnet native SegWit addresses. This
    // validates the network and Bech32/Bech32m character shape; Bitcoin Core
    // remains the checksum authority that generated the address.
    return /^bc1[023456789acdefghjklmnpqrstuvwxyz]{11,87}$/.test(address);
  }
  // Monero mainnet primary addresses start with 4 and subaddresses with 8.
  // Wallet RPC is still the checksum authority; reject other networks/shapes.
  return /^[48][123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz]{94}$/.test(address);
}

export async function insertAnonymousInvoice(
  db: D1Database,
  row: Omit<AnonymousInvoiceRow,
    "claim_hash" | "status" | "settlement_event_id" | "encrypted_license" |
    "created_at" | "resolved_at" | "delivered_at" | "acknowledged_at"
  > & { claim_token: string },
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await db.prepare(
    `INSERT INTO crypto_invoices_v2
      (invoice_id, claim_hash, payment_method, plan, amount_usd_cents,
       amount_atomic, confirmations_required, price_locked_at,
       delivery_public_key_spki, status, settlement_event_id,
       encrypted_license, created_at, expires_at, resolved_at, cleanup_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, NULL, ?, ?, NULL, ?)`,
  ).bind(
    row.invoice_id,
    await sha256Hex(row.claim_token),
    row.payment_method,
    row.plan,
    row.amount_usd_cents,
    row.amount_atomic,
    row.confirmations_required,
    row.price_locked_at,
    row.delivery_public_key_spki,
    now,
    row.expires_at,
    row.cleanup_at,
  ).run();
}

export async function getAnonymousInvoice(db: D1Database, invoiceId: string): Promise<AnonymousInvoiceRow | null> {
  return await db.prepare("SELECT * FROM crypto_invoices_v2 WHERE invoice_id = ?")
    .bind(invoiceId).first<AnonymousInvoiceRow>();
}

export async function claimMatches(row: AnonymousInvoiceRow, claimToken: string): Promise<boolean> {
  const actual = await sha256Hex(claimToken);
  const encoder = new TextEncoder();
  const [actualDigest, storedDigest] = await Promise.all([
    crypto.subtle.digest("SHA-256", encoder.encode(actual)),
    crypto.subtle.digest("SHA-256", encoder.encode(row.claim_hash)),
  ]);
  return crypto.subtle.timingSafeEqual(actualDigest, storedDigest);
}

export async function markCryptoDeliveryFetched(
  db: D1Database,
  invoiceId: string,
): Promise<void> {
  await db.prepare(
    `UPDATE crypto_invoices_v2
        SET delivered_at = COALESCE(delivered_at, ?)
      WHERE invoice_id = ? AND status = 'delivery_ready' AND acknowledged_at IS NULL`,
  ).bind(Math.floor(Date.now() / 1000), invoiceId).run();
}

export async function acknowledgeCryptoDelivery(
  db: D1Database,
  invoiceId: string,
): Promise<"acknowledged" | "already_acknowledged" | "not_ready"> {
  const changed = await db.prepare(
    `UPDATE crypto_invoices_v2
        SET acknowledged_at = ?, encrypted_license = NULL
      WHERE invoice_id = ? AND status = 'delivery_ready' AND acknowledged_at IS NULL`,
  ).bind(Math.floor(Date.now() / 1000), invoiceId).run();
  if ((changed.meta?.changes ?? 0) === 1) return "acknowledged";
  const row = await db.prepare(
    "SELECT status, acknowledged_at FROM crypto_invoices_v2 WHERE invoice_id = ?",
  ).bind(invoiceId).first<{ status: string; acknowledged_at: number | null }>();
  if (row?.status === "delivery_ready" && row.acknowledged_at !== null) {
    return "already_acknowledged";
  }
  return "not_ready";
}
