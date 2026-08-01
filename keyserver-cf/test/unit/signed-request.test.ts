import { describe, expect, it } from "vitest";
import {
  buildRegMsg,
  buildRotMsg,
  ed25519SelfTest,
  parseRnCapabilities,
  RN_CAP_WIRE_RN,
  RN_CAP_WIRE_RN_LIVE,
  verifySignedRequest,
} from "../../src/lib/signed-request.js";

const dec = new TextDecoder();

describe("RN capability allocation", () => {
  it("reserves bit 1 for a fuse-open OSL-RN build", () => {
    expect(RN_CAP_WIRE_RN).toBe(1);
    expect(RN_CAP_WIRE_RN_LIVE).toBe(2);
    expect(parseRnCapabilities(RN_CAP_WIRE_RN)).toEqual({ present: true, value: 1 });
    expect(parseRnCapabilities(RN_CAP_WIRE_RN | RN_CAP_WIRE_RN_LIVE)).toEqual({
      present: true,
      value: 3,
    });
  });
});

describe("REG_MSG / ROT_MSG byte format (MIRRORED with client.rs)", () => {
  // This EXACT vector is pinned in
  // crates/keystore/tests/client_test.rs
  // `reg_msg_byte_format_is_pinned_and_mirrored`. If these two
  // disagree by one byte, every registration fails.
  it("REG_MSG matches the pinned client vector (with ratchet)", () => {
    const bytes = buildRegMsg({
      user_id: "900000000000000001",
      ik_x25519_pub: "WdsAAA==",
      ik_ed25519_pub: "ZWQyNTUx",
      ik_mlkem768_pub: "bWxrZW0=",
      ik_ratchet_initial_pub: "cmF0Y2g=",
    });
    expect(dec.decode(bytes)).toBe(
      "OSL-REGISTER-v1\n900000000000000001\nWdsAAA==\nZWQyNTUx\nbWxrZW0=\ncmF0Y2g=",
    );
  });

  it("REG_MSG with absent ratchet ends with empty component (no trailing NL)", () => {
    expect(
      dec.decode(
        buildRegMsg({
          user_id: "u",
          ik_x25519_pub: "x",
          ik_ed25519_pub: "e",
          ik_mlkem768_pub: "m",
          ik_ratchet_initial_pub: null,
        }),
      ),
    ).toBe("OSL-REGISTER-v1\nu\nx\ne\nm\n");
  });

  // Mirrored with
  // crates/keystore/tests/client_test.rs
  // `reg_msg_with_capabilities_byte_format_is_pinned_and_mirrored`.
  it("REG_MSG with a capability bitmap matches the pinned client vector", () => {
    const bytes = buildRegMsg({
      user_id: "900000000000000001",
      ik_x25519_pub: "WdsAAA==",
      ik_ed25519_pub: "ZWQyNTUx",
      ik_mlkem768_pub: "bWxrZW0=",
      ik_ratchet_initial_pub: "cmF0Y2g=",
      rn_capabilities: 1,
    });
    expect(dec.decode(bytes)).toBe(
      "OSL-REGISTER-v1\n900000000000000001\nWdsAAA==\nZWQyNTUx\nbWxrZW0=\ncmF0Y2g=\n1",
    );
  });

  it("an absent bitmap reproduces the legacy bytes exactly; a present 0 does not", () => {
    const base = {
      user_id: "u",
      ik_x25519_pub: "x",
      ik_ed25519_pub: "e",
      ik_mlkem768_pub: "m",
      ik_ratchet_initial_pub: null,
    };
    // undefined and null both mean "the client did not send the field",
    // which must reproduce the message every deployed client signs.
    expect(dec.decode(buildRegMsg(base))).toBe("OSL-REGISTER-v1\nu\nx\ne\nm\n");
    expect(dec.decode(buildRegMsg({ ...base, rn_capabilities: null }))).toBe(
      "OSL-REGISTER-v1\nu\nx\ne\nm\n",
    );
    // An explicit zero is a DIFFERENT message. That asymmetry is what
    // makes stripping the field a signature failure rather than a
    // silent downgrade to "no capability".
    expect(dec.decode(buildRegMsg({ ...base, rn_capabilities: 0 }))).toBe(
      "OSL-REGISTER-v1\nu\nx\ne\nm\n\n0",
    );
  });

  it("ROT_MSG carries the bitmap when present and is unchanged when absent", () => {
    const base = {
      user_id: "alice",
      prev_ik_ed25519_pub: "OLD=",
      new_ik_x25519_pub: "NX=",
      new_ik_ed25519_pub: "NE=",
      new_ik_mlkem768_pub: "NM=",
      new_ik_ratchet_initial_pub: null,
    };
    expect(dec.decode(buildRotMsg(base))).toBe(
      "OSL-ROTATE-v1\nalice\nOLD=\nNX=\nNE=\nNM=\n",
    );
    expect(dec.decode(buildRotMsg({ ...base, rn_capabilities: 1 }))).toBe(
      "OSL-ROTATE-v1\nalice\nOLD=\nNX=\nNE=\nNM=\n\n1",
    );
  });

  it("ROT_MSG matches the pinned client vector", () => {
    const bytes = buildRotMsg({
      user_id: "alice",
      prev_ik_ed25519_pub: "OLD=",
      new_ik_x25519_pub: "NX=",
      new_ik_ed25519_pub: "NE=",
      new_ik_mlkem768_pub: "NM=",
      new_ik_ratchet_initial_pub: null,
    });
    expect(dec.decode(bytes)).toBe(
      "OSL-ROTATE-v1\nalice\nOLD=\nNX=\nNE=\nNM=\n",
    );
  });
});

describe("verifySignedRequest + self-test (WebCrypto Ed25519)", () => {
  it("passes the RFC 8032 self-test vector", async () => {
    await expect(ed25519SelfTest()).resolves.toBeUndefined();
  });

  it("verifies a real signature and rejects tampering", async () => {
    const pair = (await crypto.subtle.generateKey(
      { name: "Ed25519" },
      true,
      ["sign", "verify"],
    )) as CryptoKeyPair;
    const pubRaw = new Uint8Array(
      (await crypto.subtle.exportKey("raw", pair.publicKey)) as ArrayBuffer,
    );
    const b64 = (b: Uint8Array) => {
      let s = "";
      for (const x of b) s += String.fromCharCode(x);
      return btoa(s);
    };
    const msg = buildRegMsg({
      user_id: "bob",
      ik_x25519_pub: "AAAA",
      ik_ed25519_pub: b64(pubRaw),
      ik_mlkem768_pub: "BBBB",
      ik_ratchet_initial_pub: null,
    });
    const sig = new Uint8Array(
      await crypto.subtle.sign({ name: "Ed25519" }, pair.privateKey, msg),
    );
    expect(await verifySignedRequest(b64(pubRaw), msg, b64(sig))).toBe(true);
    // tampered message
    const bad = buildRegMsg({
      user_id: "bob",
      ik_x25519_pub: "ZZZZ",
      ik_ed25519_pub: b64(pubRaw),
      ik_mlkem768_pub: "BBBB",
      ik_ratchet_initial_pub: null,
    });
    expect(await verifySignedRequest(b64(pubRaw), bad, b64(sig))).toBe(false);
    // wrong-length / garbage inputs never throw, return false
    expect(await verifySignedRequest("not b64!", msg, b64(sig))).toBe(false);
    expect(await verifySignedRequest(b64(pubRaw), msg, "AAAA")).toBe(false);
  });
});
