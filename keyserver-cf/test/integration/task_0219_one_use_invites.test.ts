import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { base64Encode } from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;
const text = new TextEncoder();

function bytes(fill: number, length: number): Uint8Array {
  return new Uint8Array(length).fill(fill);
}

function hex(value: Uint8Array): string {
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function blobBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (
    Array.isArray(value) &&
    value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)
  ) {
    return Uint8Array.from(value);
  }
  throw new Error("D1 BLOB readback was not bytes");
}

async function sha256Hex(value: Uint8Array): Promise<string> {
  return hex(new Uint8Array(await crypto.subtle.digest("SHA-256", value)));
}

async function seedInvite(
  inviteId: Uint8Array,
  creator: Uint8Array,
  createdAt: number,
  expiresAt: number,
): Promise<void> {
  await testDb
    .prepare(
      `INSERT INTO one_use_invite_links
        (invite_id, creator, intended_use, created_at, expires_at)
       VALUES (?, ?, 'space_admission', ?, ?)`,
    )
    .bind(inviteId, creator, createdAt, expiresAt)
    .run();
}

async function pendingCount(creator: Uint8Array): Promise<number> {
  const row = await testDb
    .prepare(
      "SELECT COUNT(*) AS count FROM space_event_queue WHERE recipient_tag = ?",
    )
    .bind(creator)
    .first<{ count: number }>();
  return row?.count ?? 0;
}

async function pendingFingerprint(creator: Uint8Array): Promise<string> {
  const row = await testDb
    .prepare(
      "SELECT ciphertext FROM space_event_queue WHERE recipient_tag = ?",
    )
    .bind(creator)
    .first<{ ciphertext: Uint8Array }>();
  if (!row?.ciphertext) throw new Error("pending request not found");
  return await sha256Hex(blobBytes(row.ciphertext));
}

async function redeem(inviteId: Uint8Array, requestCiphertext: Uint8Array): Promise<Response> {
  return await SELF.fetch("http://test/v1/space-invites/redeem", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      invite_id: base64Encode(inviteId),
      request_ciphertext: base64Encode(requestCiphertext),
    }),
  });
}

function audit(line: string): void {
  process.stdout.write(`${line}\n`);
}

describe("TASK 0219 one-use Space invites reject reuse and expiry", () => {
  it("redeems PLUM-0219 once and refuses only-used and only-expired variants", async () => {
    const label = "PLUM-0219";
    const requestName = "REQ-0219";
    const inviteId = text.encode("PLUM-0219-live01");
    const expiredCopyInviteId = text.encode("PLUM-0219-copy01");
    const creator = bytes(0x21, 32);
    const requestCiphertext = text.encode(requestName);
    const createdAt = Math.floor(Date.now() / 1000) - 60;
    const expiresAt = createdAt + 3600;

    await seedInvite(inviteId, creator, createdAt, expiresAt);
    const readable = await testDb
      .prepare(
        "SELECT lower(hex(invite_id)) AS invite_id, consumed_at FROM one_use_invite_links WHERE invite_id = ?",
      )
      .bind(inviteId)
      .first<{ invite_id: string; consumed_at: number | null }>();
    expect(readable?.invite_id).toBe(hex(inviteId));
    expect(readable?.consumed_at).toBeNull();

    const countBefore = await pendingCount(creator);
    expect(countBefore).toBe(0);

    const firstUse = await redeem(inviteId, requestCiphertext);
    if (firstUse.status !== 202) {
      throw new Error(`first redemption failed: ${firstUse.status} ${await firstUse.text()}`);
    }
    const firstBody = (await firstUse.json()) as {
      accepted: boolean;
      request_fingerprint: string;
    };
    const countAfterFirst = await pendingCount(creator);
    const fingerprintAfterFirst = await pendingFingerprint(creator);
    expect(firstBody.accepted).toBe(true);
    expect(firstBody.request_fingerprint).toBe(fingerprintAfterFirst);
    expect(countAfterFirst).toBe(1);

    const usedRetry = await redeem(inviteId, requestCiphertext);
    expect(usedRetry.status).toBe(409);
    const usedError = ((await usedRetry.json()) as { error: string }).error;
    expect(usedError).toBe("invite already used");

    await seedInvite(expiredCopyInviteId, creator, createdAt, expiresAt);
    await testDb
      .prepare("UPDATE one_use_invite_links SET expires_at = ? WHERE invite_id = ?")
      .bind(Math.floor(Date.now() / 1000) - 1, expiredCopyInviteId)
      .run();
    const expiredRetry = await redeem(expiredCopyInviteId, requestCiphertext);
    expect(expiredRetry.status).toBe(410);
    const expiredError = ((await expiredRetry.json()) as { error: string }).error;
    expect(expiredError).toBe("invite expired");

    const countAfterRefusals = await pendingCount(creator);
    const fingerprintAfterRefusals = await pendingFingerprint(creator);
    expect(countAfterRefusals).toBe(1);
    expect(fingerprintAfterRefusals).toBe(fingerprintAfterFirst);

    audit(`TASK0219_INVITE=${label}`);
    audit(`TASK0219_INVITE_READABLE=${readable !== null}`);
    audit(`TASK0219_PENDING_COUNT_BEFORE=${countBefore}`);
    audit(`TASK0219_FIRST_USE_REQUEST=${requestName}`);
    audit(`TASK0219_PENDING_COUNT_AFTER_FIRST_USE=${countAfterFirst}`);
    audit(`TASK0219_CHANGED_USED_FIELD_REFUSAL=${usedError}`);
    audit("TASK0219_EXPIRED_COPY_CHANGED_ONLY=expires_at");
    audit(`TASK0219_CHANGED_EXPIRY_FIELD_REFUSAL=${expiredError}`);
    audit(`TASK0219_PENDING_COUNT_AFTER_REFUSALS=${countAfterRefusals}`);
    audit(`TASK0219_REQ_0219_FINGERPRINT_BEFORE=${fingerprintAfterFirst}`);
    audit(`TASK0219_REQ_0219_FINGERPRINT_AFTER=${fingerprintAfterRefusals}`);
  });
});
