/// TTL sweep: deletes all blobs whose expires_at is in the past.
///
/// Runs every 5 minutes from the [triggers].crons in wrangler.toml.
/// Returns the aggregate row count to the in-process caller for tests and local
/// diagnostics. Production scheduling deliberately does not log the count:
/// even identifier-free counts reveal traffic volume and timing.

import type { Env } from "../env.js";

export async function sweepExpired(env: Env): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  // D1 doesn't expose affected-rows directly; do a SELECT-count
  // first, then DELETE. The two queries don't have to be atomic --
  // a new expiry crossing the boundary mid-sweep just gets caught
  // on the next tick.
  const countRow = await env.DB.prepare(
    "SELECT COUNT(*) AS c FROM blobs WHERE expires_at < ?"
  )
    .bind(now)
    .first<{ c: number }>();
  const deleted = countRow?.c ?? 0;
  if (deleted > 0) {
    await env.DB.prepare("DELETE FROM blobs WHERE expires_at < ?")
      .bind(now)
      .run();
  }
  return deleted;
}
