import { env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import {
  claimNextExpiredAttachment,
  completeAttachmentSweepClaim,
  newAttachmentSweepWorkerId,
} from "../src/lib/attachment-sweep-claims.js";
import {
  drainTerminalRetentionReports,
  recordAttachmentCleanupFailure,
  UnrecoverableRetentionCleanupError,
} from "../src/lib/retention-cleanup-recovery.js";

const DIGEST = "a".repeat(64);
const BOT_TOKEN = "1234567890:abcdefghijklmnopqrstuvwxyzABCDE";

async function seed(id: string, expiresAt: number): Promise<void> {
  await env.DB.prepare(
    `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
        fetch_token_sha256_hex, state, upload_id)
     VALUES (?, ?, 1, ?, ?, ?, ?, 'ready', NULL)`,
  ).bind(id, `attachments/${id}`, expiresAt, expiresAt, expiresAt - 60, DIGEST).run();
  await env.ATTACHMENTS.put(`attachments/${id}`, new Uint8Array([1]));
}

function configuredEnv(): Env {
  return {
    ...env,
    TELEGRAM_BOT_TOKEN: BOT_TOKEN,
    TELEGRAM_OPERATOR_CHAT_IDS: "1122334455",
  } as Env;
}

describe("TASK 6578 unattended retention recovery", () => {
  it("retries a transient failure after restart and completes idempotently", async () => {
    const id = "1".repeat(32);
    let now = 2_000_000_000;
    await seed(id, now - 1);
    const first = await claimNextExpiredAttachment(env as Env, newAttachmentSweepWorkerId(), now);
    expect(first?.attempt_count).toBe(1);
    await expect(recordAttachmentCleanupFailure(
      env as Env,
      first!,
      new Error("provider timeout"),
      now,
    )).resolves.toBe("retry");

    // A fresh Worker identity models scheduler/service/machine restart. The
    // authoritative attachment row is rediscovered after durable backoff.
    now += 301;
    const recovered = await claimNextExpiredAttachment(env as Env, newAttachmentSweepWorkerId(), now);
    expect(recovered?.attempt_count).toBe(2);
    await env.ATTACHMENTS.delete(recovered!.object_key);
    await expect(completeAttachmentSweepClaim(env as Env, recovered!, now))
      .resolves.toBe("completed");
    await expect(completeAttachmentSweepClaim(env as Env, recovered!, now))
      .resolves.toBe("already_completed");
    expect(await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM attachment_objects WHERE id = ?",
    ).bind(id).first()).toEqual({ count: 0 });
  });

  it("reports exactly once only after three matching unrecoverable observations", async () => {
    const id = "2".repeat(32);
    let now = 2_100_000_000;
    await seed(id, now - 10);
    for (let attempt = 1; attempt <= 3; attempt += 1) {
      const claim = await claimNextExpiredAttachment(env as Env, newAttachmentSweepWorkerId(), now);
      expect(claim?.attempt_count).toBe(attempt);
      const disposition = await recordAttachmentCleanupFailure(
        env as Env,
        claim!,
        new UnrecoverableRetentionCleanupError("policy_lock_permanent"),
        now,
      );
      expect(disposition).toBe(attempt === 3 ? "terminal" : "retry");
      const report = await env.DB.prepare(
        `SELECT report_status FROM attachment_sweep_claims WHERE attachment_id = ?`,
      ).bind(id).first<{ report_status: string }>();
      expect(report?.report_status).toBe(attempt === 3 ? "pending" : "none");
      now += attempt === 1 ? 301 : 601;
    }
    await expect(claimNextExpiredAttachment(env as Env, newAttachmentSweepWorkerId(), now))
      .resolves.toBeNull();

    const calls: Array<{ url: string; body: Record<string, unknown> }> = [];
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ url: String(input), body: JSON.parse(String(init?.body)) });
      return Response.json({ ok: true });
    }) as unknown as typeof fetch;
    await expect(drainTerminalRetentionReports(configuredEnv(), fetcher, now))
      .resolves.toEqual({ attempted: 1, delivered: 1 });
    await expect(drainTerminalRetentionReports(configuredEnv(), fetcher, now + 1))
      .resolves.toEqual({ attempted: 0, delivered: 0 });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url).toBe(`https://api.telegram.org/bot${BOT_TOKEN}/sendMessage`);
    const text = String(calls[0]!.body.text);
    expect(text).toContain("policy=attachment_retention");
    expect(text).toContain(`object=${id}`);
    expect(text).toContain(`oldest_due_item=${id}`);
    expect(text).toContain("attempts=3");
    expect(text).toContain("terminal_reason=policy_lock_permanent");
  });
});
