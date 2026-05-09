// Canonical byte encoding for things the server signs / verifies.
//
// The Ed25519 signature on the replenish batch must be verifiable on
// both sides (Rust client + Node server) against the exact same byte
// string. Rather than wrestle with JSON canonicalisation, we define
// our own length-prefixed encoding that both sides construct
// deterministically.

import { createPublicKey, verify as cryptoVerify } from 'node:crypto';

const REPLENISH_DOMAIN = 'discord-privacy-client/prekey-replenish/v1';
const BURN_DOMAIN = 'discord-privacy-client/burn/v1';
const MANIFEST_DOMAIN = 'discord-privacy-client/selector-manifest/v1';

// length-prefix helper: u32 BE length || bytes.
function lpString(buf, s) {
  const bytes = Buffer.from(s, 'utf8');
  const len = Buffer.alloc(4);
  len.writeUInt32BE(bytes.length);
  buf.push(len);
  buf.push(bytes);
}

function lpBytes(buf, bytes) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(bytes.length);
  buf.push(len);
  buf.push(bytes);
}

function u8(buf, v) {
  buf.push(Buffer.from([v]));
}

function u32(buf, v) {
  const out = Buffer.alloc(4);
  out.writeUInt32BE(v);
  buf.push(out);
}

// Canonical encoding per the design doc / wire spec:
//
//   domain_label (LP)
//   user_id (LP)
//   spk_present (u8: 0 | 1)
//   if spk_present:
//     spk.pub_b64 string (LP — the base64 string, NOT decoded)
//     spk.signature_b64 string (LP)
//     spk.rotated_at string (LP)
//   opk_count (u32 BE)
//   per opk: u32 BE id, LP pub_b64 string
//
// The base64-string-not-bytes form is intentional: lets either side
// construct the encoding without round-tripping through base64
// decode.
export function canonicalReplenishBytes({ user_id, spk, opks }) {
  const buf = [];
  lpString(buf, REPLENISH_DOMAIN);
  lpString(buf, user_id);
  u8(buf, spk ? 1 : 0);
  if (spk) {
    lpString(buf, spk.pub_b64);
    lpString(buf, spk.signature_b64);
    lpString(buf, spk.rotated_at);
  }
  u32(buf, opks.length);
  for (const o of opks) {
    u32(buf, o.id);
    lpString(buf, o.pub_b64);
  }
  return Buffer.concat(buf);
}

// Canonical encoding of a burn request. Both Rust client and Node
// server reconstruct identical bytes for Ed25519 verification.
//
//   domain (LP)
//   user_id (LP)
//   scope_str (LP, "single" | "to_user" | "all")
//   target_kind (u8: 0 = none, 1 = content_id, 2 = user_id)
//   if target_kind != 0: target_value (LP)
export function canonicalBurnBytes({ user_id, scope, target }) {
  const buf = [];
  lpString(buf, BURN_DOMAIN);
  lpString(buf, user_id);
  lpString(buf, scope);
  if (scope === 'single') {
    u8(buf, 1);
    lpString(buf, target.content_id);
  } else if (scope === 'to_user') {
    u8(buf, 2);
    lpString(buf, target.user_id);
  } else {
    u8(buf, 0);
  }
  return Buffer.concat(buf);
}

// Canonical encoding of a selector manifest. Mirrors
// `crates/selectors/src/manifest.rs` `canonical_manifest_bytes`.
//
//   domain (LP)
//   version (u32 BE)
//   issued_at_unix_seconds (u64 BE)
//   client_min_version (LP)
//   selector_count (u32 BE)
//   per selector (sorted by key, lex):
//     key (LP)
//     value (LP)
//
// `manifest` is `{ version, issued_at_unix_seconds, client_min_version,
// selectors: Record<string, string> }`.
export function canonicalManifestBytes(manifest) {
  const buf = [];
  lpString(buf, MANIFEST_DOMAIN);
  u32(buf, manifest.version);
  // Node's u64-BE: 8 bytes manually (Buffer.writeBigUInt64BE).
  const issuedBuf = Buffer.alloc(8);
  issuedBuf.writeBigUInt64BE(BigInt(manifest.issued_at_unix_seconds));
  buf.push(issuedBuf);
  lpString(buf, manifest.client_min_version);
  const entries = Object.entries(manifest.selectors).sort(([a], [b]) =>
    a < b ? -1 : a > b ? 1 : 0,
  );
  u32(buf, entries.length);
  for (const [k, v] of entries) {
    lpString(buf, k);
    lpString(buf, v);
  }
  return Buffer.concat(buf);
}

// Verify an Ed25519 signature given a raw 32-byte public key.
// Wraps the raw bytes in the SPKI DER prefix so Node's crypto.verify
// accepts them.
const ED25519_SPKI_PREFIX = Buffer.from('302a300506032b6570032100', 'hex');

export function verifyEd25519(pubKeyRaw32, message, signature64) {
  if (pubKeyRaw32.length !== 32) {
    return false;
  }
  if (signature64.length !== 64) {
    return false;
  }
  const der = Buffer.concat([ED25519_SPKI_PREFIX, pubKeyRaw32]);
  const pubKey = createPublicKey({
    key: der,
    format: 'der',
    type: 'spki',
  });
  return cryptoVerify(null, message, pubKey, signature64);
}
