// Canonical byte encoding for things the server signs / verifies.
//
// The Ed25519 signature on the replenish batch must be verifiable on
// both sides (Rust client + Node server) against the exact same byte
// string. Rather than wrestle with JSON canonicalisation, we define
// our own length-prefixed encoding that both sides construct
// deterministically.

import { createPublicKey, verify as cryptoVerify } from 'node:crypto';

const REPLENISH_DOMAIN = 'discord-privacy-client/prekey-replenish/v1';

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
