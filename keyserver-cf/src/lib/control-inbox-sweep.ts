const CONTROL_INBOX_SWEEP_BATCH_SIZE = 100;

export interface ControlInboxSweepResult {
  inboxRows: number;
  requestReceipts: number;
}

/**
 * Reclaim one bounded batch from each control-inbox retention table.
 *
 * Both tables are globally unbounded across identities even though live rows
 * are quota-bounded per recipient. Selecting the oldest expired primary keys
 * inside each DELETE keeps one cron tick incremental without materialising a
 * large RETURNING result in the Worker.
 */
export async function sweepExpiredControlInboxRows(
  db: D1Database,
  now = Math.floor(Date.now() / 1000),
): Promise<ControlInboxSweepResult> {
  const results = await db.batch([
    db.prepare(
      `DELETE FROM control_inbox
        WHERE id IN (
          SELECT id
            FROM control_inbox
           WHERE expires_at < ?
           ORDER BY expires_at, id
           LIMIT ?
        )`,
    ).bind(now, CONTROL_INBOX_SWEEP_BATCH_SIZE),
    db.prepare(
      `DELETE FROM control_inbox_requests
        WHERE (sender_id, request_digest) IN (
          SELECT sender_id, request_digest
            FROM control_inbox_requests
           WHERE expires_at < ?
           ORDER BY expires_at, sender_id, request_digest
           LIMIT ?
        )`,
    ).bind(now, CONTROL_INBOX_SWEEP_BATCH_SIZE),
  ]);

  return {
    inboxRows: results[0]?.meta?.changes ?? 0,
    requestReceipts: results[1]?.meta?.changes ?? 0,
  };
}
