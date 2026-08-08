import { isProtocolId } from "./validation.js";

export const OWNER_ROLE = "owner";
export const SUCCESSION_LOG_ACTION = "ownership_succession_transferred_to_named_successor";

export interface AccountOwnershipChange {
  readonly changedRoles: number;
}

export interface OwnershipSuccessionSweep {
  readonly changedRoles: number;
  readonly moderationLogRows: number;
}

interface SuccessionCandidate {
  account_id: string;
  owner_user_id: string;
  successor_user_id: string;
}

function assertProtocolId(value: string, field: string): void {
  if (!isProtocolId(value)) throw new Error(`${field} must be a bounded protocol id`);
}

function assertDifferent(left: string, right: string, message: string): void {
  if (left === right) throw new Error(message);
}

async function requireOwnerRole(
  db: D1Database,
  accountId: string,
  ownerUserId: string,
): Promise<void> {
  const row = await db.prepare(
    "SELECT 1 ok FROM account_roles WHERE account_id = ? AND user_id = ? AND role = 'owner'",
  ).bind(accountId, ownerUserId).first<{ ok: number }>();
  if (!row) throw new Error("account owner role is required");
}

export async function seedAccountOwner(
  db: D1Database,
  accountId: string,
  ownerUserId: string,
  nowUnixSeconds: number,
): Promise<void> {
  assertProtocolId(accountId, "account_id");
  assertProtocolId(ownerUserId, "owner_user_id");
  await db.prepare(
    `INSERT INTO account_roles (
       account_id, user_id, role, granted_by_user_id,
       granted_at_unix_seconds, last_seen_at_unix_seconds
     )
     VALUES (?, ?, 'owner', ?, ?, ?)`,
  ).bind(accountId, ownerUserId, ownerUserId, nowUnixSeconds, nowUnixSeconds).run();
}

export async function addCoOwner(
  db: D1Database,
  accountId: string,
  ownerUserId: string,
  coOwnerUserId: string,
  nowUnixSeconds: number,
): Promise<AccountOwnershipChange> {
  assertProtocolId(accountId, "account_id");
  assertProtocolId(ownerUserId, "owner_user_id");
  assertProtocolId(coOwnerUserId, "co_owner_user_id");
  assertDifferent(ownerUserId, coOwnerUserId, "co-owner must be a different account");
  await requireOwnerRole(db, accountId, ownerUserId);
  const result = await db.prepare(
    `INSERT INTO account_roles (
       account_id, user_id, role, granted_by_user_id,
       granted_at_unix_seconds, last_seen_at_unix_seconds
     )
     VALUES (?, ?, 'owner', ?, ?, ?)`,
  ).bind(accountId, coOwnerUserId, ownerUserId, nowUnixSeconds, nowUnixSeconds).run();
  return { changedRoles: result.meta.changes ?? 0 };
}

export async function requestOwnershipTransfer(
  db: D1Database,
  transferId: string,
  accountId: string,
  fromUserId: string,
  toUserId: string,
  nowUnixSeconds: number,
): Promise<void> {
  assertProtocolId(transferId, "transfer_id");
  assertProtocolId(accountId, "account_id");
  assertProtocolId(fromUserId, "from_user_id");
  assertProtocolId(toUserId, "to_user_id");
  assertDifferent(fromUserId, toUserId, "transfer receiver must be a different account");
  await requireOwnerRole(db, accountId, fromUserId);
  await db.prepare(
    `INSERT INTO ownership_transfers (
       transfer_id, account_id, from_user_id, to_user_id,
       status, requested_at_unix_seconds, accepted_at_unix_seconds
     )
     VALUES (?, ?, ?, ?, 'pending', ?, NULL)`,
  ).bind(transferId, accountId, fromUserId, toUserId, nowUnixSeconds).run();
}

export async function acceptOwnershipTransfer(
  db: D1Database,
  transferId: string,
  receiverUserId: string,
  nowUnixSeconds: number,
): Promise<AccountOwnershipChange> {
  assertProtocolId(transferId, "transfer_id");
  assertProtocolId(receiverUserId, "receiver_user_id");
  const transfer = await db.prepare(
    `SELECT account_id, from_user_id, to_user_id
       FROM ownership_transfers
      WHERE transfer_id = ? AND to_user_id = ? AND status = 'pending'`,
  ).bind(transferId, receiverUserId).first<{
    account_id: string;
    from_user_id: string;
    to_user_id: string;
  }>();
  if (!transfer) throw new Error("pending transfer for receiver is required");

  const results = await db.batch([
    db.prepare(
      `UPDATE ownership_transfers
          SET status = 'accepted', accepted_at_unix_seconds = ?
        WHERE transfer_id = ? AND status = 'pending'`,
    ).bind(nowUnixSeconds, transferId),
    db.prepare(
      "DELETE FROM account_roles WHERE account_id = ? AND user_id = ? AND role = 'owner'",
    ).bind(transfer.account_id, transfer.from_user_id),
    db.prepare(
      `INSERT OR IGNORE INTO account_roles (
         account_id, user_id, role, granted_by_user_id,
         granted_at_unix_seconds, last_seen_at_unix_seconds
       )
       VALUES (?, ?, 'owner', ?, ?, ?)`,
    ).bind(
      transfer.account_id,
      transfer.to_user_id,
      transfer.from_user_id,
      nowUnixSeconds,
      nowUnixSeconds,
    ),
  ]);
  return {
    changedRoles: (results[1]?.meta.changes ?? 0) + (results[2]?.meta.changes ?? 0),
  };
}

export async function chooseOwnershipSuccessor(
  db: D1Database,
  accountId: string,
  ownerUserId: string,
  successorUserId: string,
  quietPeriodSeconds: number,
  nowUnixSeconds: number,
): Promise<void> {
  assertProtocolId(accountId, "account_id");
  assertProtocolId(ownerUserId, "owner_user_id");
  assertProtocolId(successorUserId, "successor_user_id");
  assertDifferent(ownerUserId, successorUserId, "successor must be a different account");
  if (!Number.isSafeInteger(quietPeriodSeconds) || quietPeriodSeconds <= 0) {
    throw new Error("quiet period must be a positive integer");
  }
  await requireOwnerRole(db, accountId, ownerUserId);
  await db.prepare(
    `INSERT INTO ownership_succession_settings (
       account_id, owner_user_id, successor_user_id,
       quiet_period_seconds, chosen_at_unix_seconds
     )
     VALUES (?, ?, ?, ?, ?)
     ON CONFLICT(account_id, owner_user_id) DO UPDATE SET
       successor_user_id = excluded.successor_user_id,
       quiet_period_seconds = excluded.quiet_period_seconds,
       chosen_at_unix_seconds = excluded.chosen_at_unix_seconds`,
  ).bind(
    accountId,
    ownerUserId,
    successorUserId,
    quietPeriodSeconds,
    nowUnixSeconds,
  ).run();
}

export async function recordOwnerActivity(
  db: D1Database,
  accountId: string,
  ownerUserId: string,
  lastSeenUnixSeconds: number,
): Promise<AccountOwnershipChange> {
  assertProtocolId(accountId, "account_id");
  assertProtocolId(ownerUserId, "owner_user_id");
  const result = await db.prepare(
    `UPDATE account_roles
        SET last_seen_at_unix_seconds = ?
      WHERE account_id = ? AND user_id = ? AND role = 'owner'`,
  ).bind(lastSeenUnixSeconds, accountId, ownerUserId).run();
  return { changedRoles: result.meta.changes ?? 0 };
}

export async function sweepQuietOwnerSuccessions(
  db: D1Database,
  nowUnixSeconds = Math.floor(Date.now() / 1000),
): Promise<OwnershipSuccessionSweep> {
  const candidates = await db.prepare(
    `SELECT r.account_id, r.user_id AS owner_user_id, s.successor_user_id
       FROM account_roles r
       JOIN ownership_succession_settings s
         ON s.account_id = r.account_id
        AND s.owner_user_id = r.user_id
      WHERE r.role = 'owner'
        AND r.last_seen_at_unix_seconds + s.quiet_period_seconds <= ?
      ORDER BY r.account_id, r.user_id`,
  ).bind(nowUnixSeconds).all<SuccessionCandidate>();

  let changedRoles = 0;
  let moderationLogRows = 0;
  for (const candidate of candidates.results) {
    const logId = crypto.randomUUID();
    const searchableText = [
      SUCCESSION_LOG_ACTION,
      `account_id=${candidate.account_id}`,
      `former_owner=${candidate.owner_user_id}`,
      `named_successor=${candidate.successor_user_id}`,
    ].join(" ");
    const results = await db.batch([
      db.prepare(
        "DELETE FROM account_roles WHERE account_id = ? AND user_id = ? AND role = 'owner'",
      ).bind(candidate.account_id, candidate.owner_user_id),
      db.prepare(
        `INSERT OR IGNORE INTO account_roles (
           account_id, user_id, role, granted_by_user_id,
           granted_at_unix_seconds, last_seen_at_unix_seconds
         )
         VALUES (?, ?, 'owner', ?, ?, ?)`,
      ).bind(
        candidate.account_id,
        candidate.successor_user_id,
        candidate.owner_user_id,
        nowUnixSeconds,
        nowUnixSeconds,
      ),
      db.prepare(
        `DELETE FROM ownership_succession_settings
          WHERE account_id = ? AND owner_user_id = ?`,
      ).bind(candidate.account_id, candidate.owner_user_id),
      db.prepare(
        `INSERT INTO searchable_moderation_log (
           log_id, account_id, actor_user_id, subject_user_id,
           action, searchable_text, created_at_unix_seconds
         )
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
      ).bind(
        logId,
        candidate.account_id,
        candidate.owner_user_id,
        candidate.successor_user_id,
        SUCCESSION_LOG_ACTION,
        searchableText,
        nowUnixSeconds,
      ),
    ]);
    changedRoles += (results[0]?.meta.changes ?? 0) + (results[1]?.meta.changes ?? 0);
    moderationLogRows += results[3]?.meta.changes ?? 0;
  }

  return { changedRoles, moderationLogRows };
}
