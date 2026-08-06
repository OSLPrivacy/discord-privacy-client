import type { Env } from "../env.js";
import { badRequest, json, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

export type UpdateAttemptResult = "installed" | "failed";

const VERSION_RE = /^[A-Za-z0-9.+-]{1,64}$/;

function validVersion(value: unknown): value is string {
  return typeof value === "string" && VERSION_RE.test(value);
}

function validResult(value: unknown): value is UpdateAttemptResult {
  return value === "installed" || value === "failed";
}

export async function handleUpdateAttemptRecord(
  request: Request,
  env: Env,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 10, "update-attempts");
  if (!rl.ok) return tooMany(rl.retryAfter);

  let body: Record<string, unknown>;
  try {
    const parsed = await request.json();
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      return badRequest("invalid update attempt record");
    }
    body = parsed as Record<string, unknown>;
  } catch {
    return badRequest("invalid JSON body");
  }

  const fromVersion = body.fromVersion;
  const toVersion = body.toVersion;
  const result = body.result;
  if (!validVersion(fromVersion)) return badRequest("fromVersion malformed");
  if (!validVersion(toVersion)) return badRequest("toVersion malformed");
  if (!validResult(result)) return badRequest("result malformed");

  const recordedAtUnixMs = Date.now();
  await env.DB.prepare(
    `INSERT INTO update_attempt_records
      (from_version, to_version, result, recorded_at_unix_ms)
     VALUES (?, ?, ?, ?)`,
  )
    .bind(fromVersion, toVersion, result, recordedAtUnixMs)
    .run();

  return json({
    status: "recorded",
    fromVersion,
    toVersion,
    result,
    recordedAtUnixMs,
  });
}
