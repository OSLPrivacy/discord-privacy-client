import {
  isDiscordSnowflake,
  isProtocolId,
} from "./validation.js";

export const CONTROL_INBOX_SWEEP_BATCH_SIZE = 100;
export const CONTROL_INBOX_DISABLED_RETRY_LIMIT = 3;
export const CONTROL_INBOX_DISABLED_RETRY_SECONDS = 60 * 60;
export const CONTROL_INBOX_DISABLED_RETENTION_SECONDS = 7 * 24 * 60 * 60;

type DeliveryStatus = "live" | "retryable" | "quarantined" | "retired";

interface ReconcileCandidate {
  id: unknown;
  sender_id: string;
  expires_at: number;
  delivery_status: DeliveryStatus;
  delivery_attempts: number;
  sender_disabled_first_seen_at: number | null;
  delivery_next_retry_at: number | null;
  delivery_retain_until: number | null;
  sender_enabled: number;
}

export interface ControlInboxReconcileResult {
  examined: number;
  reenabled: number;
  retryable: number;
  quarantined: number;
  retired: number;
}

export interface ControlInboxSweepResult {
  inboxRows: number;
  requestReceipts: number;
  senderStates: ControlInboxReconcileResult;
}

export const CONTROL_INBOX_DISPOSITION_CAPABILITY =
  "control_inbox_sender_disposition";
export const CONTROL_INBOX_RECONCILIATION_STARTED_CAPABILITY =
  "control_inbox_sender_reconciliation_started";
const dispositionReadyDatabases = new WeakSet<object>();

/**
 * Exact schema capability used by control-inbox routes and healthz.
 *
 * The marker makes release/rollback state observable, while the zero-row
 * projection proves the columns themselves exist. Neither query reads an
 * inbox row or its identifiers/payload.
 */
export async function controlInboxDispositionSchemaReady(
  db: D1Database,
): Promise<boolean> {
  if (dispositionReadyDatabases.has(db as object)) return true;
  try {
    const marker = await db.prepare(
      `SELECT version
         FROM worker_schema_capabilities
        WHERE capability = ?`,
    ).bind(CONTROL_INBOX_DISPOSITION_CAPABILITY).first<{ version: number }>();
    if (marker?.version !== 1) return false;
    await db.prepare(
      `SELECT delivery_status,
              delivery_reason,
              delivery_attempts,
              sender_disabled_first_seen_at,
              delivery_next_retry_at,
              delivery_retain_until
         FROM control_inbox
        LIMIT 0`,
    ).all();
    dispositionReadyDatabases.add(db as object);
    return true;
  } catch {
    return false;
  }
}

function blobBytes(value: unknown): Uint8Array | null {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  if (Array.isArray(value)) return Uint8Array.from(value as number[]);
  return null;
}

/**
 * Classify one bounded batch of rows whose sender cannot currently be looked
 * up, or restore rows whose sender has since re-enabled lookup.
 *
 * This function never selects or updates `bundle`. Every state transition is a
 * compare-and-swap over metadata and rechecks the current users row in SQL, so
 * a registration racing the sweep cannot leave a newly enabled sender hidden.
 */
export async function reconcileControlInboxSenderStates(
  db: D1Database,
  now = Math.floor(Date.now() / 1000),
): Promise<ControlInboxReconcileResult> {
  if (!(await controlInboxDispositionSchemaReady(db))) {
    throw new Error("control inbox schema unavailable");
  }

  // This durable, monotonic marker is the rollback boundary. It is committed
  // before the candidate SELECT and before any delivery-status write. Artifact
  // selection must refuse the pre-0031 bridge once this marker exists, even
  // when the first reconciliation finds no candidates.
  await db.prepare(
    `INSERT INTO worker_schema_capabilities (capability, version)
     VALUES (?, 1)
     ON CONFLICT(capability) DO UPDATE SET version =
       MAX(worker_schema_capabilities.version, excluded.version)`,
  ).bind(CONTROL_INBOX_RECONCILIATION_STARTED_CAPABILITY).run();
  const reconciliationMarker = await db.prepare(
    `SELECT version
       FROM worker_schema_capabilities
      WHERE capability = ?`,
  ).bind(CONTROL_INBOX_RECONCILIATION_STARTED_CAPABILITY)
    .first<{ version: number }>();
  if (reconciliationMarker?.version !== 1) {
    throw new Error("control inbox reconciliation marker invalid");
  }

  const selected = await db.prepare(
    `SELECT ci.id,
            ci.sender_id,
            ci.expires_at,
            ci.delivery_status,
            ci.delivery_attempts,
            ci.sender_disabled_first_seen_at,
            ci.delivery_next_retry_at,
            ci.delivery_retain_until,
            CASE WHEN u.identity_lookup_enabled = 1 THEN 1 ELSE 0 END
              AS sender_enabled
      FROM control_inbox ci
       LEFT JOIN users u ON u.user_id = ci.sender_id
      WHERE
        (
          length(ci.sender_id) BETWEEN 17 AND 20
          AND ci.sender_id NOT GLOB '*[^0-9]*'
          AND ci.delivery_status <> 'retired'
        )
        OR
        (
          u.identity_lookup_enabled = 1
          AND ci.delivery_status <> 'live'
          AND NOT (
            length(ci.sender_id) BETWEEN 17 AND 20
            AND ci.sender_id NOT GLOB '*[^0-9]*'
          )
        )
        OR
        (
          (u.user_id IS NULL OR u.identity_lookup_enabled = 0)
          AND (
            ci.delivery_status = 'live'
            OR (
              ci.delivery_status = 'retryable'
              AND ci.delivery_next_retry_at <= ?
            )
          )
        )
      ORDER BY
        CASE
          WHEN u.identity_lookup_enabled = 1
               AND ci.delivery_status <> 'live' THEN 0
          ELSE 1
        END,
        ci.created_at,
        ci.id
      LIMIT ?`,
  ).bind(now, CONTROL_INBOX_SWEEP_BATCH_SIZE).all<ReconcileCandidate>();

  const candidates = selected.results ?? [];
  const statements: D1PreparedStatement[] = [];
  const planned: Array<keyof Omit<ControlInboxReconcileResult, "examined">> = [];

  for (const row of candidates) {
    const id = blobBytes(row.id);
    if (!id) continue;

    const firstSeen = row.sender_disabled_first_seen_at ?? now;
    const retainUntil =
      firstSeen + CONTROL_INBOX_DISABLED_RETENTION_SECONDS;
    if (isDiscordSnowflake(row.sender_id)) {
      if (row.delivery_status === "retired") continue;
      statements.push(
        db.prepare(
          `UPDATE control_inbox
              SET delivery_status = 'retired',
                  delivery_reason = 'sender_discord_snowflake',
                  delivery_attempts = 0,
                  sender_disabled_first_seen_at = ?,
                  delivery_next_retry_at = NULL,
                  delivery_retain_until = ?
            WHERE id = ?
              AND delivery_status = ?
              AND delivery_attempts = ?
              AND COALESCE(delivery_next_retry_at, -1) =
                  COALESCE(?, -1)`,
        ).bind(
          firstSeen,
          retainUntil,
          id,
          row.delivery_status,
          row.delivery_attempts,
          row.delivery_next_retry_at,
        ),
      );
      planned.push("retired");
      continue;
    }

    if (row.sender_enabled === 1) {
      statements.push(
        db.prepare(
          `UPDATE control_inbox
              SET expires_at = CASE
                    WHEN delivery_retain_until IS NOT NULL
                         AND delivery_retain_until > expires_at
                      THEN delivery_retain_until
                    ELSE expires_at
                  END,
                  delivery_status = 'live',
                  delivery_reason = NULL,
                  delivery_attempts = 0,
                  sender_disabled_first_seen_at = NULL,
                  delivery_next_retry_at = NULL,
                  delivery_retain_until = NULL
            WHERE id = ?
              AND delivery_status = ?
              AND delivery_attempts = ?
              AND COALESCE(delivery_next_retry_at, -1) =
                  COALESCE(?, -1)
              AND EXISTS (
                SELECT 1 FROM users
                 WHERE user_id = ? AND identity_lookup_enabled = 1
              )`,
        ).bind(
          id,
          row.delivery_status,
          row.delivery_attempts,
          row.delivery_next_retry_at,
          row.sender_id,
        ),
      );
      planned.push("reenabled");
      continue;
    }

    const attempts = row.delivery_attempts + 1;
    const malformed = !isProtocolId(row.sender_id);
    const quarantined = attempts >= CONTROL_INBOX_DISABLED_RETRY_LIMIT;
    const status: DeliveryStatus = quarantined ? "quarantined" : "retryable";
    const reason = quarantined
      ? (malformed
          ? "sender_identifier_malformed"
          : "sender_lookup_retry_exhausted")
      : (malformed
          ? "sender_identifier_malformed"
          : "sender_lookup_disabled");
    const nextRetryAt = quarantined
      ? null
      : Math.min(
          now + CONTROL_INBOX_DISABLED_RETRY_SECONDS,
          retainUntil,
        );

    statements.push(
      db.prepare(
        `UPDATE control_inbox
            SET delivery_status = ?,
                delivery_reason = ?,
                delivery_attempts = ?,
                sender_disabled_first_seen_at = ?,
                delivery_next_retry_at = ?,
                delivery_retain_until = ?
          WHERE id = ?
            AND delivery_status = ?
            AND delivery_attempts = ?
            AND COALESCE(delivery_next_retry_at, -1) =
                COALESCE(?, -1)
            AND NOT EXISTS (
              SELECT 1 FROM users
               WHERE user_id = ? AND identity_lookup_enabled = 1
            )`,
      ).bind(
        status,
        reason,
        attempts,
        firstSeen,
        nextRetryAt,
        retainUntil,
        id,
        row.delivery_status,
        row.delivery_attempts,
        row.delivery_next_retry_at,
        row.sender_id,
      ),
    );
    planned.push(quarantined ? "quarantined" : "retryable");
  }

  const aggregate: ControlInboxReconcileResult = {
    examined: candidates.length,
    reenabled: 0,
    retryable: 0,
    quarantined: 0,
    retired: 0,
  };
  if (statements.length === 0) return aggregate;

  const results = await db.batch(statements);
  for (let index = 0; index < results.length; index += 1) {
    if ((results[index]?.meta?.changes ?? 0) === 1) {
      aggregate[planned[index]!] += 1;
    }
  }
  return aggregate;
}

/**
 * Reclaim one bounded batch from each control-inbox retention table.
 *
 * Sender disposition happens before deletion. An expired live row is eligible
 * only when its sender is currently lookup-enabled; a disabled/missing sender
 * must first enter the recorded retry/retention state. Retryable rows are never
 * cleanup candidates; quarantined/retired rows become eligible only after the
 * exact bounded retention deadline, independently of the original TTL.
 */
export async function sweepExpiredControlInboxRows(
  db: D1Database,
  now = Math.floor(Date.now() / 1000),
): Promise<ControlInboxSweepResult> {
  const senderStates = await reconcileControlInboxSenderStates(db, now);
  const results = await db.batch([
    db.prepare(
      `DELETE FROM control_inbox
        WHERE id IN (
          SELECT ci.id
            FROM control_inbox ci
           WHERE (
             ci.delivery_status = 'live'
             AND ci.expires_at < ?
             AND EXISTS (
               SELECT 1 FROM users
                WHERE user_id = ci.sender_id
                  AND identity_lookup_enabled = 1
             )
           )
           OR (
             ci.delivery_status IN ('quarantined', 'retired')
             AND ci.delivery_retain_until < ?
           )
           ORDER BY
             CASE
               WHEN ci.delivery_status = 'live' THEN ci.expires_at
               ELSE ci.delivery_retain_until
             END,
             ci.id
           LIMIT ?
        )`,
    ).bind(now, now, CONTROL_INBOX_SWEEP_BATCH_SIZE),
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
    senderStates,
  };
}
