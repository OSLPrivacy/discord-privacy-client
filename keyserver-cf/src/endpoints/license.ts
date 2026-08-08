/// POST /v1/license/validate
///
/// Body: { license_key: string }
/// Returns: { status: "ACTIVE" | "GRACE" | "CANCELLED" | "EXPIRED" |
///                    "REVOKED" | "PENDING" | "UNKNOWN",
///           current_period_end?: number,
///           checksum_ok: boolean }
///
/// Public — the license IS the bearer credential. Rate-limited so
/// an attacker can't brute-force the key space online (which would
/// take centuries even unlimited, but we don't pay for that
/// bandwidth either).
///
/// `checksum_ok` is the cheap typo gate. When false, the client
/// knows the user mistyped — show "please re-enter" rather than
/// "your license is not recognised."

import type { Env } from "../env.js";
import { hashLicense, normalizeLicense, validateChecksum } from "../lib/license.js";
import { revokedLicenseMessage } from "../lib/license-refusal.js";
import { getSubscription } from "../lib/subscriptions.js";
import { badRequest, json, serviceUnavailable, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

interface ValidationLicenseRow {
  subscription_id: string;
  revoked_at: number | null;
  revoked_reason: string | null;
  terminal_event_type: string | null;
  redeemed_at: number | null;
  expires_at: number | null;
}

function revokedMessage(reason: string | null): string | undefined {
  if (reason === "manual") return "this code was refunded";
  return undefined;
}

export async function handleLicenseValidate(
  request: Request,
  env: Env,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 10, "license-validate");
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

  // The issuer marker is part of the credential format. A production Worker
  // never falls through to the QA secret, even if a QA hash was accidentally
  // copied into its database; a QA Worker likewise accepts only OSLQ codes.
  const normalized = normalizeLicense(body.license_key, issuer);
  if (!normalized) {
    return json({ status: "UNKNOWN", checksum_ok: false });
  }
  const checksum_ok = await validateChecksum(normalized, hmacSecret, issuer);
  if (!checksum_ok) {
    return json({ status: "UNKNOWN", checksum_ok: false });
  }

  const hash = await hashLicense(normalized);
  const license = await env.DB.prepare(
    `SELECT licenses.subscription_id,
            licenses.revoked_at,
            licenses.revoked_reason,
            observations.event_type AS terminal_event_type,
            licenses.redeemed_at,
            licenses.expires_at
       FROM licenses
       LEFT JOIN stripe_checkout_claims AS claims
              ON claims.license_hash = licenses.license_hash
       LEFT JOIN stripe_subscription_observations AS observations
              ON observations.subscription_id = COALESCE(claims.subscription_id, licenses.subscription_id)
             AND observations.status IN ('REVOKED', 'EXPIRED')
      WHERE licenses.license_hash = ?`,
  ).bind(hash).first<ValidationLicenseRow>();
  if (!license) {
    return json({ status: "UNKNOWN", checksum_ok: true });
  }
  if (license.revoked_at !== null) {
    const error = revokedMessage(license.revoked_reason);
    const message = revokedLicenseMessage(license);
    return json({
      status: "REVOKED",
      redeemed_at: license.redeemed_at,
      expires_at: license.expires_at,
      checksum_ok: true,
      ...(error ? { error } : {}),
      ...(message ? { message } : {}),
    });
  }
  const sub = await getSubscription(env.DB, license.subscription_id);
  if (!sub) {
    // Orphan license — should not happen given the FK. Treat as
    // REVOKED so the client locks paid features.
    return json({ status: "REVOKED", checksum_ok: true });
  }
  if (sub.status === "REVOKED" || sub.status === "EXPIRED") {
    return json({
      status: sub.status,
      redeemed_at: license.redeemed_at,
      expires_at: license.expires_at,
      checksum_ok: true,
    });
  }
  if (license.redeemed_at === null) {
    return json({
      status: "UNREDEEMED",
      redeemed_at: null,
      expires_at: license.expires_at,
      checksum_ok: true,
    });
  }
  if (license.expires_at === null) {
    // A redeemed license must have an expiry. Treat a corrupt partial row as
    // non-entitled rather than granting a perpetual Pro state.
    return json({
      status: "UNKNOWN",
      redeemed_at: license.redeemed_at,
      expires_at: null,
      checksum_ok: true,
    });
  }
  if (license.expires_at <= Math.floor(Date.now() / 1000)) {
    return json({
      status: "EXPIRED",
      redeemed_at: license.redeemed_at,
      expires_at: license.expires_at,
      checksum_ok: true,
    });
  }
  return json({
    status: sub.status,
    current_period_end: sub.current_period_end,
    redeemed_at: license.redeemed_at,
    expires_at: license.expires_at,
    checksum_ok: true,
  });
}
