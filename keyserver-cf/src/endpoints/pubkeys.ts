import type { Env } from "../env.js";
import { getUserPubkeys } from "../lib/db.js";
import { json, notFound } from "../lib/http.js";

export async function handlePubkeys(env: Env, userId: string): Promise<Response> {
  const row = await getUserPubkeys(env.DB, userId);
  if (!row) return notFound("unknown user_id");
  return json(row);
}
