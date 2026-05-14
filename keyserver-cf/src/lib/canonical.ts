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
///     LP(scope_str: "single" | "to_user" | "all")
///     target_kind: u8  (0 = none / all, 1 = content_id, 2 = user_id)
///     if target_kind != 0: LP(target_value)
///
///   Replenish:
///     LP(domain)
///     LP(user_id)
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
  scope: BurnScope;
  target?: BurnTarget;
}): Uint8Array {
  const parts: Uint8Array[] = [
    lpString(BURN_DOMAIN),
    lpString(args.user_id),
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
  spk: ReplenishSpk | null;
  opks: ReplenishOpk[];
}): Uint8Array {
  const parts: Uint8Array[] = [
    lpString(REPLENISH_DOMAIN),
    lpString(args.user_id),
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
