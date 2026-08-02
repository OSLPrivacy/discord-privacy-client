import { DurableObject } from "cloudflare:workers";
import type { Env } from "../env.js";
import {
  enforceArchiveByteBudget,
  expiredArchiveEntries,
  nextArchiveWakeAt,
  type ArchiveEntry,
} from "./policy.js";

type ArchivePayloadEnv = Env & { ARCHIVE_PAYLOADS: R2Bucket };

interface ArchiveRow extends Record<string, SqlStorageValue> {
  id: string;
  object_key: string;
  received_at: number;
  expires_at: number;
  byte_length: number;
}

/**
 * Per-owner retained-pool metadata. Payload bytes stay in ARCHIVE_PAYLOADS;
 * this SQLite store contains only opaque object keys and lifecycle metadata.
 *
 * This class is intentionally not registered for deployment yet (D77).
 */
export class Archive extends DurableObject<Env> {
  private readonly sql: SqlStorage;
  private readonly payloads: R2Bucket;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.payloads = (env as ArchivePayloadEnv).ARCHIVE_PAYLOADS;
    ctx.blockConcurrencyWhile(async () => this.ensureSchema());
  }

  private ensureSchema(): void {
    this.sql.exec(`
      CREATE TABLE IF NOT EXISTS archive_entries (
        id TEXT PRIMARY KEY,
        object_key TEXT NOT NULL UNIQUE,
        received_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL,
        byte_length INTEGER NOT NULL CHECK (byte_length >= 0)
      ) WITHOUT ROWID;
      CREATE INDEX IF NOT EXISTS idx_archive_entries_received
        ON archive_entries(received_at ASC, id ASC);
      CREATE INDEX IF NOT EXISTS idx_archive_entries_expiry
        ON archive_entries(expires_at ASC);
    `);
  }

  /** Stores metadata and evicts only the oldest retained entries needed for the budget. */
  async store(entry: ArchiveEntry, byteBudget: number): Promise<{ evicted: ArchiveEntry[] }> {
    if (entry.byteLength > byteBudget) throw new RangeError("archive entry exceeds byte budget");
    const rows = this.entries();
    if (rows.some((row) => row.id === entry.id || row.objectKey === entry.objectKey)) {
      throw new Error("archive entry already exists");
    }
    const kept = enforceArchiveByteBudget([...rows, entry], byteBudget);
    const keptIds = new Set(kept.map((row) => row.id));
    const evicted = rows.filter((row) => !keptIds.has(row.id));

    this.ctx.storage.transactionSync(() => {
      this.sql.exec(
        `INSERT INTO archive_entries(id, object_key, received_at, expires_at, byte_length)
         VALUES (?, ?, ?, ?, ?)`,
        entry.id, entry.objectKey, entry.receivedAt, entry.expiresAt, entry.byteLength,
      );
      for (const row of evicted) this.sql.exec("DELETE FROM archive_entries WHERE id = ?", row.id);
    });
    for (const row of evicted) await this.payloads.delete(row.objectKey);
    await this.scheduleNextAlarm();
    return { evicted };
  }

  override async alarm(): Promise<void> {
    const expired = expiredArchiveEntries(this.entries(), Date.now());
    this.ctx.storage.transactionSync(() => {
      for (const row of expired) this.sql.exec("DELETE FROM archive_entries WHERE id = ?", row.id);
    });
    for (const row of expired) await this.payloads.delete(row.objectKey);
    await this.scheduleNextAlarm();
  }

  private entries(): ArchiveEntry[] {
    return this.sql.exec<ArchiveRow>(
      "SELECT id, object_key, received_at, expires_at, byte_length FROM archive_entries",
    ).toArray().map((row) => ({
      id: row.id,
      objectKey: row.object_key,
      receivedAt: row.received_at,
      expiresAt: row.expires_at,
      byteLength: row.byte_length,
    }));
  }

  private async scheduleNextAlarm(): Promise<void> {
    const next = nextArchiveWakeAt(this.entries());
    if (next === null) await this.ctx.storage.deleteAlarm();
    else await this.ctx.storage.setAlarm(Math.max(Date.now() + 1_000, next));
  }
}
