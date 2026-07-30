const TEXT_ENCODER = new TextEncoder();
export const ACCOUNT_OWNERSHIP_CHALLENGE_VERSION = 1;
export const ACCOUNT_OWNERSHIP_CHALLENGE_TTL_SECONDS = 5 * 60;
export const ACCOUNT_OWNERSHIP_CHALLENGE_NONCE_BYTES = 32;

export interface IssuedAccountOwnershipChallenge {
  challenge_version: 1;
  service: "discord";
  service_account_id: string;
  owner_user_id: string;
  nonce: string;
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
  spent: false;
}

export interface StoredAccountOwnershipChallenge {
  response: IssuedAccountOwnershipChallenge;
  nonceSha256: string;
  bindingSha256: string;
}

export async function issueDiscordAccountOwnershipChallenge(
  serviceAccountId: string,
  ownerUserId: string,
  nowUnixSeconds = Math.floor(Date.now() / 1000),
): Promise<StoredAccountOwnershipChallenge> {
  const nonceBytes = new Uint8Array(ACCOUNT_OWNERSHIP_CHALLENGE_NONCE_BYTES);
  crypto.getRandomValues(nonceBytes);
  const nonce = base64(nonceBytes);
  const expiresAt = nowUnixSeconds + ACCOUNT_OWNERSHIP_CHALLENGE_TTL_SECONDS;
  const response: IssuedAccountOwnershipChallenge = {
    challenge_version: ACCOUNT_OWNERSHIP_CHALLENGE_VERSION,
    service: "discord",
    service_account_id: serviceAccountId,
    owner_user_id: ownerUserId,
    nonce,
    issued_at_unix_seconds: nowUnixSeconds,
    expires_at_unix_seconds: expiresAt,
    spent: false,
  };
  const [nonceSha256, bindingSha256] = await Promise.all([
    sha256Hex(nonceBytes),
    sha256Hex(canonicalChallengeBindingBytes(response)),
  ]);
  return { response, nonceSha256, bindingSha256 };
}

export function canonicalChallengeBindingBytes(
  challenge: IssuedAccountOwnershipChallenge,
): Uint8Array {
  return TEXT_ENCODER.encode(
    [
      "osl.account-ownership-challenge.v1",
      `service=${challenge.service}`,
      `service_account_id=${challenge.service_account_id}`,
      `owner_user_id=${challenge.owner_user_id}`,
      `nonce=${challenge.nonce}`,
      `issued_at_unix_seconds=${challenge.issued_at_unix_seconds}`,
      `expires_at_unix_seconds=${challenge.expires_at_unix_seconds}`,
    ].join("\n"),
  );
}

export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return Array.from(
    digest,
    (byte) => byte.toString(16).padStart(2, "0"),
  ).join("");
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}
