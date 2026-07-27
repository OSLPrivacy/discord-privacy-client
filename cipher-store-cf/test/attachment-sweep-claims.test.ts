import { afterEach, describe, expect, it, vi } from "vitest";
import migration0008 from "../migrations/0008_attachment_sweep_claims.sql?raw";
import type { Env } from "../src/env.js";
import worker from "../src/index.js";
import {
  ATTACHMENT_SWEEP_LEASE_SECONDS,
  claimNextExpiredAttachment,
  completeAttachmentSweepClaim,
} from "../src/lib/attachment-sweep-claims.js";
import { NATURAL_CRON } from "../src/lib/d2-proof-contract.js";
import { sweepExpiredAttachments } from "../src/lib/sweep.js";
import {
  d1All,
  d1Count,
  d1First,
  d1Run,
  workerEnv,
} from "./helpers/workerd.js";

const DIGEST = "a".repeat(64);

function r2WithOverrides(
  real: R2Bucket,
  overrides: Record<PropertyKey, unknown>,
): R2Bucket {
  return new Proxy(real, {
    get(target, property, receiver) {
      if (property in overrides) return overrides[property];
      const value = Reflect.get(target, property, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

function d1WithCompletionFailure(real: D1Database): D1Database {
  let failed = false;
  return new Proxy(real, {
    get(target, property, receiver) {
      if (property !== "prepare") {
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      }
      return (sql: string) => {
        const prepared = target.prepare(sql);
        if (
          failed
          || !/DELETE FROM attachment_objects[\s\S]+RETURNING id/.test(sql)
        ) {
          return prepared;
        }
        return new Proxy(prepared, {
          get(statement, statementProperty, statementReceiver) {
            if (statementProperty !== "bind") {
              const value = Reflect.get(
                statement,
                statementProperty,
                statementReceiver,
              );
              return typeof value === "function" ? value.bind(statement) : value;
            }
            return (...values: unknown[]) => {
              const bound = statement.bind(...values);
              return new Proxy(bound, {
                get(boundStatement, boundProperty, boundReceiver) {
                  if (boundProperty === "first") {
                    return async () => {
                      failed = true;
                      throw new Error("D1 completion unavailable");
                    };
                  }
                  const value = Reflect.get(
                    boundStatement,
                    boundProperty,
                    boundReceiver,
                  );
                  return typeof value === "function"
                    ? value.bind(boundStatement)
                    : value;
                },
              });
            };
          },
        });
      };
    },
  });
}

async function insertExpiredReady(
  id: string,
  objectKey: string,
  expiresAt: number,
): Promise<void> {
  await d1Run(
    `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
        fetch_token_sha256_hex, state, upload_id)
     VALUES (?, ?, 1, ?, ?, ?, ?, 'ready', NULL)`,
    id,
    objectKey,
    expiresAt,
    expiresAt,
    expiresAt - 60,
    DIGEST,
  );
}

const scheduledEvent = {
  cron: NATURAL_CRON,
  scheduledTime: 300_000,
  type: "scheduled",
} as ScheduledEvent;
const executionContext = {} as ExecutionContext;

afterEach(() => {
  vi.restoreAllMocks();
});

describe("transactional attachment sweep claims", () => {
  it("pins the exact 0004/0006 object boundary and keeps 0008 inert for the old Worker", async () => {
    const objectColumns = await d1All<{ name: string }>(
      "PRAGMA table_info(attachment_objects)",
    );
    expect(objectColumns.map((column) => column.name)).toEqual([
      "id",
      "object_key",
      "size_bytes",
      "expires_at",
      "created_at",
      "fetch_token_sha256_hex",
      "state",
      "upload_id",
      "content_expires_at",
    ]);

    const uncommented = migration0008.replace(/--[^\n]*/g, "");
    const statements = uncommented
      .split(";")
      .map((statement) => statement.trim())
      .filter(Boolean);
    expect(statements).toHaveLength(2);
    expect(statements[0]).toMatch(/^CREATE TABLE attachment_sweep_claims/);
    expect(statements[1]).toMatch(/^CREATE INDEX idx_attachment_sweep_claims_retry/);
    expect(uncommented).not.toMatch(
      /\b(?:ALTER\s+TABLE|DROP\s+TABLE|INSERT\s+INTO|UPDATE\s+\w+|DELETE\s+FROM|CREATE\s+TRIGGER)\b/i,
    );

    const claimColumns = await d1All<{ name: string }>(
      "PRAGMA table_info(attachment_sweep_claims)",
    );
    expect(claimColumns.map((column) => column.name)).toEqual([
      "attachment_id",
      "worker_id",
      "claim_token",
      "lease_version",
      "lease_expires_at",
      "retry_not_before",
      "attempt_count",
      "last_claimed_at",
    ]);
    const foreignKeys = await d1All<{ table: string; from: string; to: string }>(
      "PRAGMA foreign_key_list(attachment_sweep_claims)",
    );
    expect(foreignKeys).toContainEqual(expect.objectContaining({
      table: "attachment_objects",
      from: "attachment_id",
      to: "id",
    }));
  });

  it("allows only one concurrent identity-bound claim for one object", async () => {
    const now = 1_800_000_000;
    const id = "1".repeat(32);
    await insertExpiredReady(id, "attachments/concurrent", now - 1);

    const claims = await Promise.all([
      claimNextExpiredAttachment(workerEnv(), "a".repeat(32), now),
      claimNextExpiredAttachment(workerEnv(), "b".repeat(32), now),
    ]);
    const winners = claims.filter((claim) => claim !== null);
    expect(winners).toHaveLength(1);
    expect(winners[0]).toMatchObject({
      attachment_id: id,
      lease_version: 1,
      attempt_count: 1,
    });
    expect(winners[0]!.claim_token).toMatch(/^[0-9a-f]{64}$/);
    expect(winners[0]!.worker_id).toMatch(/^[ab]{32}$/);
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_sweep_claims WHERE attachment_id = ?",
        id,
      ),
    ).toBe(1);
  });

  it("refuses stale identity, token, and version completion while completion is idempotent", async () => {
    const now = 1_800_000_100;
    const id = "2".repeat(32);
    await insertExpiredReady(id, "attachments/stale-completion", now - 1);
    const claim = await claimNextExpiredAttachment(
      workerEnv(),
      "c".repeat(32),
      now,
    );
    expect(claim).not.toBeNull();

    await expect(
      completeAttachmentSweepClaim(
        workerEnv(),
        { ...claim!, worker_id: "d".repeat(32) },
        now,
      ),
    ).resolves.toBe("stale");
    await expect(
      completeAttachmentSweepClaim(
        workerEnv(),
        { ...claim!, claim_token: "e".repeat(64) },
        now,
      ),
    ).resolves.toBe("stale");
    await expect(
      completeAttachmentSweepClaim(
        workerEnv(),
        { ...claim!, lease_version: claim!.lease_version + 1 },
        now,
      ),
    ).resolves.toBe("stale");
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
        id,
      ),
    ).toBe(1);

    await expect(
      completeAttachmentSweepClaim(workerEnv(), claim!, now),
    ).resolves.toBe("completed");
    await expect(
      completeAttachmentSweepClaim(workerEnv(), claim!, now),
    ).resolves.toBe("already_completed");
  });

  it("recovers an expired crashed lease monotonically and fences the crashed Worker", async () => {
    const now = 1_800_000_200;
    const id = "3".repeat(32);
    await insertExpiredReady(id, "attachments/crash-reclaim", now - 1);
    const crashed = await claimNextExpiredAttachment(
      workerEnv(),
      "1".repeat(32),
      now,
    );
    expect(crashed).not.toBeNull();

    const recovered = await claimNextExpiredAttachment(
      workerEnv(),
      "2".repeat(32),
      now + ATTACHMENT_SWEEP_LEASE_SECONDS + 1,
    );
    expect(recovered).toMatchObject({
      attachment_id: id,
      worker_id: "2".repeat(32),
      lease_version: crashed!.lease_version + 1,
      attempt_count: crashed!.attempt_count + 1,
    });
    expect(recovered!.claim_token).not.toBe(crashed!.claim_token);
    await expect(
      completeAttachmentSweepClaim(
        workerEnv(),
        crashed!,
        now + ATTACHMENT_SWEEP_LEASE_SECONDS + 1,
      ),
    ).resolves.toBe("stale");
    await expect(
      completeAttachmentSweepClaim(
        workerEnv(),
        recovered!,
        now + ATTACHMENT_SWEEP_LEASE_SECONDS + 1,
      ),
    ).resolves.toBe("completed");
  });

  it("refuses completion after lease expiry even before another Worker recovers it", async () => {
    const now = 1_800_000_300;
    const id = "6".repeat(32);
    await insertExpiredReady(id, "attachments/expired-lease", now - 1);
    const claim = await claimNextExpiredAttachment(
      workerEnv(),
      "6".repeat(32),
      now,
    );
    expect(claim).not.toBeNull();
    await expect(
      completeAttachmentSweepClaim(
        workerEnv(),
        claim!,
        claim!.lease_expires_at,
      ),
    ).resolves.toBe("stale");
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
        id,
      ),
    ).toBe(1);
  });

  it("isolates a poisoned object, backs it off once, and completes unrelated work", async () => {
    const now = Math.floor(Date.now() / 1000);
    const poisonId = "0".repeat(32);
    const healthyId = "f".repeat(32);
    const poisonKey = "attachments/poison";
    const healthyKey = "attachments/healthy";
    const real = workerEnv();
    await insertExpiredReady(poisonId, poisonKey, now - 2);
    await insertExpiredReady(healthyId, healthyKey, now - 1);
    await real.ATTACHMENTS.put(poisonKey, new Uint8Array([1]));
    await real.ATTACHMENTS.put(healthyKey, new Uint8Array([2]));

    const remove = vi.fn(async (key: string | string[]) => {
      if (key === poisonKey) throw new Error("poisoned R2 object");
      await real.ATTACHMENTS.delete(key);
    });
    const env = {
      ...real,
      ATTACHMENTS: r2WithOverrides(real.ATTACHMENTS, { delete: remove }),
    } as unknown as Env;

    await expect(sweepExpiredAttachments(env)).resolves.toEqual({
      claimed: 2,
      completed: 1,
      failed: 1,
    });
    expect(remove).toHaveBeenCalledTimes(2);
    expect(await real.ATTACHMENTS.head(poisonKey)).not.toBeNull();
    expect(await real.ATTACHMENTS.head(healthyKey)).toBeNull();
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
        poisonId,
      ),
    ).toBe(1);
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
        healthyId,
      ),
    ).toBe(0);
    const retry = await d1First<{
      worker_id: string | null;
      claim_token: string | null;
      lease_version: number;
      attempt_count: number;
      retry_not_before: number;
    }>(
      `SELECT worker_id, claim_token, lease_version, attempt_count,
              retry_not_before
         FROM attachment_sweep_claims WHERE attachment_id = ?`,
      poisonId,
    );
    expect(retry).toMatchObject({
      worker_id: null,
      claim_token: null,
      lease_version: 2,
      attempt_count: 1,
    });
    expect(retry.retry_not_before).toBeGreaterThan(now);
    await expect(
      claimNextExpiredAttachment(real, "9".repeat(32), now),
    ).resolves.toBeNull();
  });

  it("retains indexed metadata when D1 completion fails after idempotent R2 cleanup", async () => {
    const real = workerEnv();
    const now = Math.floor(Date.now() / 1000);
    const id = "7".repeat(32);
    const key = "attachments/d1-completion-failure";
    await insertExpiredReady(id, key, now - 1);
    await real.ATTACHMENTS.put(key, new Uint8Array([7]));
    const env = {
      ...real,
      DB: d1WithCompletionFailure(real.DB),
    } as Env;

    await expect(sweepExpiredAttachments(env)).resolves.toEqual({
      claimed: 1,
      completed: 0,
      failed: 1,
    });
    expect(await real.ATTACHMENTS.head(key)).toBeNull();
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
        id,
      ),
    ).toBe(1);
    const retained = await d1First<{
      worker_id: string | null;
      claim_token: string | null;
      retry_not_before: number;
    }>(
      `SELECT worker_id, claim_token, retry_not_before
         FROM attachment_sweep_claims WHERE attachment_id = ?`,
      id,
    );
    expect(retained.worker_id).toBeNull();
    expect(retained.claim_token).toBeNull();
    expect(retained.retry_not_before).toBeGreaterThan(now);

    await d1Run(
      "UPDATE attachment_sweep_claims SET retry_not_before = 0 WHERE attachment_id = ?",
      id,
    );
    await expect(sweepExpiredAttachments(real)).resolves.toEqual({
      claimed: 1,
      completed: 1,
      failed: 0,
    });
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
        id,
      ),
    ).toBe(0);
  });

  it("returns exact empty and nonempty success counts", async () => {
    const real = workerEnv();
    await expect(sweepExpiredAttachments(real)).resolves.toEqual({
      claimed: 0,
      completed: 0,
      failed: 0,
    });

    const now = Math.floor(Date.now() / 1000);
    const id = "4".repeat(32);
    const key = "attachments/nonempty";
    await insertExpiredReady(id, key, now - 1);
    await real.ATTACHMENTS.put(key, new Uint8Array([4]));
    await expect(sweepExpiredAttachments(real)).resolves.toEqual({
      claimed: 1,
      completed: 1,
      failed: 0,
    });
    expect(await real.ATTACHMENTS.head(key)).toBeNull();
    expect(
      await d1Count(
        "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
        id,
      ),
    ).toBe(0);
  });

  it("makes the shipping scheduled Worker refuse before R2 when 0008 is absent", async () => {
    const real = workerEnv();
    const now = Math.floor(Date.now() / 1000);
    const id = "5".repeat(32);
    const key = "attachments/migration-order";
    const upload = await real.ATTACHMENTS.createMultipartUpload(key);
    const originalExpiry = now + 24 * 60 * 60;
    await d1Run(
      `INSERT INTO attachment_objects
         (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
          fetch_token_sha256_hex, state, upload_id)
       VALUES (?, ?, 1, ?, NULL, ?, ?, 'uploading', ?)`,
      id,
      key,
      originalExpiry,
      now - 16 * 60,
      DIGEST,
      upload.uploadId,
    );
    await d1Run("DROP TABLE attachment_sweep_claims");

    const remove = vi.fn((value: string | string[]) =>
      real.ATTACHMENTS.delete(value)
    );
    const env = {
      ...real,
      ATTACHMENTS: r2WithOverrides(real.ATTACHMENTS, { delete: remove }),
    } as unknown as Env;
    const marker = vi.spyOn(console, "log").mockImplementation(() => undefined);
    const failure = vi.spyOn(console, "error").mockImplementation(() => undefined);

    await worker.scheduled(scheduledEvent, env, executionContext);

    expect(remove).not.toHaveBeenCalled();
    expect(marker).not.toHaveBeenCalled();
    expect(failure).toHaveBeenCalledWith("[attachment-sweep] failed");
    const retained = await d1First<{ expires_at: number; upload_id: string }>(
      "SELECT expires_at, upload_id FROM attachment_objects WHERE id = ?",
      id,
    );
    expect(retained).toEqual({
      expires_at: originalExpiry,
      upload_id: upload.uploadId,
    });
    await expect(upload.uploadPart(1, new Uint8Array([5]))).resolves.toMatchObject({
      partNumber: 1,
    });
  });
});
