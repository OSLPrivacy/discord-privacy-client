/// POST /v1/license/redeem
///
/// Redeems a prepaid license exactly once. A repeat redemption is refused;
/// clients use `/v1/license/validate` to read back an already-redeemed period.

import type { Env } from "../env.js";
import { hashLicense, normalizeLicense, validateChecksum } from "../lib/license.js";
import { badRequest, conflict, json, serviceUnavailable, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

interface RedemptionRow {
  subscription_id: string;
  revoked_at: number | null;
  redeemed_at: number | null;
  expires_at: number | null;
}

export async function handleLicenseRedeem(
  request: Request,
  env: Env,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 10, "license-redeem");
  if (!rl.ok) return tooMany(rl.retryAfter);
  const issuer = env.DEPLOYMENT_ENV === "qa" ? "qa" : "production";
  const hmacSecret = issuer === "qa"
    ? env.QA_LICENSE_HMAC_SECRET
    : env.LICENSE_HMAC_SECRET;
  if (!hmacSecret) {
    return serviceUnavailable("license validation is not configured on this deployment");
  }
  if (
    issuer === "qa" &&
    env.LICENSE_HMAC_SECRET &&
    env.LICENSE_HMAC_SECRET === env.QA_LICENSE_HMAC_SECRET
  ) {
    return serviceUnavailable("QA license trust root is not isolated");
  }

  let body: { license_key?: unknown };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (typeof body.license_key !== "string" || body.license_key.length === 0) {
    return badRequest("license_key required");
  }

  const normalized = normalizeLicense(body.license_key, issuer);
  if (!normalized) return json({ status: "UNKNOWN", checksum_ok: false });
  const checksum_ok = await validateChecksum(normalized, hmacSecret, issuer);
  if (!checksum_ok) return json({ status: "UNKNOWN", checksum_ok: false });

  const licenseHash = await hashLicense(normalized);
  const now = Math.floor(Date.now() / 1000);
  const redemption = await env.DB.prepare(
    `UPDATE licenses
        SET redeemed_at = ?, expires_at = ? + grant_seconds
      WHERE license_hash = ?
        AND redeemed_at IS NULL
        AND revoked_at IS NULL
        AND grant_seconds IS NOT NULL`,
  ).bind(now, now, licenseHash).run();

  const license = await env.DB.prepare(
    `SELECT subscription_id, revoked_at, redeemed_at, expires_at
       FROM licenses
      WHERE license_hash = ?`,
  ).bind(licenseHash).first<RedemptionRow>();
  if (!license) return json({ status: "UNKNOWN", checksum_ok: true });
  if (license.revoked_at !== null) return json({ status: "REVOKED", checksum_ok: true });
  if ((redemption.meta?.changes ?? 0) === 0 && license.redeemed_at !== null) {
    return conflict("license code already redeemed");
  }
  if (license.redeemed_at === null || license.expires_at === null) {
    return json({ status: "UNKNOWN", checksum_ok: true });
  }
  if (license.expires_at <= now) return json({ status: "EXPIRED", checksum_ok: true });
  await env.DB.prepare(
    `UPDATE subscriptions
        SET status = 'ACTIVE', current_period_end = ?, updated_at = ?
      WHERE subscription_id = ?`,
  ).bind(license.expires_at, now, license.subscription_id).run();
  return json({
    status: "ACTIVE",
    redeemed_at: license.redeemed_at,
    expires_at: license.expires_at,
    checksum_ok: true,
  });
}
