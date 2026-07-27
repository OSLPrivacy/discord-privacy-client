import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import worker from "../../src/index.js";

const EXPIRED_ROWS = 101;
const EXPECTED_PER_TICK = 100;

function randomBytes(size: number): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(size));
}

async function seedControlInboxBacklog(
  now: number,
): Promise<{ expiredInbox: number; expiredReceipts: number }> {
  const statements: D1PreparedStatement[] = [];
  for (let index = 0; index < EXPIRED_ROWS; index += 1) {
    const sender = `sweep-sender-${crypto.randomUUID()}`;
    statements.push(
      env.DB.prepare(
        `INSERT INTO control_inbox
           (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
         VALUES (?, ?, ?, 'scope', ?, ?, ?)`,
      ).bind(
        randomBytes(16),
        `sweep-recipient-${index}`,
        sender,
        randomBytes(8),
        now - 1,
        now - 60,
      ),
      env.DB.prepare(
        `INSERT INTO control_inbox_requests
           (sender_id, request_digest, inbox_id, recipient_id, expires_at)
         VALUES (?, ?, ?, ?, ?)`,
      ).bind(
        sender,
        randomBytes(32),
        randomBytes(16),
        `sweep-recipient-${index}`,
        now - 1,
      ),
    );
  }

  statements.push(
    env.DB.prepare(
      `INSERT INTO control_inbox
         (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
       VALUES (?, 'sweep-live-recipient', 'sweep-live-sender', 'scope', ?, ?, ?)`,
    ).bind(randomBytes(16), randomBytes(8), now + 3600, now),
    env.DB.prepare(
      `INSERT INTO control_inbox_requests
         (sender_id, request_digest, inbox_id, recipient_id, expires_at)
       VALUES ('sweep-live-sender', ?, ?, 'sweep-live-recipient', ?)`,
    ).bind(randomBytes(32), randomBytes(16), now + 3600),
  );

  for (let offset = 0; offset < statements.length; offset += 80) {
    await env.DB.batch(statements.slice(offset, offset + 80));
  }

  const expiredInbox = await env.DB.prepare(
    "SELECT COUNT(*) AS count FROM control_inbox WHERE expires_at < ?",
  ).bind(now).first<number>("count");
  const expiredReceipts = await env.DB.prepare(
    "SELECT COUNT(*) AS count FROM control_inbox_requests WHERE expires_at < ?",
  ).bind(now).first<number>("count");
  return { expiredInbox: expiredInbox ?? 0, expiredReceipts: expiredReceipts ?? 0 };
}

async function counts(now: number): Promise<{
  expiredInbox: number;
  expiredReceipts: number;
  liveInbox: number;
  liveReceipts: number;
}> {
  const [expiredInbox, expiredReceipts, liveInbox, liveReceipts] = await Promise.all([
    env.DB.prepare(
      "SELECT COUNT(*) AS count FROM control_inbox WHERE expires_at < ?",
    ).bind(now).first<number>("count"),
    env.DB.prepare(
      "SELECT COUNT(*) AS count FROM control_inbox_requests WHERE expires_at < ?",
    ).bind(now).first<number>("count"),
    env.DB.prepare(
      "SELECT COUNT(*) AS count FROM control_inbox WHERE expires_at >= ?",
    ).bind(now).first<number>("count"),
    env.DB.prepare(
      "SELECT COUNT(*) AS count FROM control_inbox_requests WHERE expires_at >= ?",
    ).bind(now).first<number>("count"),
  ]);
  return {
    expiredInbox: expiredInbox ?? 0,
    expiredReceipts: expiredReceipts ?? 0,
    liveInbox: liveInbox ?? 0,
    liveReceipts: liveReceipts ?? 0,
  };
}

async function runHourlyCron(now: number): Promise<void> {
  await worker.scheduled(
    {
      cron: "17 * * * *",
      scheduledTime: now * 1000,
      noRetry() {},
    } as ScheduledController,
    env,
    {} as ExecutionContext,
  );
}

describe("control-inbox scheduled cleanup", () => {
  it("makes bounded progress per tick and never deletes live rows", async () => {
    const now = Math.floor(Date.now() / 1000);
    expect(await seedControlInboxBacklog(now)).toEqual({
      expiredInbox: EXPIRED_ROWS,
      expiredReceipts: EXPIRED_ROWS,
    });
    expect(await counts(now)).toEqual({
      expiredInbox: EXPIRED_ROWS,
      expiredReceipts: EXPIRED_ROWS,
      liveInbox: 1,
      liveReceipts: 1,
    });

    await runHourlyCron(now);
    expect(await counts(now)).toEqual({
      expiredInbox: EXPIRED_ROWS - EXPECTED_PER_TICK,
      expiredReceipts: EXPIRED_ROWS - EXPECTED_PER_TICK,
      liveInbox: 1,
      liveReceipts: 1,
    });

    await runHourlyCron(now);
    expect(await counts(now)).toEqual({
      expiredInbox: 0,
      expiredReceipts: 0,
      liveInbox: 1,
      liveReceipts: 1,
    });
  });
});
