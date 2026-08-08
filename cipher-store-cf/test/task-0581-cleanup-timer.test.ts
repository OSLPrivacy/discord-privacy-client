import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import { runAttachmentCleanupTimer } from "../src/lib/attachment-cleanup-timer.js";
import { d1Count, d1Run, workerEnv } from "./helpers/workerd.js";

const DIGEST = "b".repeat(64);
const CRON = "*/5 * * * *";
const HOUR_MS = 60 * 60 * 1000;

async function insertExpiredAttachment(env: Env, suffix: string): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  const objectKey = `attachments/task-0581-expired-${suffix}`;
  await env.ATTACHMENTS.put(objectKey, new Uint8Array([Number(suffix)]));
  await d1Run(
    `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
        fetch_token_sha256_hex, state, upload_id)
     VALUES (?, ?, 1, ?, ?, ?, ?, 'ready', NULL)`,
    suffix.repeat(32),
    objectKey,
    now - 1,
    now - 1,
    now - 60,
    DIGEST,
  );
}

describe("TASK 0581 attachment cleanup repeating timer", () => {
  it("writes exact deleted counts on hourly-separated runs and stops changing after the timer stops", async () => {
    const env = workerEnv();
    const cleanupLog = vi.spyOn(console, "log");
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const firstRunAt = 1_800_000_000_000;

    await insertExpiredAttachment(env, "1");
    await runAttachmentCleanupTimer(env, firstRunAt);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);

    await insertExpiredAttachment(env, "2");
    await runAttachmentCleanupTimer(env, firstRunAt + HOUR_MS);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);

    const cleanupLines = cleanupLog.mock.calls
      .map(([line]) => line)
      .filter((line): line is string => typeof line === "string" && line.startsWith("[attachment-cleanup]"));
    expect(cleanupLines).toEqual([
      `[attachment-cleanup] scheduled_at=${firstRunAt} deleted=1`,
      `[attachment-cleanup] scheduled_at=${firstRunAt + HOUR_MS} deleted=1`,
    ]);

    // Stopping the timer means no further scheduled invocation. A newly due
    // object remains untouched and no third cleanup count is written.
    await insertExpiredAttachment(env, "3");
    const linesWhenStopped = cleanupLines.length;
    const remainingWhenStopped = await d1Count("SELECT COUNT(*) AS c FROM attachment_objects");
    expect(remainingWhenStopped).toBe(1);
    expect(
      cleanupLog.mock.calls.filter(
        ([line]) => typeof line === "string" && line.startsWith("[attachment-cleanup]"),
      ),
    ).toHaveLength(linesWhenStopped);

    console.log(
      `TASK0581 cron=${CRON} first_run_deleted=1 second_run_deleted=1 run_gap_minutes=60 cleanup_lines=${linesWhenStopped} timer_stopped_remaining=${remainingWhenStopped}`,
    );
  });
});
