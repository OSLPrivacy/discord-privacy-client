/**
 * D-274 — retention for `space_event_queue`.
 *
 * `expires_at` was a read filter only: the drain skipped expired rows and
 * nothing ever deleted them, so the table grew without bound and expired
 * ciphertext stayed in storage while being invisible to every reader. That is
 * retention without a retention policy.
 *
 * Bounded and observable, both on purpose:
 *
 * - **Bounded.** At most `SPACE_EVENT_SWEEP_MAX_BATCHES` batches of
 *   `SPACE_EVENT_SWEEP_BATCH_SIZE` per run, so one cron tick cannot spend the
 *   whole D1 budget on a backlog. The same batched-`DELETE ... WHERE id IN
 *   (SELECT ... LIMIT ?)` shape `sweepExpiredControlInboxRows` already uses.
 * - **Observable, and it never silently truncates** (D-254's residue-overflow
 *   lesson). Hitting the bound is not "done": the result carries `remaining`,
 *   counted after the deletes, so the caller can say what it could NOT remove.
 *   A sweep that quietly stops at its bound is indistinguishable from a sweep
 *   that finished, and only one of those means the table is bounded.
 */

export const SPACE_EVENT_SWEEP_BATCH_SIZE = 500;
export const SPACE_EVENT_SWEEP_MAX_BATCHES = 10;
export const SPACE_EVENT_SWEEP_MAX_ROWS =
  SPACE_EVENT_SWEEP_BATCH_SIZE * SPACE_EVENT_SWEEP_MAX_BATCHES;

export interface SpaceEventSweepResult {
  /** Rows actually deleted by this run. */
  deleted: number;
  /**
   * Expired rows STILL in storage when this run gave up — what the sweep
   * could not remove. Non-zero means the bound was reached, not that the
   * table is clean.
   */
  remaining: number;
  /** True when `remaining > 0`, i.e. the run stopped at its bound. */
  boundReached: boolean;
}

async function countExpired(db: D1Database, now: number): Promise<number> {
  const row = await db
    .prepare("SELECT COUNT(*) AS n FROM space_event_queue WHERE expires_at <= ?")
    .bind(now)
    .first<{ n: number }>();
  return row?.n ?? 0;
}

/**
 * `bounds` exists so the residue path can be EXECUTED by a test rather than
 * described by one: a bound that can only be reached by queueing five thousand
 * rows is a bound nothing ever proves. Production passes neither argument and
 * gets the shipped constants above.
 */
export async function sweepExpiredSpaceEvents(
  db: D1Database,
  now = Math.floor(Date.now() / 1000),
  bounds: { batchSize?: number; maxBatches?: number } = {},
): Promise<SpaceEventSweepResult> {
  const batchSize = bounds.batchSize ?? SPACE_EVENT_SWEEP_BATCH_SIZE;
  const maxBatches = bounds.maxBatches ?? SPACE_EVENT_SWEEP_MAX_BATCHES;
  let deleted = 0;
  for (let batch = 0; batch < maxBatches; batch += 1) {
    const before = await countExpired(db, now);
    if (before === 0) break;
    await db
      .prepare(
        `DELETE FROM space_event_queue
          WHERE id IN (
            SELECT id
              FROM space_event_queue
             WHERE expires_at <= ?
             ORDER BY expires_at, id
             LIMIT ?
          )`,
      )
      .bind(now, batchSize)
      .run();
    const after = await countExpired(db, now);
    // Counted from the table rather than trusted from `meta.changes`: this
    // number is the evidence the sweep works, and a driver that does not
    // report `changes` would otherwise make an inert sweep look successful.
    const removed = before - after;
    deleted += removed;
    if (removed <= 0) break;
    if (after === 0) break;
  }
  const remaining = await countExpired(db, now);
  return { deleted, remaining, boundReached: remaining > 0 };
}
