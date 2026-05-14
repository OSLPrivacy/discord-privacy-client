import { describe, expect, it } from "vitest";
import {
  canonicalBurnBytes,
  canonicalReplenishBytes,
} from "../../src/lib/canonical.js";

// Helper: hex-print Uint8Array for byte-equality assertions.
function hex(b: Uint8Array): string {
  return Array.from(b)
    .map((x) => x.toString(16).padStart(2, "0"))
    .join("");
}

describe("canonicalBurnBytes", () => {
  it("encodes scope=single with target_kind=1 and content_id LP", () => {
    const out = canonicalBurnBytes({
      user_id: "alice",
      scope: "single",
      target: { content_id: "msg-1" },
    });
    // Layout: LP(domain) || LP("alice") || LP("single") || u8(1) || LP("msg-1")
    // domain = "discord-privacy-client/burn/v1" (30 bytes)
    expect(hex(out)).toMatch(/^0000001e/); // u32(30) prefix on domain
    expect(out.indexOf(0x01)).toBeGreaterThan(0); // target_kind=1 byte present
  });

  it("encodes scope=to_user with target_kind=2", () => {
    const out = canonicalBurnBytes({
      user_id: "alice",
      scope: "to_user",
      target: { user_id: "bob" },
    });
    // u8(2) target_kind appears in the byte stream
    expect(Array.from(out)).toContain(0x02);
  });

  it("encodes scope=all with target_kind=0 and no target value", () => {
    const out = canonicalBurnBytes({ user_id: "alice", scope: "all" });
    // Last byte of the encoding is the target_kind=0 byte (no LP follows).
    expect(out[out.length - 1]).toBe(0x00);
  });

  it("differs across users / scopes / targets", () => {
    const a = canonicalBurnBytes({
      user_id: "alice",
      scope: "single",
      target: { content_id: "msg-1" },
    });
    const b = canonicalBurnBytes({
      user_id: "bob",
      scope: "single",
      target: { content_id: "msg-1" },
    });
    const c = canonicalBurnBytes({
      user_id: "alice",
      scope: "single",
      target: { content_id: "msg-2" },
    });
    expect(hex(a)).not.toBe(hex(b));
    expect(hex(a)).not.toBe(hex(c));
  });

  it("throws when scope=single missing content_id", () => {
    expect(() =>
      canonicalBurnBytes({ user_id: "alice", scope: "single" }),
    ).toThrow();
  });
});

describe("canonicalReplenishBytes", () => {
  it("encodes spk_present=0 when spk is null", () => {
    const out = canonicalReplenishBytes({
      user_id: "alice",
      spk: null,
      opks: [],
    });
    // Stream ends with: u8(0) spk_present || u32(0) opk_count
    expect(out.slice(out.length - 5)).toEqual(new Uint8Array([0, 0, 0, 0, 0]));
  });

  it("encodes spk_present=1 with SPK base64 STRING (not decoded bytes)", () => {
    // Single-char b64 strings make the byte layout easy to inspect:
    //   ...u8(1) || LP("AA==") || LP("BB==") || LP("2024-01-01T00:00:00Z") || u32(0)
    // LP("AA==") = u32(4) || "AA=="
    const out = canonicalReplenishBytes({
      user_id: "alice",
      spk: { pub_b64: "AA==", signature_b64: "BB==", rotated_at: "2024" },
      opks: [],
    });
    const slice = Array.from(out);
    // The literal ASCII bytes for "AA==" should be present (0x41 0x41 0x3d 0x3d).
    const idxAA = sliceIndexOf(slice, [0x41, 0x41, 0x3d, 0x3d]);
    expect(idxAA).toBeGreaterThan(0);
  });

  it("encodes multiple OPKs with u32 id + LP(pub_b64 string)", () => {
    const out = canonicalReplenishBytes({
      user_id: "alice",
      spk: null,
      opks: [
        { id: 1, pub_b64: "AAAA" },
        { id: 2, pub_b64: "BBBB" },
      ],
    });
    // u32 count == 2 should appear before the opk entries.
    const slice = Array.from(out);
    const idx2 = sliceIndexOf(slice, [0x00, 0x00, 0x00, 0x02]); // u32(2)
    expect(idx2).toBeGreaterThan(0);
  });
});

function sliceIndexOf(haystack: number[], needle: number[]): number {
  outer: for (let i = 0; i <= haystack.length - needle.length; i++) {
    for (let j = 0; j < needle.length; j++) {
      if (haystack[i + j] !== needle[j]) continue outer;
    }
    return i;
  }
  return -1;
}
