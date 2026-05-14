/// D1 query helpers for the F1.2 tables: subscriptions, licenses,
/// stripe_events. Kept thin — state-transition logic lives in
/// subscription-state.ts.

export type SubscriptionStatus =
  | "PENDING"
  | "ACTIVE"
  | "CANCELLED"
  | "GRACE"
  | "REVOKED"
  | "EXPIRED";

export interface SubscriptionRow {
  subscription_id: string;
  customer_id: string;
  customer_email: string;
  status: SubscriptionStatus;
  current_period_end: number | null;
  cancel_at_period_end: number;
  created_at: number;
  updated_at: number;
}

export interface LicenseRow {
  license_hash: string;
  subscription_id: string;
  issued_at: number;
  revoked_at: number | null;
  revoked_reason: "chargeback" | "fraud" | "manual" | null;
}

// ---- subscriptions ----

export async function upsertSubscription(
  db: D1Database,
  row: Omit<SubscriptionRow, "created_at" | "updated_at"> & {
    created_at?: number;
    updated_at?: number;
  },
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await db
    .prepare(
      `INSERT INTO subscriptions (
         subscription_id, customer_id, customer_email, status,
         current_period_end, cancel_at_period_end, created_at, updated_at
       ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
       ON CONFLICT(subscription_id) DO UPDATE SET
         customer_id = excluded.customer_id,
         customer_email = excluded.customer_email,
         status = excluded.status,
         current_period_end = excluded.current_period_end,
         cancel_at_period_end = excluded.cancel_at_period_end,
         updated_at = excluded.updated_at`,
    )
    .bind(
      row.subscription_id,
      row.customer_id,
      row.customer_email,
      row.status,
      row.current_period_end,
      row.cancel_at_period_end,
      row.created_at ?? now,
      row.updated_at ?? now,
    )
    .run();
}

export async function getSubscription(
  db: D1Database,
  subscriptionId: string,
): Promise<SubscriptionRow | null> {
  return await db
    .prepare("SELECT * FROM subscriptions WHERE subscription_id = ?")
    .bind(subscriptionId)
    .first<SubscriptionRow>();
}

export async function getSubscriptionByCustomerId(
  db: D1Database,
  customerId: string,
): Promise<SubscriptionRow | null> {
  return await db
    .prepare(
      `SELECT * FROM subscriptions
        WHERE customer_id = ?
        ORDER BY created_at DESC
        LIMIT 1`,
    )
    .bind(customerId)
    .first<SubscriptionRow>();
}

export async function updateSubscriptionStatus(
  db: D1Database,
  subscriptionId: string,
  patch: Partial<
    Pick<
      SubscriptionRow,
      "status" | "current_period_end" | "cancel_at_period_end"
    >
  >,
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  const sets: string[] = ["updated_at = ?"];
  const binds: (string | number | null)[] = [now];
  if (patch.status !== undefined) {
    sets.push(`status = ?`);
    binds.push(patch.status);
  }
  if (patch.current_period_end !== undefined) {
    sets.push(`current_period_end = ?`);
    binds.push(patch.current_period_end);
  }
  if (patch.cancel_at_period_end !== undefined) {
    sets.push(`cancel_at_period_end = ?`);
    binds.push(patch.cancel_at_period_end);
  }
  binds.push(subscriptionId);
  await db
    .prepare(
      `UPDATE subscriptions SET ${sets.join(", ")} WHERE subscription_id = ?`,
    )
    .bind(...binds)
    .run();
}

/** Hourly cron: promote CANCELLED/GRACE rows past their period
 *  end to EXPIRED. Idempotent — re-running is a no-op. */
export async function sweepExpired(db: D1Database): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const res = await db
    .prepare(
      `UPDATE subscriptions
          SET status = 'EXPIRED', updated_at = ?
        WHERE status IN ('CANCELLED', 'GRACE')
          AND current_period_end IS NOT NULL
          AND current_period_end < ?`,
    )
    .bind(now, now)
    .run();
  return res.meta?.changes ?? 0;
}

// ---- licenses ----

export async function insertLicense(
  db: D1Database,
  row: { license_hash: string; subscription_id: string; issued_at?: number },
): Promise<void> {
  const issued = row.issued_at ?? Math.floor(Date.now() / 1000);
  await db
    .prepare(
      `INSERT INTO licenses (license_hash, subscription_id, issued_at)
       VALUES (?, ?, ?)`,
    )
    .bind(row.license_hash, row.subscription_id, issued)
    .run();
}

export async function getLicenseByHash(
  db: D1Database,
  licenseHash: string,
): Promise<LicenseRow | null> {
  return await db
    .prepare("SELECT * FROM licenses WHERE license_hash = ?")
    .bind(licenseHash)
    .first<LicenseRow>();
}

export async function revokeLicensesForSubscription(
  db: D1Database,
  subscriptionId: string,
  reason: "chargeback" | "fraud" | "manual",
): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const res = await db
    .prepare(
      `UPDATE licenses
          SET revoked_at = ?, revoked_reason = ?
        WHERE subscription_id = ? AND revoked_at IS NULL`,
    )
    .bind(now, reason, subscriptionId)
    .run();
  return res.meta?.changes ?? 0;
}

// ---- stripe_events ----

/** Idempotency gate. Returns true if this is the first time we've
 *  seen `eventId`. Subsequent calls return false (no-op). */
export async function markEventProcessed(
  db: D1Database,
  eventId: string,
  eventType: string,
): Promise<boolean> {
  const now = Math.floor(Date.now() / 1000);
  const res = await db
    .prepare(
      `INSERT OR IGNORE INTO stripe_events (event_id, event_type, processed_at)
       VALUES (?, ?, ?)`,
    )
    .bind(eventId, eventType, now)
    .run();
  return (res.meta?.changes ?? 0) === 1;
}
