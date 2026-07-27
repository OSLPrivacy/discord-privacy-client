import { afterEach, describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import worker from "../src/index.js";
import { INCOMPLETE_SESSION_TTL_SECONDS } from "../src/lib/attachment-limits.js";
import { CYCLE_MARKER, NATURAL_CRON } from "../src/lib/d2-proof-contract.js";
import { d1Count, d1Run, workerEnv } from "./helpers/workerd.js";

const DIGEST = "a".repeat(64);

function r2WithOverrides(real: R2Bucket, overrides: Record<PropertyKey, unknown>): R2Bucket {
  return new Proxy(real, {
    get(target, property, receiver) {
      if (property in overrides) return overrides[property];
      const value = Reflect.get(target, property, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

async function insertStaleLegacy(
  id: string,
  objectKey: string,
  uploadId: string,
): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await d1Run(
    `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
        fetch_token_sha256_hex, state, upload_id)
     VALUES (?, ?, 1, ?, NULL, ?, ?, 'uploading', ?)`,
    id,
    objectKey,
    now + 24 * 60 * 60,
    now - INCOMPLETE_SESSION_TTL_SECONDS - 1,
    DIGEST,
    uploadId,
  );
}

const event = {
  cron: NATURAL_CRON,
  scheduledTime: 300_000,
  type: "scheduled",
} as ScheduledEvent;
const context = {} as ExecutionContext;

afterEach(() => {
  vi.restoreAllMocks();
});

describe("natural attachment sweep witness", () => {
  it("emits the fixed marker only after R2 abort and D1 removal succeed", async () => {
    const real = workerEnv();
    const id = "7".repeat(32);
    const objectKey = "attachments/scheduled-proof-success";
    const upload = await real.ATTACHMENTS.createMultipartUpload(objectKey);
    await insertStaleLegacy(id, objectKey, upload.uploadId);
    await d1Run(
      `UPDATE attachment_predecessor_adoption
          SET migration_started_at = unixepoch() - 2000,
              eligible_created_through = unixepoch() - 1000
        WHERE singleton = 1`,
    );
    await upload.uploadPart(1, new Uint8Array([7]));
    await real.ATTACHMENTS.put(objectKey, new Uint8Array([7, 7]));
    await d1Run(
      `UPDATE attachment_objects
          SET state = 'completing',
              content_expires_at = ?,
              expires_at = ?
        WHERE id = ?`,
      Math.floor(Date.now() / 1000) + 3600,
      Math.floor(Date.now() / 1000) - 1,
      id,
    );
    expect(
      await d1Count(
        `SELECT COUNT(*) AS c
           FROM attachment_objects AS object_row
           JOIN attachment_predecessor_adoption AS adoption
             ON adoption.singleton = 1
          WHERE object_row.id = ?
            AND object_row.created_at > adoption.eligible_created_through`,
        id,
      ),
    ).toBe(1);

    const marker = vi.spyOn(console, "log").mockImplementation(() => undefined);
    let abortFinished = false;
    const resume = vi.fn((key: string, uploadId: string) => {
      const multipart = real.ATTACHMENTS.resumeMultipartUpload(key, uploadId);
      return new Proxy(multipart, {
        get(target, property, receiver) {
          if (property === "abort") {
            return async () => {
              expect(marker).not.toHaveBeenCalledWith(CYCLE_MARKER);
              expect(
                await d1Count(
                  "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
                  id,
                ),
              ).toBe(1);
              await target.abort();
              abortFinished = true;
            };
          }
          const value = Reflect.get(target, property, receiver);
          return typeof value === "function" ? value.bind(target) : value;
        },
      });
    });
    const remove = vi.fn(async (key: string | string[]) => {
      expect(abortFinished).toBe(true);
      await real.ATTACHMENTS.delete(key);
    });
    const env = {
      ...real,
      ATTACHMENTS: r2WithOverrides(real.ATTACHMENTS, {
        resumeMultipartUpload: resume,
        delete: remove,
      }),
    } as unknown as Env;

    await worker.scheduled(event, env, context);

    expect(resume).toHaveBeenCalledWith(objectKey, upload.uploadId);
    expect(remove).toHaveBeenCalledWith(objectKey);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?", id)).toBe(0);
    expect(await real.ATTACHMENTS.head(objectKey)).toBeNull();
    expect(marker).toHaveBeenCalledTimes(1);
    expect(marker).toHaveBeenCalledWith(CYCLE_MARKER);
  });

  it("emits no success marker and retains retryable metadata when R2 abort fails", async () => {
    const real = workerEnv();
    const id = "8".repeat(32);
    const objectKey = "attachments/scheduled-proof-retry";
    const upload = await real.ATTACHMENTS.createMultipartUpload(objectKey);
    await insertStaleLegacy(id, objectKey, upload.uploadId);

    const marker = vi.spyOn(console, "log").mockImplementation(() => undefined);
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const abort = vi.fn().mockRejectedValue(new Error("R2 unavailable"));
    const env = {
      ...real,
      ATTACHMENTS: r2WithOverrides(real.ATTACHMENTS, {
        resumeMultipartUpload: vi.fn(() => ({ abort })),
      }),
    } as unknown as Env;

    await worker.scheduled(event, env, context);

    expect(abort).toHaveBeenCalledOnce();
    expect(marker).not.toHaveBeenCalledWith(CYCLE_MARKER);
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ? AND expires_at < unixepoch()",
        id,
      ),
    ).toBe(1);
  });

});
