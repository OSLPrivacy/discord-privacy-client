import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { describe, expect, it } from "vitest";
import {
  SUCCESSION_LOG_ACTION,
  acceptOwnershipTransfer,
  addCoOwner,
  chooseOwnershipSuccessor,
  requestOwnershipTransfer,
  seedAccountOwner,
  sweepQuietOwnerSuccessions,
} from "../src/lib/account-ownership.js";

const NOW = 1_800_000_000;

class SqliteD1Statement {
  private values: any[] = [];

  constructor(
    private readonly db: DatabaseSync,
    private readonly sql: string,
  ) {}

  bind(...values: any[]): this {
    this.values = values;
    return this;
  }

  run(): D1Result {
    const result = this.db.prepare(this.sql).run(...this.values);
    return { meta: { changes: result.changes } } as D1Result;
  }

  first<T>(): T | null {
    return (this.db.prepare(this.sql).get(...this.values) as T | undefined) ?? null;
  }

  all<T>(): D1Result<T> {
    return { results: this.db.prepare(this.sql).all(...this.values) as T[] } as D1Result<T>;
  }
}

class SqliteD1 {
  constructor(readonly db: DatabaseSync) {}

  prepare(sql: string): SqliteD1Statement {
    return new SqliteD1Statement(this.db, sql);
  }

  batch(statements: SqliteD1Statement[]): D1Result[] {
    const results: D1Result[] = [];
    this.db.exec("BEGIN");
    try {
      for (const statement of statements) results.push(statement.run());
      this.db.exec("COMMIT");
      return results;
    } catch (error) {
      this.db.exec("ROLLBACK");
      throw error;
    }
  }
}

function migratedDb(): D1Database {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON");
  db.exec("CREATE TABLE users (user_id TEXT PRIMARY KEY)");
  db.exec(readFileSync(new URL("../migrations/0047_account_owner_transfer_succession.sql", import.meta.url), "utf8"));
  return new SqliteD1(db) as unknown as D1Database;
}

async function insertUsers(db: D1Database, userIds: string[]): Promise<void> {
  for (const userId of userIds) {
    await db.prepare("INSERT INTO users (user_id) VALUES (?)").bind(userId).run();
  }
}

async function ownerRoleCount(db: D1Database, accountId: string): Promise<number> {
  const row = await db.prepare(
    "SELECT COUNT(*) count FROM account_roles WHERE account_id = ? AND role = 'owner'",
  ).bind(accountId).first<{ count: number }>();
  return row?.count ?? 0;
}

async function ownerRoleUsers(db: D1Database, accountId: string): Promise<string[]> {
  const rows = await db.prepare(
    "SELECT user_id FROM account_roles WHERE account_id = ? AND role = 'owner' ORDER BY user_id",
  ).bind(accountId).all<{ user_id: string }>();
  return rows.results.map((row) => row.user_id);
}

async function pendingTransferCount(db: D1Database, accountId: string): Promise<number> {
  const row = await db.prepare(
    "SELECT COUNT(*) count FROM ownership_transfers WHERE account_id = ? AND status = 'pending'",
  ).bind(accountId).first<{ count: number }>();
  return row?.count ?? 0;
}

async function successionLogCount(db: D1Database, accountId: string): Promise<number> {
  const row = await db.prepare(
    `SELECT COUNT(*) count
       FROM searchable_moderation_log
      WHERE account_id = ?
        AND action = ?
        AND searchable_text LIKE ?`,
  ).bind(
    accountId,
    SUCCESSION_LOG_ACTION,
    `%${SUCCESSION_LOG_ACTION}%`,
  ).first<{ count: number }>();
  return row?.count ?? 0;
}

describe("account co-owners, transfer acceptance, and succession", () => {
  it("TASK5027 proves co-owner count, two-sided transfer, named succession, and no-successor no-op", async () => {
    const db = migratedDb();
    const suffix = "task5027";
    const owner = `owner-${suffix}`;
    const coOwner = `co-owner-${suffix}`;
    const receiver = `receiver-${suffix}`;
    const successor = `successor-${suffix}`;
    await insertUsers(db, [owner, coOwner, receiver, successor]);

    const coOwnedAccount = `acct-co-${suffix}`;
    await seedAccountOwner(db, coOwnedAccount, owner, NOW);
    await addCoOwner(db, coOwnedAccount, owner, coOwner, NOW + 1);
    const coOwnerCount = await ownerRoleCount(db, coOwnedAccount);
    console.log(`TASK5027_CO_OWNER_ROLE_COUNT=${coOwnerCount}`);
    expect(coOwnerCount).toBe(2);

    const transferAccount = `acct-transfer-${suffix}`;
    await seedAccountOwner(db, transferAccount, owner, NOW);
    const rolesBeforePending = await ownerRoleCount(db, transferAccount);
    await requestOwnershipTransfer(
      db,
      `transfer-${suffix}`,
      transferAccount,
      owner,
      receiver,
      NOW + 2,
    );
    const pendingTransfers = await pendingTransferCount(db, transferAccount);
    const rolesAfterPending = await ownerRoleCount(db, transferAccount);
    console.log(`TASK5027_PENDING_TRANSFER_COUNT=${pendingTransfers}`);
    console.log(`TASK5027_PENDING_TRANSFER_ROLE_DELTA=${rolesAfterPending - rolesBeforePending}`);
    expect(pendingTransfers).toBe(1);
    expect(rolesAfterPending - rolesBeforePending).toBe(0);

    await acceptOwnershipTransfer(db, `transfer-${suffix}`, receiver, NOW + 3);
    const acceptedOwners = await ownerRoleUsers(db, transferAccount);
    console.log(`TASK5027_ACCEPTED_TRANSFER_OWNER_COUNT=${acceptedOwners.length}`);
    console.log(`TASK5027_ACCEPTED_TRANSFER_OWNER=${acceptedOwners.join("|")}`);
    expect(acceptedOwners).toEqual([receiver]);

    const successionAccount = `acct-succession-${suffix}`;
    const quietPeriodSeconds = 60;
    await seedAccountOwner(db, successionAccount, owner, NOW - 120);
    await chooseOwnershipSuccessor(
      db,
      successionAccount,
      owner,
      successor,
      quietPeriodSeconds,
      NOW - 119,
    );
    const successionSweep = await sweepQuietOwnerSuccessions(db, NOW);
    const successionOwners = await ownerRoleUsers(db, successionAccount);
    const logRows = await successionLogCount(db, successionAccount);
    console.log(`TASK5027_SUCCESSION_CHANGED_ROLES=${successionSweep.changedRoles}`);
    console.log(`TASK5027_SUCCESSION_OWNER_COUNT=${successionOwners.length}`);
    console.log(`TASK5027_SUCCESSION_OWNER=${successionOwners.join("|")}`);
    console.log(`TASK5027_SUCCESSION_MODERATION_LOG_ACTION=${SUCCESSION_LOG_ACTION}`);
    console.log(`TASK5027_SUCCESSION_MODERATION_LOG_ROWS=${logRows}`);
    expect(successionOwners).toEqual([successor]);
    expect(logRows).toBe(1);

    const noSuccessorAccount = `acct-no-successor-${suffix}`;
    await seedAccountOwner(db, noSuccessorAccount, owner, NOW - 120);
    const noSuccessorBefore = await ownerRoleUsers(db, noSuccessorAccount);
    const noSuccessorSweep = await sweepQuietOwnerSuccessions(db, NOW);
    const noSuccessorAfter = await ownerRoleUsers(db, noSuccessorAccount);
    console.log(`TASK5027_NO_SUCCESSOR_CHANGED_ROLES=${noSuccessorSweep.changedRoles}`);
    console.log(`TASK5027_NO_SUCCESSOR_OWNER_COUNT=${noSuccessorAfter.length}`);
    expect(noSuccessorSweep.changedRoles).toBe(0);
    expect(noSuccessorAfter).toEqual(noSuccessorBefore);
  });
});
