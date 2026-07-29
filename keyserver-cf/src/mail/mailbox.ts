import { DurableObject } from "cloudflare:workers";
import type { Env } from "../env.js";

export const MAIL_MAX_CIPHERTEXT_BYTES = 768 * 1024;
export const MAIL_MAX_MESSAGES = 500;
export const MAIL_MAX_TOTAL_BYTES = 32 * 1024 * 1024;
export const OSL_MAIL_TTL_MS = 7 * 24 * 60 * 60 * 1000;
export const EXTERNAL_INBOUND_TTL_MS = 72 * 60 * 60 * 1000;
const RECEIPT_TTL_MS = 24 * 60 * 60 * 1000;
const SENDS_PER_DAY = 100;
const SEND_BYTES_PER_DAY = 10 * 1024 * 1024;
const SENDS_PER_RECIPIENT_PER_DAY = 20;

export type MailKind = "osl_e2ee" | "external_envelope";

export interface StoredMailInput {
  ownerUserId: string;
  messageId: string;
  requestId: string;
  kind: MailKind;
  senderUserId: string | null;
  opaqueThreadToken: string;
  ciphertextB64: string;
  envelopeJson: string;
  recipientKeyFingerprint: string;
  receivedAt: number;
  expiresAt: number;
}

interface MessageRow extends Record<string, SqlStorageValue> {
  message_id: string;
  kind: MailKind;
  sender_user_id: string | null;
  opaque_thread_token: string;
  ciphertext_b64: string;
  envelope_json: string;
  recipient_key_fingerprint: string;
  received_at: number;
  expires_at: number;
  byte_length: number;
}

export class Mailbox extends DurableObject<Env> {
  private readonly sql: SqlStorage;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    ctx.blockConcurrencyWhile(async () => {
      this.ensureSchema();
    });
  }

  private ensureSchema(): void {
    this.sql.exec(`
      CREATE TABLE IF NOT EXISTS mailbox_state (
        key TEXT PRIMARY KEY, value TEXT NOT NULL
      ) WITHOUT ROWID;
      CREATE TABLE IF NOT EXISTS messages (
        message_id TEXT PRIMARY KEY,
        kind TEXT NOT NULL CHECK (kind IN ('osl_e2ee', 'external_envelope')),
        sender_user_id TEXT,
        opaque_thread_token TEXT NOT NULL,
        ciphertext_b64 TEXT NOT NULL,
        envelope_json TEXT NOT NULL,
        recipient_key_fingerprint TEXT NOT NULL,
        received_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL,
        byte_length INTEGER NOT NULL
      ) WITHOUT ROWID;
      CREATE INDEX IF NOT EXISTS idx_messages_received ON messages(received_at DESC);
      CREATE INDEX IF NOT EXISTS idx_messages_expiry ON messages(expires_at);
      CREATE TABLE IF NOT EXISTS request_receipts (
        request_id TEXT PRIMARY KEY,
        operation TEXT NOT NULL,
        message_id TEXT,
        outcome TEXT NOT NULL,
        expires_at INTEGER NOT NULL
      ) WITHOUT ROWID;
      CREATE INDEX IF NOT EXISTS idx_receipts_expiry ON request_receipts(expires_at);
      CREATE TABLE IF NOT EXISTS send_buckets (
        bucket_start INTEGER NOT NULL,
        recipient_user_id TEXT NOT NULL,
        send_count INTEGER NOT NULL,
        byte_count INTEGER NOT NULL,
        PRIMARY KEY (bucket_start, recipient_user_id)
      ) WITHOUT ROWID;
    `);
  }

  private assertOwner(ownerUserId: string): void {
    const row = this.sql.exec<{ value: string }>(
      "SELECT value FROM mailbox_state WHERE key = 'owner_user_id'",
    ).toArray()[0];
    if (row && row.value !== ownerUserId) throw new Error("mailbox owner mismatch");
    if (!row) {
      this.sql.exec(
        "INSERT INTO mailbox_state(key, value) VALUES ('owner_user_id', ?)",
        ownerUserId,
      );
    }
  }

  async reserveOutgoing(
    ownerUserId: string,
    requestId: string,
    recipientUserId: string,
    byteLength: number,
    now: number,
  ): Promise<{ ok: boolean; replay: boolean; reason?: string }> {
    this.assertOwner(ownerUserId);
    if (!Number.isSafeInteger(byteLength) || byteLength < 1 || byteLength > MAIL_MAX_CIPHERTEXT_BYTES) {
      return { ok: false, replay: false, reason: "message_too_large" };
    }
    const prior = this.sql.exec<{ outcome: string }>(
      "SELECT outcome FROM request_receipts WHERE request_id = ? AND operation = 'send'",
      requestId,
    ).toArray()[0];
    if (prior) return { ok: prior.outcome === "reserved" || prior.outcome === "stored", replay: true };

    const bucket = Math.floor(now / 86_400_000) * 86_400_000;
    const totals = this.sql.exec<{ sends: number; bytes: number }>(
      "SELECT COALESCE(SUM(send_count),0) sends, COALESCE(SUM(byte_count),0) bytes FROM send_buckets WHERE bucket_start = ?",
      bucket,
    ).one()!;
    const recipient = this.sql.exec<{ sends: number }>(
      "SELECT COALESCE(send_count,0) sends FROM send_buckets WHERE bucket_start = ? AND recipient_user_id = ?",
      bucket,
      recipientUserId,
    ).toArray()[0] ?? { sends: 0 };
    if (totals.sends >= SENDS_PER_DAY) return { ok: false, replay: false, reason: "daily_send_quota" };
    if (totals.bytes + byteLength > SEND_BYTES_PER_DAY) return { ok: false, replay: false, reason: "daily_byte_quota" };
    if (recipient.sends >= SENDS_PER_RECIPIENT_PER_DAY) return { ok: false, replay: false, reason: "recipient_rate_limit" };

    this.ctx.storage.transactionSync(() => {
      this.sql.exec(
        `INSERT INTO send_buckets(bucket_start, recipient_user_id, send_count, byte_count)
         VALUES (?, ?, 1, ?)
         ON CONFLICT(bucket_start, recipient_user_id) DO UPDATE SET
           send_count = send_count + 1, byte_count = byte_count + excluded.byte_count`,
        bucket,
        recipientUserId,
        byteLength,
      );
      this.sql.exec(
        "INSERT INTO request_receipts(request_id, operation, outcome, expires_at) VALUES (?, 'send', 'reserved', ?)",
        requestId,
        now + RECEIPT_TTL_MS,
      );
    });
    await this.scheduleNextAlarm();
    return { ok: true, replay: false };
  }

  async store(input: StoredMailInput): Promise<{ stored: boolean; replay: boolean }> {
    this.assertOwner(input.ownerUserId);
    const byteLength = base64DecodedLength(input.ciphertextB64);
    if (byteLength < 1 || byteLength > MAIL_MAX_CIPHERTEXT_BYTES) throw new Error("ciphertext bounds rejected");
    const maxTtl = input.kind === "external_envelope" ? EXTERNAL_INBOUND_TTL_MS : OSL_MAIL_TTL_MS;
    if (input.expiresAt <= input.receivedAt || input.expiresAt - input.receivedAt > maxTtl) {
      throw new Error("retention bounds rejected");
    }
    const prior = this.sql.exec<{ message_id: string | null }>(
      "SELECT message_id FROM request_receipts WHERE request_id = ? AND operation = 'store'",
      input.requestId,
    ).toArray()[0];
    if (prior) return { stored: prior.message_id === input.messageId, replay: true };
    const usage = this.sql.exec<{ count: number; bytes: number }>(
      "SELECT COUNT(*) count, COALESCE(SUM(byte_length),0) bytes FROM messages",
    ).one()!;
    if (usage.count >= MAIL_MAX_MESSAGES || usage.bytes + byteLength > MAIL_MAX_TOTAL_BYTES) {
      throw new Error("mailbox quota exceeded");
    }
    this.ctx.storage.transactionSync(() => {
      this.sql.exec(
        `INSERT INTO messages(message_id, kind, sender_user_id, opaque_thread_token,
          ciphertext_b64, envelope_json, recipient_key_fingerprint, received_at,
          expires_at, byte_length) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
        input.messageId,
        input.kind,
        input.senderUserId,
        input.opaqueThreadToken,
        input.ciphertextB64,
        input.envelopeJson,
        input.recipientKeyFingerprint,
        input.receivedAt,
        input.expiresAt,
        byteLength,
      );
      this.sql.exec(
        "INSERT INTO request_receipts(request_id, operation, message_id, outcome, expires_at) VALUES (?, 'store', ?, 'stored', ?)",
        input.requestId,
        input.messageId,
        input.receivedAt + RECEIPT_TTL_MS,
      );
    });
    await this.scheduleNextAlarm();
    return { stored: true, replay: false };
  }

  list(ownerUserId: string, limit = 50): Omit<MessageRow, "ciphertext_b64" | "envelope_json">[] {
    this.assertOwner(ownerUserId);
    const bounded = Math.max(1, Math.min(100, Math.trunc(limit)));
    return this.sql.exec<Omit<MessageRow, "ciphertext_b64" | "envelope_json">>(
      `SELECT message_id, kind, sender_user_id, opaque_thread_token,
       recipient_key_fingerprint, received_at, expires_at, byte_length
       FROM messages WHERE expires_at > ? ORDER BY received_at DESC LIMIT ?`,
      Date.now(),
      bounded,
    ).toArray();
  }

  fetchMessage(ownerUserId: string, messageId: string): MessageRow | null {
    this.assertOwner(ownerUserId);
    return this.sql.exec<MessageRow>(
      "SELECT * FROM messages WHERE message_id = ? AND expires_at > ?",
      messageId,
      Date.now(),
    ).toArray()[0] ?? null;
  }

  ack(ownerUserId: string, requestId: string, messageId: string, now: number): { deleted: boolean; replay: boolean } {
    this.assertOwner(ownerUserId);
    const prior = this.sql.exec<{ outcome: string }>(
      "SELECT outcome FROM request_receipts WHERE request_id = ? AND operation = 'ack'",
      requestId,
    ).toArray()[0];
    if (prior) return { deleted: prior.outcome === "deleted", replay: true };
    let deleted = false;
    this.ctx.storage.transactionSync(() => {
      const result = this.sql.exec("DELETE FROM messages WHERE message_id = ?", messageId);
      deleted = result.rowsWritten > 0;
      this.sql.exec(
        "INSERT INTO request_receipts(request_id, operation, message_id, outcome, expires_at) VALUES (?, 'ack', ?, ?, ?)",
        requestId,
        messageId,
        deleted ? "deleted" : "absent",
        now + RECEIPT_TTL_MS,
      );
    });
    return { deleted, replay: false };
  }

  deleteAll(ownerUserId: string, requestId: string, now: number): { deleted: number; receipt: string; replay: boolean } {
    this.assertOwner(ownerUserId);
    const prior = this.sql.exec<{ outcome: string }>(
      "SELECT outcome FROM request_receipts WHERE request_id = ? AND operation = 'burn'",
      requestId,
    ).toArray()[0];
    if (prior) return { deleted: 0, receipt: prior.outcome, replay: true };
    const count = this.sql.exec<{ count: number }>("SELECT COUNT(*) count FROM messages").one()!.count;
    const receipt = `burned:${requestId}:${now}`;
    this.ctx.storage.transactionSync(() => {
      this.sql.exec("DELETE FROM messages");
      this.sql.exec("DELETE FROM send_buckets");
      this.sql.exec("DELETE FROM request_receipts");
      this.sql.exec(
        "INSERT INTO request_receipts(request_id, operation, outcome, expires_at) VALUES (?, 'burn', ?, ?)",
        requestId,
        receipt,
        now + RECEIPT_TTL_MS,
      );
    });
    return { deleted: count, receipt, replay: false };
  }

  override async alarm(): Promise<void> {
    const now = Date.now();
    this.ctx.storage.transactionSync(() => {
      this.sql.exec("DELETE FROM messages WHERE expires_at <= ?", now);
      this.sql.exec("DELETE FROM request_receipts WHERE expires_at <= ?", now);
      this.sql.exec("DELETE FROM send_buckets WHERE bucket_start < ?", now - 86_400_000);
    });
    await this.scheduleNextAlarm();
  }

  private async scheduleNextAlarm(): Promise<void> {
    const row = this.sql.exec<{ next_at: number | null }>(
      `SELECT MIN(next_at) next_at FROM (
        SELECT MIN(expires_at) next_at FROM messages
        UNION ALL SELECT MIN(expires_at) FROM request_receipts
        UNION ALL SELECT MIN(bucket_start + 172800000) FROM send_buckets
      )`,
    ).one?.();
    if (row?.next_at) await this.ctx.storage.setAlarm(Math.max(Date.now() + 1_000, row.next_at));
    else await this.ctx.storage.deleteAlarm();
  }
}

function base64DecodedLength(value: string): number {
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(value)) return -1;
  const padding = value.endsWith("==") ? 2 : value.endsWith("=") ? 1 : 0;
  return Math.floor(value.length * 3 / 4) - padding;
}
