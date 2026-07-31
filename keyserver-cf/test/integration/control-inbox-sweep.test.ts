import { env, SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import worker from "../../src/index.js";
import { registerTestUser } from "./helpers.js";

const EXPIRED_ROWS = 101;
const EXPECTED_PER_TICK = 100;

function randomBytes(size: number): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(size));
}

async function seedControlInboxBacklog(
  now: number,
): Promise<{ expiredInbox: number; expiredReceipts: number }> {
  const statements: D1PreparedStatement[] = [];
  const sender = `sweep-enabled-${crypto.randomUUID()}`;
  await registerTestUser(SELF, sender);
  for (let index = 0; index < EXPIRED_ROWS; index += 1) {
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
       VALUES (?, 'sweep-live-recipient', ?, 'scope', ?, ?, ?)`,
    ).bind(randomBytes(16), sender, randomBytes(8), now + 3600, now),
    env.DB.prepare(
      `INSERT INTO control_inbox_requests
         (sender_id, request_digest, inbox_id, recipient_id, expires_at)
       VALUES (?, ?, ?, 'sweep-live-recipient', ?)`,
    ).bind(sender, randomBytes(32), randomBytes(16), now + 3600),
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
  it("does not silently delete an expired row whose sender is lookup-disabled", async () => {
    const now = Math.floor(Date.now() / 1000);
    const senderId = `disabled-sweep-${crypto.randomUUID()}`;
    const recipientId = `disabled-recipient-${crypto.randomUUID()}`;
    await registerTestUser(SELF, senderId);
    await registerTestUser(SELF, recipientId);
    await env.DB.prepare(
      "UPDATE users SET identity_lookup_enabled = 0 WHERE user_id = ?",
    ).bind(senderId).run();

    const bundle = new TextEncoder().encode("authenticated-opaque-control-bytes");
    await env.DB.prepare(
      `INSERT INTO control_inbox
         (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
       VALUES (?, ?, ?, 'scope', ?, ?, ?)`,
    ).bind(
      randomBytes(16),
      recipientId,
      senderId,
      bundle,
      now - 1,
      now - 60,
    ).run();

    expect(
      await env.DB.prepare(
        "SELECT COUNT(*) AS count FROM control_inbox WHERE sender_id = ?",
      ).bind(senderId).first<number>("count"),
    ).toBe(1);

    await runHourlyCron(now);

    const retained = await env.DB.prepare(
      "SELECT bundle FROM control_inbox WHERE sender_id = ?",
    ).bind(senderId).first<{ bundle: unknown }>();
    expect(retained).not.toBeNull();
    expect(Array.from(retained?.bundle as number[])).toEqual(Array.from(bundle));

    // This file's D1 isolation is per file rather than per `it`; remove the
    // fixture so the separate 101-row batch assertion below measures only its
    // own backlog.
    await env.DB.prepare(
      "UPDATE users SET identity_lookup_enabled = 1 WHERE user_id = ?",
    ).bind(senderId).run();
    await runHourlyCron(now + 1);
    await env.DB.prepare(
      "DELETE FROM control_inbox WHERE sender_id = ?",
    ).bind(senderId).run();
  });

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

  it("classifies at most one bounded batch and retains the unclassified tail", async () => {
    const now = Math.floor(Date.now() / 1000);
    const senderPrefix = `missing-sweep-${crypto.randomUUID()}`;
    const recipientId = `missing-recipient-${crypto.randomUUID()}`;
    const statements: D1PreparedStatement[] = [];
    for (let index = 0; index < EXPIRED_ROWS; index += 1) {
      statements.push(
        env.DB.prepare(
          `INSERT INTO control_inbox
             (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)`,
        ).bind(
          randomBytes(16),
          recipientId,
          `${senderPrefix}-${Math.floor(index / 32)}`,
          `scope-${index}`,
          randomBytes(8),
          now - 1,
          now - 120 + index,
        ),
      );
    }
    for (let offset = 0; offset < statements.length; offset += 80) {
      await env.DB.batch(statements.slice(offset, offset + 80));
    }

    await runHourlyCron(now);
    const first = await env.DB.prepare(
      `SELECT COUNT(*) AS rows,
              SUM(delivery_status = 'retryable') AS retryable,
              SUM(delivery_status = 'live') AS live
         FROM control_inbox
        WHERE recipient_id = ?`,
    ).bind(recipientId).first<{
      rows: number;
      retryable: number;
      live: number;
    }>();
    expect(first).toEqual({
      rows: EXPIRED_ROWS,
      retryable: EXPECTED_PER_TICK,
      live: EXPIRED_ROWS - EXPECTED_PER_TICK,
    });

    await runHourlyCron(now);
    const second = await env.DB.prepare(
      `SELECT COUNT(*) AS rows,
              SUM(delivery_status = 'retryable') AS retryable,
              SUM(delivery_status = 'live') AS live
         FROM control_inbox
        WHERE recipient_id = ?`,
    ).bind(recipientId).first<{
      rows: number;
      retryable: number;
      live: number;
    }>();
    expect(second).toEqual({
      rows: EXPIRED_ROWS,
      retryable: EXPIRED_ROWS,
      live: 0,
    });
  });
});
