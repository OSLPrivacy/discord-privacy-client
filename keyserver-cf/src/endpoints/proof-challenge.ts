import { badRequest, json } from "../lib/http.js";
import { isDiscordSnowflake, isProtocolId } from "../lib/validation.js";

const PROOF_CHALLENGE_NONCE_BYTES = 32;
const PROOF_CHALLENGE_TTL_SECONDS = 5 * 60;

export interface ProofChallengeResponse {
  service: "discord";
  service_account_id: string;
  owner_user_id: string;
  nonce_b64url: string;
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
  spent: false;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function base64UrlNoPad(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/u, "");
}

function issueProofChallenge(
  serviceAccountId: string,
  ownerUserId: string,
  nowUnixSeconds: number,
): ProofChallengeResponse {
  const nonce = new Uint8Array(PROOF_CHALLENGE_NONCE_BYTES);
  crypto.getRandomValues(nonce);
  return {
    service: "discord",
    service_account_id: serviceAccountId,
    owner_user_id: ownerUserId,
    nonce_b64url: base64UrlNoPad(nonce),
    issued_at_unix_seconds: nowUnixSeconds,
    expires_at_unix_seconds: nowUnixSeconds + PROOF_CHALLENGE_TTL_SECONDS,
    spent: false,
  };
}

export async function handleProofChallenge(request: Request): Promise<Response> {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return badRequest("invalid JSON body");
  }
  if (!record(body)) return badRequest("request body must be an object");
  if (body.service !== "discord") return badRequest("service must be discord");
  if (
    typeof body.service_account_id !== "string" ||
    !isDiscordSnowflake(body.service_account_id)
  ) {
    return badRequest("service_account_id must be a Discord snowflake");
  }
  if (
    !isProtocolId(body.owner_user_id) ||
    isDiscordSnowflake(body.owner_user_id)
  ) {
    return badRequest("owner_user_id must be an OSL identity");
  }

  return json(
    issueProofChallenge(
      body.service_account_id,
      body.owner_user_id,
      Math.floor(Date.now() / 1000),
    ),
    { status: 201 },
  );
}
