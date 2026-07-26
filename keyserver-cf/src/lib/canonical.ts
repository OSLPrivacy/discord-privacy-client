/// Canonical byte encoding for signature verification. Ported
/// from keyserver/src/canonical.js. The Rust client constructs
/// the same byte string in crates/keystore/src/burn.rs +
/// prekeys.rs — these implementations MUST stay byte-identical
/// or signatures fail to verify.
///
/// Wire format (length-prefixed = `u32 BE length || bytes`):
///
///   Burn:
///     LP(domain)
///     LP(user_id)
///     LP(timestamp_ms decimal string)
///     LP(request_id base64url string)
///     LP(scope_str: "single" | "to_user" | "all")
///     target_kind: u8  (0 = none / all, 1 = content_id, 2 = user_id)
///     if target_kind != 0: LP(target_value)
///
///   Replenish:
///     LP(domain)
///     LP(user_id)
///     LP(timestamp_ms decimal string)
///     LP(request_id base64url string)
///     spk_present: u8  (0 | 1)
///     if spk_present:
///       LP(spk.pub_b64 string)        ← NOT decoded; the base64 chars
///       LP(spk.signature_b64 string)  ← NOT decoded
///       LP(spk.rotated_at string)
///     opk_count: u32 BE
///     per opk: u32 BE id, LP(opk.pub_b64 string)
///
/// The base64-string-not-bytes form for SPK / OPK pubs is
/// intentional — both sides can produce the encoding without ever
/// round-tripping through base64 decode.

const REPLENISH_DOMAIN = "discord-privacy-client/prekey-replenish/v1";
const BURN_DOMAIN = "discord-privacy-client/burn/v1";
const UNREGISTER_DOMAIN = "discord-privacy-client/unregister/v1";
const CONTROL_INBOX_POST_DOMAIN = "discord-privacy-client/control-inbox-post/v1";
const CONTROL_INBOX_GET_DOMAIN = "discord-privacy-client/control-inbox-get/v1";
const CONTROL_INBOX_DELETE_DOMAIN = "discord-privacy-client/control-inbox-delete/v1";
const PREKEY_BUNDLE_GET_DOMAIN = "discord-privacy-client/prekey-bundle-get/v1";
const WRAPPED_KEY_GET_DOMAIN = "discord-privacy-client/wrapped-key-get/v1";
const WRAPPED_KEY_POST_DOMAIN = "discord-privacy-client/wrapped-key-post/v1";

export const SIGNED_COMMAND_FRESHNESS_WINDOW_MS = 5 * 60 * 1000;

function concatBytes(parts: Uint8Array[]): Uint8Array {
  let total = 0;
  for (const p of parts) total += p.length;
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

function u32be(n: number): Uint8Array {
  if (!Number.isSafeInteger(n) || n < 0 || n > 0xffff_ffff) {
    throw new RangeError("canonical u32 value out of range");
  }
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, n, false);
  return out;
}

function u8(v: number): Uint8Array {
  return new Uint8Array([v & 0xff]);
}

function lpString(s: string): Uint8Array {
  const bytes = new TextEncoder().encode(s);
  return concatBytes([u32be(bytes.length), bytes]);
}

// ---- burn ----

export type BurnScope = "single" | "to_user" | "all";
export interface BurnTarget {
  content_id?: string;
  user_id?: string;
}

export function canonicalBurnBytes(args: {
  user_id: string;
  timestamp_ms: number;
  request_id: string;
  scope: BurnScope;
  target?: BurnTarget;
}): Uint8Array {
  const parts: Uint8Array[] = [
    lpString(BURN_DOMAIN),
    lpString(args.user_id),
    lpString(String(args.timestamp_ms)),
    lpString(args.request_id),
    lpString(args.scope),
  ];
  if (args.scope === "single") {
    if (!args.target?.content_id) {
      throw new Error("canonicalBurnBytes: scope=single needs target.content_id");
    }
    parts.push(u8(1));
    parts.push(lpString(args.target.content_id));
  } else if (args.scope === "to_user") {
    if (!args.target?.user_id) {
      throw new Error("canonicalBurnBytes: scope=to_user needs target.user_id");
    }
    parts.push(u8(2));
    parts.push(lpString(args.target.user_id));
  } else {
    parts.push(u8(0));
  }
  return concatBytes(parts);
}

// ---- unregister ----
//
// Used by the account-burn flow to delete the user's keyserver row
// so the next register call hits the empty-row path (Case A in
// handleRegister) instead of being rejected as "user_id registered
// to a different key". Signed by the CURRENT stored
// `ik_ed25519_pub` — only the legitimate identity holder can issue.
//
// Wire:
//   LP(domain) || LP(user_id) || LP(timestamp_ms_string)
//
// timestamp_ms is a freshness anchor; the server rejects requests
// whose timestamp is more than UNREGISTER_FRESHNESS_WINDOW_MS away
// from server clock. Prevents indefinite replay of an old signature.

export const UNREGISTER_FRESHNESS_WINDOW_MS = 5 * 60 * 1000; // 5 minutes

export function canonicalUnregisterBytes(args: {
  user_id: string;
  timestamp_ms: number;
}): Uint8Array {
  return concatBytes([
    lpString(UNREGISTER_DOMAIN),
    lpString(args.user_id),
    lpString(String(args.timestamp_ms)),
  ]);
}

// ---- replenish ----

export interface ReplenishOpk {
  id: number;
  pub_b64: string;
}
export interface ReplenishSpk {
  pub_b64: string;
  signature_b64: string;
  rotated_at: string;
}

export function canonicalReplenishBytes(args: {
  user_id: string;
  timestamp_ms: number;
  request_id: string;
  spk: ReplenishSpk | null;
  opks: ReplenishOpk[];
}): Uint8Array {
  const parts: Uint8Array[] = [
    lpString(REPLENISH_DOMAIN),
    lpString(args.user_id),
    lpString(String(args.timestamp_ms)),
    lpString(args.request_id),
    u8(args.spk ? 1 : 0),
  ];
  if (args.spk) {
    parts.push(lpString(args.spk.pub_b64));
    parts.push(lpString(args.spk.signature_b64));
    parts.push(lpString(args.spk.rotated_at));
  }
  parts.push(u32be(args.opks.length));
  for (const opk of args.opks) {
    parts.push(u32be(opk.id));
    parts.push(lpString(opk.pub_b64));
  }
  return concatBytes(parts);
}

// ---- SKDM inbox (Phase 6.4) ----
//
// Three operations, each signed by the requester's identity ed25519.
// The freshness window mirrors UNREGISTER_FRESHNESS_WINDOW_MS (5min).
//
// POST: sender claims they're sending a bundle of size N for
// recipient R in scope S at time T. The bundle hash is included so
// a tamper between sig + body fails verify.
// GET / DELETE: the recipient claims authority over their own inbox
// at time T (DELETE also pins a specific row id).

export const CONTROL_INBOX_FRESHNESS_WINDOW_MS = 5 * 60 * 1000;

// ---- authenticated consuming GETs ----
//
// A bearer shipped with every client is not an identity credential.
// These messages are signed by a registered Ed25519 identity and bind
// the actor, intended recipient, destructive-read target and freshness
// timestamp. The prekey target is the recipient user id; the wrapped-key
// target is the content id.

export const CONSUMING_GET_FRESHNESS_WINDOW_MS = 5 * 60 * 1000;
export const WRAPPED_KEY_POST_FRESHNESS_WINDOW_MS = 5 * 60 * 1000;

export interface WrappedKeyPostCanonicalInput {
  content_id: string;
  content_type: string;
  system_message_kind: string | null;
  sender_id: string;
  recipient_id: string;
  session_version: number;
  share_index: number;
  wrapped_share_blob: string;
  blob_version: number;
  single_use: boolean;
  display_duration_seconds: number | null;
  expires_at: string;
  timestamp_ms: number;
}

/**
 * Identity authorization for wrapped-key uploads. Every persisted field and
 * the freshness timestamp are covered so neither the Worker nor an on-path
 * caller can retarget, extend, or alter an opaque share after it is signed.
 * The base64 share is signed as its exact wire string; callers must not
 * decode/re-encode it between signing and serialization.
 */
export function canonicalWrappedKeyPostBytes(
  args: WrappedKeyPostCanonicalInput,
): Uint8Array {
  const parts: Uint8Array[] = [
    lpString(WRAPPED_KEY_POST_DOMAIN),
    lpString(args.content_id),
    lpString(args.content_type),
    lpString(args.system_message_kind ?? ""),
    lpString(args.sender_id),
    lpString(args.recipient_id),
    u32be(args.session_version),
    u32be(args.share_index),
    lpString(args.wrapped_share_blob),
    u32be(args.blob_version),
    u8(args.single_use ? 1 : 0),
    u8(args.display_duration_seconds == null ? 0 : 1),
  ];
  if (args.display_duration_seconds != null) {
    parts.push(u32be(args.display_duration_seconds));
  }
  parts.push(lpString(args.expires_at));
  parts.push(lpString(String(args.timestamp_ms)));
  return concatBytes(parts);
}

export function canonicalPrekeyBundleGetBytes(args: {
  requester_id: string;
  recipient_id: string;
  timestamp_ms: number;
}): Uint8Array {
  return concatBytes([
    lpString(PREKEY_BUNDLE_GET_DOMAIN),
    lpString(args.requester_id),
    lpString(args.recipient_id),
    lpString(args.recipient_id),
    lpString(String(args.timestamp_ms)),
  ]);
}

export function canonicalWrappedKeyGetBytes(args: {
  requester_id: string;
  recipient_id: string;
  content_id: string;
  timestamp_ms: number;
}): Uint8Array {
  return concatBytes([
    lpString(WRAPPED_KEY_GET_DOMAIN),
    lpString(args.requester_id),
    lpString(args.recipient_id),
    lpString(args.content_id),
    lpString(String(args.timestamp_ms)),
  ]);
}

/**
 * Canonical bytes for a control-inbox POST.
 *
 * Wire:
 *   LP(domain) || LP(sender_id) || LP(recipient_id) || LP(scope_id)
 *   || LP(timestamp_ms) || LP(kind) || sha256(bundle) [ || LP(collapse_key) ]
 *
 * `kind` occupies the slot that shipped as `LP("")` and was documented as
 * "reserved for future fields without breaking sig shape". This is that future
 * field: an omitted or empty `kind` reproduces the pre-lane bytes exactly, so
 * every deployed client keeps verifying, while a `revocation` row's lane is a
 * **signed** component.
 *
 * That signing is load-bearing, in both directions:
 *
 * - An attacker cannot strip `kind: "revocation"` in transit to demote the row
 *   into the evictable ordinary lane, which would silently destroy a burn.
 * - An attacker cannot add it to a stranger's ordinary message to jump the
 *   non-evictable lane.
 *
 * `collapse_key` is an optional trailing length-prefixed component -- the same
 * shape (and the same reasoning) as the `sender_id` filter on the GET. It is an
 * opaque client-computed MAC over (scope commitment, burn epoch); this server
 * never learns either. Appending after the fixed-length digest is unambiguous.
 */
export function canonicalControlInboxPostBytes(args: {
  sender_id: string;
  recipient_id: string;
  scope_id: string;
  timestamp_ms: number;
  bundle_sha256: Uint8Array;
  kind?: string | null;
  collapse_key?: string | null;
}): Uint8Array {
  const parts = [
    lpString(CONTROL_INBOX_POST_DOMAIN),
    lpString(args.sender_id),
    lpString(args.recipient_id),
    lpString(args.scope_id),
    lpString(String(args.timestamp_ms)),
    lpString(args.kind ?? ""),
    args.bundle_sha256,
  ];
  if (args.collapse_key !== undefined && args.collapse_key !== null) {
    parts.push(lpString(args.collapse_key));
  }
  return concatBytes(parts);
}

/**
 * Canonical bytes for the inbox drain.
 *
 * `sender_id` is the optional per-sender filter (see
 * `handleControlInboxGet`). It is a **signed** component, appended only
 * when the caller asked for a filtered drain — exactly the
 * optional-component shape `buildRegMsg` uses, and for the same two
 * reasons:
 *
 * - A caller that did not ask for a filter produces the byte-identical
 *   pre-filter message, so every deployed client keeps working.
 * - Adding, removing or altering `?sender=` in transit changes the
 *   server's reconstruction, so the signature stops verifying. In
 *   particular an attacker **cannot strip the filter** to silently
 *   restore the head-of-line starvation the filter exists to fix: that
 *   request is refused, not quietly served unfiltered.
 *
 * Unambiguity: the unfiltered form ends after the timestamp; the
 * filtered form appends one more length-prefixed component. A zero
 * length prefix is not producible, because an empty `?sender=` is
 * rejected by `isProtocolId` before it ever reaches here.
 */
export function canonicalControlInboxGetBytes(args: {
  user_id: string;
  timestamp_ms: number;
  sender_id?: string | null;
}): Uint8Array {
  const parts = [
    lpString(CONTROL_INBOX_GET_DOMAIN),
    lpString(args.user_id),
    lpString(String(args.timestamp_ms)),
  ];
  if (args.sender_id !== undefined && args.sender_id !== null) {
    parts.push(lpString(args.sender_id));
  }
  return concatBytes(parts);
}

export function canonicalControlInboxDeleteBytes(args: {
  user_id: string;
  inbox_id_hex: string;
  timestamp_ms: number;
}): Uint8Array {
  return concatBytes([
    lpString(CONTROL_INBOX_DELETE_DOMAIN),
    lpString(args.user_id),
    lpString(args.inbox_id_hex),
    lpString(String(args.timestamp_ms)),
  ]);
}
