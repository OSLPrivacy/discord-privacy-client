/**
 * Standalone keyserver wire fixture.
 *
 * These literals intentionally do not import the cipher-store verifier's
 * constants. The interoperability test must fail if either side's wire
 * contract drifts. This mirrors the keyserver issuer's canonical three-claim
 * payload and Ed25519 domain separation without reaching outside this
 * project's test closure.
 */
export const ISSUER_GRANT_SCHEME = "OSL-Link-Grant";
export const ISSUER_GRANT_AUDIENCE = "osl-link-create";
export const ISSUER_GRANT_DOMAIN = "OSL-LINK-GRANT-v1";
export const ISSUER_GRANT_LIFETIME_SECONDS = 300;

function base64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function freshJti(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  let value = "";
  for (const byte of bytes) value += byte.toString(16).padStart(2, "0");
  return value;
}

export interface KeyserverGrantFixture {
  authorization: string;
  expiresAt: number;
  payload: string;
}

export async function mintKeyserverGrantFixture(
  signingKey: CryptoKey,
  nowSeconds: number,
): Promise<KeyserverGrantFixture> {
  const expiresAt = nowSeconds + ISSUER_GRANT_LIFETIME_SECONDS;
  const jti = freshJti();
  const payload =
    `{"aud":"${ISSUER_GRANT_AUDIENCE}","exp":${expiresAt},"jti":"${jti}"}`;
  const payloadBytes = new TextEncoder().encode(payload);
  const domain = new TextEncoder().encode(ISSUER_GRANT_DOMAIN);
  const signingBytes = new Uint8Array(domain.byteLength + 1 + payloadBytes.byteLength);
  signingBytes.set(domain, 0);
  signingBytes[domain.byteLength] = 0;
  signingBytes.set(payloadBytes, domain.byteLength + 1);
  const signature = new Uint8Array(
    await crypto.subtle.sign({ name: "Ed25519" }, signingKey, signingBytes),
  );
  return {
    authorization:
      `${ISSUER_GRANT_SCHEME} ${base64Url(payloadBytes)}.${base64Url(signature)}`,
    expiresAt,
    payload,
  };
}
