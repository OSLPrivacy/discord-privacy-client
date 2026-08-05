import { verifyEd25519 } from "./crypto.js";
import { decodeBase64 } from "./validation.js";
import { analyzeIdentifier } from "./unicode-identifier/runtime.js";

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
/// D-248. Thrown when a handle cannot be reduced to a skeleton that the
/// directory's uniqueness rule can trust. It is never a "warn and continue":
/// the caller turns it into a refusal, because a claim whose skeleton is
/// unknown is a claim whose confusables are unconstrained.
export class UsernameNotAnalyzable extends Error {
  constructor(reason: string) {
    super(`username is not an acceptable identifier: ${reason}`);
    this.name = "UsernameNotAnalyzable";
  }
}

/// D-248. The UTS #39 skeleton of a handle, computed by the ONE pinned Rust
/// artifact (`src/lib/unicode-identifier/`). It is stored in
/// `username_directory.username_skeleton`, whose unique index
/// (`idx_username_directory_skeleton`, migration 0038) is what makes two
/// visually confusable handles collide.
///
/// **This is a UNIQUENESS CONSTRAINT, not a warning signal.** That is what
/// migration 0038 and contract 0100 already assume — 0038 creates
/// `CREATE UNIQUE INDEX idx_username_directory_skeleton`, 0038's retired-name
/// trigger matches on `skeleton = NEW.username_skeleton`, and 0100 makes the
/// column mandatory. The code was the only part that did not agree: it wrote
/// the raw name into the column, so the index could only ever collide on names
/// that already collided on the primary key.
///
/// The claim path is validate-don't-transform (D-162): the grammar
/// `USERNAME_RE` is ASCII-only and the request must already be canonical. So
/// `analysis.normalized` is the input for every accepted handle, and the check
/// below is a live tripwire rather than a transform — if the grammar is ever
/// widened to a form this artifact would fold differently, the claim is
/// REFUSED rather than silently stored under a name nobody typed.
export function usernameSkeleton(username: string): string {
  const analysis = analyzeIdentifier(username);
  if (analysis.normalized !== username) {
    throw new UsernameNotAnalyzable("not canonical under UTS #39 normalization");
  }
  if (!analysis.identifierAllowed) {
    throw new UsernameNotAnalyzable("outside the UTS #39 identifier profile");
  }
  if (analysis.hasExcessMarks) {
    throw new UsernameNotAnalyzable("too many consecutive combining marks");
  }
  if (analysis.skeleton.length === 0) {
    throw new UsernameNotAnalyzable("empty skeleton");
  }
  // D-248b. UTS #39 confusable prototypes are NOT case-folded, and this
  // identifier space IS case-insensitive (`USERNAME_RE` admits no uppercase, and
  // `analyze_identifier` lowercases before it skeletons). Measured over the
  // whole shipping alphabet `[a-z0-9_]`, exactly three characters are not their
  // own skeleton -- `m -> rn`, `1 -> l`, and `0 -> O` -- so without a final
  // fold the raw skeleton leaves `supp0rt` and `support` in DIFFERENT classes.
  // Zero-for-o is the commonest ASCII homograph there is; a defence that misses
  // it is green and useless.
  //
  // The fold is the ARTIFACT'S OWN normalizer (`analysis.normalized` is NFKC +
  // Unicode lowercase, computed in Rust), applied a second time. That is
  // deliberate: a `toLowerCase()` here would be the start of the second,
  // subtly-different Unicode implementation the artifact exists to prevent.
  //
  // Measured effect on the shipping alphabet: it merges exactly one further
  // class, {`o`, `0`}, and splits none. It only ever widens the confusable set.
  const folded = analyzeIdentifier(analysis.skeleton).normalized;
  if (folded.length === 0) throw new UsernameNotAnalyzable("empty folded skeleton");
  return folded;
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
