import { verifyEd25519 } from "./crypto.js";
import { decodeBase64 } from "./validation.js";

export const USERNAME_CLAIM_DOMAIN = "OSL-USERNAME-CLAIM-v1";
export const USERNAME_RE = /^[a-z0-9](?:[a-z0-9_]{1,28}[a-z0-9])?$/;
export const USERNAME_MIN = 3;
export const USERNAME_MAX = 30;
export const USERNAME_FRESHNESS_MS = 5 * 60 * 1000;
const FRIEND_CODE_PREFIX = "OSLFR1.";

export function validNormalizedUsername(value: unknown): value is string {
  return typeof value === "string" && value.length >= USERNAME_MIN &&
    value.length <= USERNAME_MAX && USERNAME_RE.test(value);
}
export function usernameClaimMessage(input: {
  username: string;
  user_id: string;
  friend_code: string;
  request_id: string;
  timestamp_ms: number;
}): Uint8Array {
  return new TextEncoder().encode(
    `${USERNAME_CLAIM_DOMAIN}\n${input.username}\n${input.user_id}\n${input.friend_code}\n${input.request_id}\n${input.timestamp_ms}`,
  );
}

function decodeBase64Url(value: string): Uint8Array {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) throw new Error("invalid base64url");
  const padded = value.replace(/-/g, "+").replace(/_/g, "/") +
    "=".repeat((4 - value.length % 4) % 4);
  return decodeBase64(padded);
}

export async function validateFriendCode(
  friendCode: string,
  expectedUserId: string,
  expectedEd25519PublicB64: string,
): Promise<boolean> {
  if (!friendCode.startsWith(FRIEND_CODE_PREFIX) || friendCode.length > 8199) return false;
  try {
    const encoded = friendCode.slice(FRIEND_CODE_PREFIX.length);
    const decoded = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(decodeBase64Url(encoded));
    const signed = JSON.parse(decoded) as Record<string, unknown>;
    if (Object.keys(signed).sort().join(",") !== "payload,signature") return false;
    if (typeof signed.signature !== "string" || typeof signed.payload !== "object" || signed.payload === null) return false;
    const payload = signed.payload as Record<string, unknown>;
    const allowed = ["ed25519_public", "mlkem768_public", "osl_user_id", "ratchet_initial_public", "version", "x25519_public"];
    if (Object.keys(payload).sort().join(",") !== allowed.join(",")) return false;
    if (payload.version !== 1 || payload.osl_user_id !== expectedUserId ||
        payload.ed25519_public !== expectedEd25519PublicB64) return false;
    if (typeof payload.x25519_public !== "string" || typeof payload.mlkem768_public !== "string" ||
        !(payload.ratchet_initial_public === null || typeof payload.ratchet_initial_public === "string")) return false;
    if (decodeBase64(payload.x25519_public).length !== 32 ||
        decodeBase64(payload.ed25519_public as string).length !== 32 ||
        decodeBase64(payload.mlkem768_public).length !== 1184 ||
        (typeof payload.ratchet_initial_public === "string" && decodeBase64(payload.ratchet_initial_public).length !== 32)) return false;
    // Rust's FriendCodeUnsigned serde order is fixed to this exact shape.
    const canonical = new TextEncoder().encode(JSON.stringify({
      version: payload.version,
      osl_user_id: payload.osl_user_id,
      x25519_public: payload.x25519_public,
      ed25519_public: payload.ed25519_public,
      mlkem768_public: payload.mlkem768_public,
      ratchet_initial_public: payload.ratchet_initial_public,
    }));
    return await verifyEd25519(
      decodeBase64(expectedEd25519PublicB64),
      canonical,
      decodeBase64Url(signed.signature),
    );
  } catch {
    return false;
  }
}
