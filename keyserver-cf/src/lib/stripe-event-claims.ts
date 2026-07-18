const CLAIM_LEASE_SECONDS = 60;

export type StripeEventClaim = "acquired" | "completed" | "busy";

export async function claimStripeEvent(
  db: D1Database,
  eventId: string,
  eventType: string,
): Promise<StripeEventClaim> {
  const legacy = await db.prepare(
    "SELECT 1 AS present FROM stripe_events WHERE event_id = ?",
  ).bind(eventId).first<{ present: number }>();
  if (legacy) return "completed";

  const now = Math.floor(Date.now() / 1000);
  const inserted = await db.prepare(
    `INSERT OR IGNORE INTO stripe_event_claims (
       event_id, event_type, status, claimed_at, completed_at
     ) VALUES (?, ?, 'processing', ?, NULL)`,
  ).bind(eventId, eventType, now).run();
  if ((inserted.meta?.changes ?? 0) === 1) return "acquired";

  const current = await db.prepare(
    "SELECT status FROM stripe_event_claims WHERE event_id = ?",
  ).bind(eventId).first<{ status: "processing" | "completed" }>();
  if (current?.status === "completed") return "completed";

  const reclaimed = await db.prepare(
    `UPDATE stripe_event_claims
        SET event_type = ?, claimed_at = ?
      WHERE event_id = ? AND status = 'processing' AND claimed_at <= ?`,
  ).bind(eventType, now, eventId, now - CLAIM_LEASE_SECONDS).run();
  return (reclaimed.meta?.changes ?? 0) === 1 ? "acquired" : "busy";
}

export async function completeStripeEvent(
  db: D1Database,
  eventId: string,
  eventType: string,
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await db.batch([
    db.prepare(
      `INSERT OR IGNORE INTO stripe_events (event_id, event_type, processed_at)
       VALUES (?, ?, ?)`,
    ).bind(eventId, eventType, now),
    db.prepare(
      `UPDATE stripe_event_claims
          SET status = 'completed', completed_at = ?
        WHERE event_id = ? AND status = 'processing'`,
    ).bind(now, eventId),
  ]);
}

export async function releaseStripeEvent(
  db: D1Database,
  eventId: string,
): Promise<void> {
  await db.prepare(
    "DELETE FROM stripe_event_claims WHERE event_id = ? AND status = 'processing'",
  ).bind(eventId).run();
}
