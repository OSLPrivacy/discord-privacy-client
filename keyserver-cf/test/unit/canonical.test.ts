import { describe, expect, it } from "vitest";
import {
  canonicalBurnBytes,
  canonicalPrekeyBundleGetBytes,
  canonicalReplenishBytes,
  canonicalWrappedKeyGetBytes,
  canonicalWrappedKeyPostBytes,
} from "../../src/lib/canonical.js";

// Helper: hex-print Uint8Array for byte-equality assertions.
function hex(b: Uint8Array): string {
  return Array.from(b)
    .map((x) => x.toString(16).padStart(2, "0"))
    .join("");
}

const signedCommand = {
  timestamp_ms: 1_700_000_000_123,
  request_id: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
};

describe("canonicalBurnBytes", () => {
  it("encodes scope=single with target_kind=1 and content_id LP", () => {
    const out = canonicalBurnBytes({
      user_id: "alice",
      ...signedCommand,
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
      ...signedCommand,
      scope: "to_user",
      target: { user_id: "bob" },
    });
    // u8(2) target_kind appears in the byte stream
    expect(Array.from(out)).toContain(0x02);
  });

  it("encodes scope=all with target_kind=0 and no target value", () => {
    const out = canonicalBurnBytes({ user_id: "alice", ...signedCommand, scope: "all" });
    // Last byte of the encoding is the target_kind=0 byte (no LP follows).
    expect(out[out.length - 1]).toBe(0x00);
  });

  it("differs across users / scopes / targets", () => {
    const a = canonicalBurnBytes({
      user_id: "alice",
      ...signedCommand,
      scope: "single",
      target: { content_id: "msg-1" },
    });
    const b = canonicalBurnBytes({
      user_id: "bob",
      ...signedCommand,
      scope: "single",
      target: { content_id: "msg-1" },
    });
    const c = canonicalBurnBytes({
      user_id: "alice",
      ...signedCommand,
      scope: "single",
      target: { content_id: "msg-2" },
    });
    expect(hex(a)).not.toBe(hex(b));
    expect(hex(a)).not.toBe(hex(c));
  });

  it("throws when scope=single missing content_id", () => {
    expect(() =>
      canonicalBurnBytes({ user_id: "alice", ...signedCommand, scope: "single" }),
    ).toThrow();
  });
});

describe("canonicalReplenishBytes", () => {
  it("encodes spk_present=0 when spk is null", () => {
    const out = canonicalReplenishBytes({
      user_id: "alice",
      ...signedCommand,
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
      ...signedCommand,
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
      ...signedCommand,
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

  it("rejects OPK ids that would alias after u32 truncation", () => {
    expect(() =>
      canonicalReplenishBytes({
        user_id: "alice",
        ...signedCommand,
        spk: null,
        opks: [{ id: 0x1_0000_0000, pub_b64: "AAAA" }],
      }),
    ).toThrow(/u32 value out of range/);
  });
});

describe("canonicalWrappedKeyPostBytes u32 bounds", () => {
  const base = {
    content_id: "content",
    content_type: "text",
    system_message_kind: null,
    sender_id: "alice",
    recipient_id: "bob",
    session_version: 1,
    share_index: 0,
    wrapped_share_blob: "AQ==",
    blob_version: 1,
    single_use: true,
    display_duration_seconds: 10,
    expires_at: "2026-01-01T00:00:00.000Z",
    timestamp_ms: 1_700_000_000_123,
  } as const;

  for (const field of [
    "session_version",
    "share_index",
    "blob_version",
    "display_duration_seconds",
  ] as const) {
    it(`rejects out-of-range ${field}`, () => {
      expect(() =>
        canonicalWrappedKeyPostBytes({ ...base, [field]: 0x1_0000_0000 }),
      ).toThrow(/u32 value out of range/);
    });
  }
});

describe("authenticated consuming GET canonical bytes", () => {
  it("binds prekey requester, recipient/target and timestamp", () => {
    const base = canonicalPrekeyBundleGetBytes({
      requester_id: "alice",
      recipient_id: "bob",
      timestamp_ms: 1_700_000_000_123,
    });
    expect(hex(base)).toBe(
      "0000002b646973636f72642d707269766163792d636c69656e742f7072656b65792d62756e646c652d6765742f7631" +
        "00000005616c69636500000003626f6200000003626f620000000d31373030303030303030313233",
    );
    expect(hex(base)).not.toBe(
      hex(
        canonicalPrekeyBundleGetBytes({
          requester_id: "mallory",
          recipient_id: "bob",
          timestamp_ms: 1_700_000_000_123,
        }),
      ),
    );
    expect(hex(base)).not.toBe(
      hex(
        canonicalPrekeyBundleGetBytes({
          requester_id: "alice",
          recipient_id: "carol",
          timestamp_ms: 1_700_000_000_123,
        }),
      ),
    );
  });

  it("binds wrapped-key requester, recipient, content target and timestamp", () => {
    const base = canonicalWrappedKeyGetBytes({
      requester_id: "bob",
      recipient_id: "bob",
      content_id: "message-1",
      timestamp_ms: 1_700_000_000_123,
    });
    const wrongTarget = canonicalWrappedKeyGetBytes({
      requester_id: "bob",
      recipient_id: "bob",
      content_id: "message-2",
      timestamp_ms: 1_700_000_000_123,
    });
    expect(hex(base)).toBe(
      "00000029646973636f72642d707269766163792d636c69656e742f777261707065642d6b65792d6765742f7631" +
        "00000003626f6200000003626f62000000096d6573736167652d310000000d31373030303030303030313233",
    );
    expect(hex(base)).not.toBe(hex(wrongTarget));
  });
});

describe("canonicalWrappedKeyPostBytes", () => {
  const base = {
    content_id: "message-1",
    content_type: "text",
    system_message_kind: null,
    sender_id: "alice",
    recipient_id: "bob",
    session_version: 1,
    share_index: 0,
    wrapped_share_blob: "AQIDBA==",
    blob_version: 1,
    single_use: false,
    display_duration_seconds: null,
    expires_at: "2026-07-18T00:00:00.000Z",
    timestamp_ms: 1_700_000_000_123,
  };

  it("binds every persisted field and the freshness timestamp", () => {
    const encoded = hex(canonicalWrappedKeyPostBytes(base));
    // Mirrored by crates/keystore/src/wrapped_key.rs. Any byte drift breaks
    // authorization for every public desktop client.
    expect(encoded).toBe(
      "0000002a646973636f72642d707269766163792d636c69656e742f777261707065642d6b65792d706f73742f7631" +
        "000000096d6573736167652d3100000004746578740000000000000005616c69636500000003626f62" +
        "0000000100000000000000084151494442413d3d00000001000000000018323032362d30372d31385430303a30303a30302e3030305a" +
        "0000000d31373030303030303030313233",
    );
    for (const [field, changed] of [
      ["content_id", "message-2"],
      ["recipient_id", "carol"],
      ["wrapped_share_blob", "BQYHCA=="],
      ["expires_at", "2026-07-19T00:00:00.000Z"],
      ["timestamp_ms", 1_700_000_000_124],
    ] as const) {
      expect(
        hex(canonicalWrappedKeyPostBytes({ ...base, [field]: changed })),
      ).not.toBe(encoded);
    }
  });

  it("distinguishes absent and present display duration", () => {
    expect(hex(canonicalWrappedKeyPostBytes(base))).not.toBe(
      hex(
        canonicalWrappedKeyPostBytes({
          ...base,
          single_use: true,
          display_duration_seconds: 10,
        }),
      ),
    );
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
