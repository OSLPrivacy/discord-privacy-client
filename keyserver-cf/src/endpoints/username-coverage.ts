import { badRequest, json } from "../lib/http.js";
import { isDiscordSnowflake } from "../lib/validation.js";

export const USERNAME_COVERAGE_RESPONSE_VERSION = 1;

export const USERNAME_COVERAGE_SIGNAL_CATEGORIES = [
  "public_profile_presence",
  "public_post_reference",
  "public_media_reference",
  "public_mention_reference",
  "contact_detail_exposure",
  "location_exposure",
  "credential_or_secret_exposure",
  "financial_or_identity_document_exposure",
  "sensitive_image_reference",
] as const;

const USERNAME_RE = /^[A-Za-z0-9._-]{1,64}$/;

export async function handleUsernameCoverage(request: Request): Promise<Response> {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return badRequest("invalid JSON body");
  }
  if (!isRecord(body) || !exactKeys(body, ["username"])) {
    return badRequest("username coverage request must contain exactly username");
  }
  const username = body.username;
  if (
    typeof username !== "string" ||
    !USERNAME_RE.test(username) ||
    isDiscordSnowflake(username)
  ) {
    return badRequest("username is invalid");
  }

  return json({
    version: USERNAME_COVERAGE_RESPONSE_VERSION,
    username,
    result_status: "not_scanned",
    coverage_signal_categories: USERNAME_COVERAGE_SIGNAL_CATEGORIES,
    signals: [],
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return (
    actual.length === expected.length &&
    actual.every((key, index) => key === expected[index])
  );
}
