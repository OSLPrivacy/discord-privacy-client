import { describe, expect, it } from "vitest";
import { canonicalJson, mailSignedMessage } from "../../src/mail/protocol.js";
import { encryptExternalMime } from "../../src/mail/external-envelope.js";
import { base64Decode, base64Encode } from "../../src/mail/protocol.js";

describe("OSL Mail protocol", () => {
  it("canonicalizes signed request fields and excludes only the signature", () => {
    const a = mailSignedMessage("LIST", {
      timestamp_ms: 123,
      signature_b64: "ignored",
      request_id: "request",
      user_id: "user",
      limit: 25,
    });
    const b = mailSignedMessage("LIST", {
      limit: 25,
      user_id: "user",
      request_id: "request",
      timestamp_ms: 123,
    });
    expect(a).toEqual(b);
    expect(new TextDecoder().decode(a)).not.toContain("ignored");
    expect(canonicalJson({ z: 1, a: [true, "x"] })).toBe('{"a":[true,"x"],"z":1}');
  });

  it("envelope-encrypts external MIME for only the recipient key", async () => {
    const recipient = await crypto.subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]) as CryptoKeyPair;
    const exported = await crypto.subtle.exportKey("raw", recipient.publicKey);
    expect(exported).toBeInstanceOf(ArrayBuffer);
    const raw = new TextEncoder().encode("Subject: private\r\n\r\nsecret body");
    const encrypted = await encryptExternalMime(raw, base64Encode(new Uint8Array(exported as ArrayBuffer)), "user-a", "mail-a", 123456);
    expect(encrypted.ciphertextB64).not.toContain("secret");
    expect(raw.every((byte) => byte === 0)).toBe(true);

    const envelope = encrypted.envelope;
    const ephemeral = await crypto.subtle.importKey("raw", base64Decode(envelope.ephemeral_public_key_b64), { name: "X25519" }, false, []);
    const shared = await crypto.subtle.deriveBits(
      { name: "X25519", public: ephemeral } as unknown as SubtleCryptoDeriveKeyAlgorithm,
      recipient.privateKey,
      256,
    );
    const hkdf = await crypto.subtle.importKey("raw", shared, "HKDF", false, ["deriveKey"]);
    const key = await crypto.subtle.deriveKey({
      name: "HKDF",
      hash: "SHA-256",
      salt: base64Decode(envelope.salt_b64),
      info: new TextEncoder().encode("OSL external inbound v1"),
    }, hkdf, { name: "AES-GCM", length: 256 }, false, ["decrypt"]);
    const plaintext = await crypto.subtle.decrypt({
      name: "AES-GCM",
      iv: base64Decode(envelope.nonce_b64),
      additionalData: base64Decode(envelope.aad_b64),
      tagLength: 128,
    }, key, base64Decode(encrypted.ciphertextB64));
    expect(new TextDecoder().decode(plaintext)).toContain("secret body");
  });
});
