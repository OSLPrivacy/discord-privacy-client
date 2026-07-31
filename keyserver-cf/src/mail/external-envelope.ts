import { base64Decode, base64Encode } from "./protocol.js";

export interface ExternalEnvelope {
  version: 1;
  algorithm: "X25519-HKDF-SHA256-AES-256-GCM";
  ephemeral_public_key_b64: string;
  salt_b64: string;
  nonce_b64: string;
  aad_b64: string;
}

export async function encryptExternalMime(
  rawMime: Uint8Array,
  recipientPublicKeyB64: string,
  recipientUserId: string,
  messageId: string,
  expiresAt: number,
): Promise<{ ciphertextB64: string; envelope: ExternalEnvelope; keyFingerprint: string }> {
  const recipientRaw = base64Decode(recipientPublicKeyB64);
  if (recipientRaw.byteLength !== 32) throw new Error("recipient X25519 key invalid");
  const recipientKey = await crypto.subtle.importKey("raw", recipientRaw, { name: "X25519" }, false, []);
  const ephemeral = await crypto.subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]) as CryptoKeyPair;
  const shared = new Uint8Array(await crypto.subtle.deriveBits(
    { name: "X25519", public: recipientKey } as unknown as SubtleCryptoDeriveKeyAlgorithm,
    ephemeral.privateKey,
    256,
  ));
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const nonce = crypto.getRandomValues(new Uint8Array(12));
  const hkdf = await crypto.subtle.importKey("raw", shared, "HKDF", false, ["deriveKey"]);
  const aes = await crypto.subtle.deriveKey(
    { name: "HKDF", hash: "SHA-256", salt, info: new TextEncoder().encode("OSL external inbound v1") },
    hkdf,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt"],
  );
  const aad = new TextEncoder().encode(`OSL-MAIL-EXTERNAL-v1\n${recipientUserId}\n${messageId}\n${expiresAt}\n`);
  const ciphertext = new Uint8Array(await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: nonce, additionalData: aad, tagLength: 128 },
    aes,
    rawMime,
  ));
  const exported = await crypto.subtle.exportKey("raw", ephemeral.publicKey);
  if (!(exported instanceof ArrayBuffer)) throw new Error("X25519 raw export failed");
  const ephemeralPublic = new Uint8Array(exported);
  const fingerprint = new Uint8Array(await crypto.subtle.digest("SHA-256", recipientRaw));
  shared.fill(0);
  rawMime.fill(0);
  return {
    ciphertextB64: base64Encode(ciphertext),
    keyFingerprint: base64Encode(fingerprint),
    envelope: {
      version: 1,
      algorithm: "X25519-HKDF-SHA256-AES-256-GCM",
      ephemeral_public_key_b64: base64Encode(ephemeralPublic),
      salt_b64: base64Encode(salt),
      nonce_b64: base64Encode(nonce),
      aad_b64: base64Encode(aad),
    },
  };
}
