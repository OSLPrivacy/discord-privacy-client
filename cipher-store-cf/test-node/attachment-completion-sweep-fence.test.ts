import { afterEach, describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import {
  handleAttachmentComplete,
  handleAttachmentPartUpload,
  handleAttachmentSessionCreate,
} from "../src/endpoints/attachment.js";
import {
  ATTACHMENT_SWEEP_LEASE_SECONDS,
  acquireAttachmentCompletionClaim,
  claimNextExpiredAttachment,
  finalizeAttachmentReadyClaim,
} from "../src/lib/attachment-sweep-claims.js";
import { sweepExpiredAttachments } from "../src/lib/sweep.js";
import { memoryR2, migratedD1 } from "../test/helpers/d1.js";

const TOKEN = "a".repeat(32);
const DIGEST = "b".repeat(64);
const START = 1_800_000_000;

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((accept) => {
    resolve = accept;
  });
  return { promise, resolve };
}

function permutations<T>(values: T[]): T[][] {
  if (values.length <= 1) return [values];
  return values.flatMap((value, index) =>
    permutations(values.filter((_, candidate) => candidate !== index))
      .map((tail) => [value, ...tail])
  );
}

function envWith(
  db: D1Database,
  bucket: R2Bucket,
): Env {
  return {
    DB: db,
    ATTACHMENTS: bucket,
  } as Env;
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("multipart completion versus sweep fencing", () => {
  it("refuses active completion ownership and unlineaged completing state", async () => {
    const database = migratedD1();
    const storage = memoryR2();
    const env = envWith(database.d1, storage.bucket);
    const activeId = "a".repeat(32);
    const unlineagedId = "b".repeat(32);
    database.exec(
      `INSERT INTO attachment_objects
         (id, object_key, size_bytes, expires_at, content_expires_at,
          created_at, fetch_token_sha256_hex, state, upload_id)
       VALUES (?, ?, 1, ?, ?, ?, ?, 'uploading', 'upload-active')`,
      activeId,
      "attachments/active-completion",
      START + 10,
      START + 3600,
      START,
      DIGEST,
    );
    const completion = await acquireAttachmentCompletionClaim(
      env,
      activeId,
      "a".repeat(32),
      START,
    );
    expect(completion).not.toBeNull();
    database.exec(
      "UPDATE attachment_objects SET expires_at = ? WHERE id = ?",
      START - 1,
      activeId,
    );
    await expect(
      claimNextExpiredAttachment(env, "e".repeat(32), START + 1),
    ).resolves.toBeNull();

    database.exec(
      `INSERT INTO attachment_objects
         (id, object_key, size_bytes, expires_at, content_expires_at,
          created_at, fetch_token_sha256_hex, state, upload_id)
       VALUES (?, ?, 1, ?, ?, ?, ?, 'completing', 'upload-unlineaged')`,
      unlineagedId,
      "attachments/unlineaged-completing",
      START - 1,
      START + 3600,
      START,
      DIGEST,
    );
    await expect(
      claimNextExpiredAttachment(env, "f".repeat(32), START + 1),
    ).resolves.toBeNull();
  });

  it("fails before R2 completion when the shared claim schema is absent", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(START * 1000);
    const database = migratedD1();
    const storage = memoryR2();
    const complete = vi.fn();
    const bucket = new Proxy(storage.bucket, {
      get(target, property, receiver) {
        if (property === "resumeMultipartUpload") {
          return (key: string, uploadId: string) => {
            const upload = target.resumeMultipartUpload(key, uploadId);
            return new Proxy(upload, {
              get(uploadTarget, uploadProperty, uploadReceiver) {
                if (uploadProperty === "complete") return complete;
                const value = Reflect.get(
                  uploadTarget,
                  uploadProperty,
                  uploadReceiver,
                );
                return typeof value === "function"
                  ? value.bind(uploadTarget)
                  : value;
              },
            });
          };
        }
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      },
    }) as R2Bucket;
    const env = envWith(database.d1, bucket);
    const sessionResponse = await handleAttachmentSessionCreate(
      new Request("https://cipher.test/v1/attachment/session", {
        method: "POST",
        headers: {
          "x-osl-ttl-seconds": "3600",
          "x-osl-fetch-token": TOKEN,
          "x-osl-size-bytes": "1",
        },
      }),
      env,
    );
    const session = await sessionResponse.json() as { id: string };
    await expect(
      handleAttachmentPartUpload(
        new Request(
          `https://cipher.test/v1/attachment/${session.id}/part/1`,
          {
            method: "PUT",
            headers: {
              "content-length": "1",
              "x-osl-fetch-token": TOKEN,
            },
            body: new Uint8Array([1]),
            duplex: "half",
          } as RequestInit & { duplex: "half" },
        ),
        env,
        session.id,
        1,
      ),
    ).resolves.toMatchObject({ status: 201 });
    database.raw.exec("DROP TABLE attachment_sweep_claims");

    await expect(
      handleAttachmentComplete(
        new Request(
          `https://cipher.test/v1/attachment/${session.id}/complete`,
          {
            method: "POST",
            headers: { "x-osl-fetch-token": TOKEN },
          },
        ),
        env,
        session.id,
      ),
    ).rejects.toThrow(/attachment_sweep_claims/);
    expect(complete).not.toHaveBeenCalled();
  });

  it("makes the final ready CAS compare the intervening sweep version", async () => {
    const database = migratedD1();
    const storage = memoryR2();
    const env = envWith(database.d1, storage.bucket);
    const id = "c".repeat(32);
    const objectKey = "attachments/version-fence";
    database.exec(
      `INSERT INTO attachment_objects
         (id, object_key, size_bytes, expires_at, content_expires_at,
          created_at, fetch_token_sha256_hex, state, upload_id)
       VALUES (?, ?, 1, ?, ?, ?, ?, 'uploading', 'upload-version-fence')`,
      id,
      objectKey,
      START + 10,
      START + 3600,
      START,
      DIGEST,
    );
    const original = await acquireAttachmentCompletionClaim(
      env,
      id,
      "1".repeat(32),
      START,
    );
    expect(original).not.toBeNull();
    const reclaimAt = START + ATTACHMENT_SWEEP_LEASE_SECONDS + 1;
    const recovered = await claimNextExpiredAttachment(
      env,
      "2".repeat(32),
      reclaimAt,
    );
    expect(recovered).toMatchObject({
      attachment_id: id,
      lease_version: original!.lease_version + 1,
      state: "completing",
    });
    await storage.bucket.put(objectKey, new Uint8Array([1]));

    await expect(
      finalizeAttachmentReadyClaim(env, original!, reclaimAt),
    ).resolves.toBe("stale");
    expect(
      database.raw.prepare(
        "SELECT state FROM attachment_objects WHERE id = ?",
      ).get(id),
    ).toEqual({ state: "completing" });
    await expect(
      finalizeAttachmentReadyClaim(env, recovered!, reclaimAt),
    ).resolves.toBe("ready");
    expect(await storage.bucket.head(objectKey)).toMatchObject({ size: 1 });
  });

  it("closes authorize -> lease expiry -> R2 complete -> sweep recovery -> final CAS", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(START * 1000);
    const database = migratedD1();
    const storage = memoryR2();
    const completionEntered = deferred();
    const allowR2Complete = deferred();
    const r2Committed = deferred();
    const allowCompletionReturn = deferred();
    let gateCompletion = false;
    const deletes: string[] = [];

    const bucket = new Proxy(storage.bucket, {
      get(target, property, receiver) {
        if (property === "delete") {
          return async (key: string | string[]) => {
            deletes.push(...(Array.isArray(key) ? key : [key]));
            return target.delete(key);
          };
        }
        if (property === "resumeMultipartUpload") {
          return (key: string, uploadId: string) => {
            const upload = target.resumeMultipartUpload(key, uploadId);
            if (!gateCompletion) return upload;
            return new Proxy(upload, {
              get(uploadTarget, uploadProperty, uploadReceiver) {
                if (uploadProperty !== "complete") {
                  const value = Reflect.get(
                    uploadTarget,
                    uploadProperty,
                    uploadReceiver,
                  );
                  return typeof value === "function"
                    ? value.bind(uploadTarget)
                    : value;
                }
                return async (
                  parts: Array<{ partNumber: number; etag: string }>,
                ) => {
                  completionEntered.resolve();
                  await allowR2Complete.promise;
                  const completed = await uploadTarget.complete(parts);
                  r2Committed.resolve();
                  await allowCompletionReturn.promise;
                  return completed;
                };
              },
            });
          };
        }
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      },
    }) as R2Bucket;
    const env = envWith(database.d1, bucket);

    const sessionResponse = await handleAttachmentSessionCreate(
      new Request("https://cipher.test/v1/attachment/session", {
        method: "POST",
        headers: {
          "x-osl-ttl-seconds": "3600",
          "x-osl-fetch-token": TOKEN,
          "x-osl-size-bytes": "1",
        },
      }),
      env,
    );
    const session = await sessionResponse.json() as { id: string };
    const part = new Uint8Array([7]);
    const partResponse = await handleAttachmentPartUpload(
      new Request(
        `https://cipher.test/v1/attachment/${session.id}/part/1`,
        {
          method: "PUT",
          headers: {
            "content-length": "1",
            "x-osl-fetch-token": TOKEN,
          },
          body: part,
          duplex: "half",
        } as RequestInit & { duplex: "half" },
      ),
      env,
      session.id,
      1,
    );
    expect(partResponse.status).toBe(201);

    gateCompletion = true;
    const completionPromise = handleAttachmentComplete(
      new Request(
        `https://cipher.test/v1/attachment/${session.id}/complete`,
        {
          method: "POST",
          headers: { "x-osl-fetch-token": TOKEN },
        },
      ),
      env,
      session.id,
    );
    await completionEntered.promise;

    vi.setSystemTime((START + 901) * 1000);
    allowR2Complete.resolve();
    await r2Committed.promise;
    const sweep = await sweepExpiredAttachments(env);
    expect(sweep).toEqual({ claimed: 1, completed: 1, failed: 0 });
    expect(deletes).toEqual([]);

    allowCompletionReturn.resolve();
    const completion = await completionPromise;
    expect(completion.status).toBe(200);
    const row = database.raw.prepare(
      `SELECT object_key, size_bytes, state, upload_id
         FROM attachment_objects WHERE id = ?`,
    ).get(session.id) as {
      object_key: string;
      size_bytes: number;
      state: string;
      upload_id: string | null;
    };
    expect(row).toMatchObject({
      size_bytes: 1,
      state: "ready",
      upload_id: null,
    });
    expect(await storage.bucket.head(row.object_key)).toMatchObject({
      size: row.size_bytes,
    });
  });

  it("recovers when completion commits between the sweep HEAD and abort", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(START * 1000);
    const database = migratedD1();
    const storage = memoryR2();
    const firstHeadObserved = deferred();
    const releaseStaleHead = deferred();
    const deletes: string[] = [];
    let headCalls = 0;
    const bucket = new Proxy(storage.bucket, {
      get(target, property, receiver) {
        if (property === "head") {
          return async (key: string) => {
            headCalls += 1;
            if (headCalls === 1) {
              firstHeadObserved.resolve();
              await releaseStaleHead.promise;
              return null;
            }
            return target.head(key);
          };
        }
        if (property === "delete") {
          return async (key: string | string[]) => {
            deletes.push(...(Array.isArray(key) ? key : [key]));
            return target.delete(key);
          };
        }
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      },
    }) as R2Bucket;
    const env = envWith(database.d1, bucket);
    const id = "d".repeat(32);
    const objectKey = "attachments/head-complete-abort";
    const multipart = await storage.bucket.createMultipartUpload(objectKey);
    const part = await multipart.uploadPart(1, new Uint8Array([9]));
    database.exec(
      `INSERT INTO attachment_objects
         (id, object_key, size_bytes, expires_at, content_expires_at,
          created_at, fetch_token_sha256_hex, state, upload_id)
       VALUES (?, ?, 1, ?, ?, ?, ?, 'uploading', ?)`,
      id,
      objectKey,
      START + 10,
      START + 3600,
      START,
      DIGEST,
      multipart.uploadId,
    );
    database.exec(
      `INSERT INTO attachment_parts
         (attachment_id, part_number, size_bytes, etag)
       VALUES (?, 1, 1, ?)`,
      id,
      part.etag,
    );
    const originalClaim = await acquireAttachmentCompletionClaim(
      env,
      id,
      "d".repeat(32),
      START,
    );
    expect(originalClaim).not.toBeNull();

    const reclaimAt = START + ATTACHMENT_SWEEP_LEASE_SECONDS + 1;
    vi.setSystemTime(reclaimAt * 1000);
    const sweepPromise = sweepExpiredAttachments(env);
    await firstHeadObserved.promise;
    await multipart.complete([{ partNumber: 1, etag: part.etag }]);
    releaseStaleHead.resolve();
    await expect(sweepPromise).resolves.toEqual({
      claimed: 1,
      completed: 1,
      failed: 0,
    });
    await expect(
      finalizeAttachmentReadyClaim(env, originalClaim!, reclaimAt),
    ).resolves.toBe("stale");
    const row = database.raw.prepare(
      "SELECT state, object_key, size_bytes FROM attachment_objects WHERE id = ?",
    ).get(id) as {
      state: string;
      object_key: string;
      size_bytes: number;
    };
    expect(row.state).toBe("ready");
    expect(await storage.bucket.head(row.object_key)).toMatchObject({
      size: row.size_bytes,
    });
    expect(deletes).toEqual([]);
  });

  it("preserves the no-loss invariant for every complete/sweep/final-CAS order", async () => {
    vi.useFakeTimers();
    const orders = permutations(["complete", "sweep", "finalize"] as const);
    expect(orders).toHaveLength(6);

    for (const [index, order] of orders.entries()) {
      const now = START + index * 10_000;
      vi.setSystemTime(now * 1000);
      const database = migratedD1();
      const storage = memoryR2();
      const deletes: string[] = [];
      const bucket = new Proxy(storage.bucket, {
        get(target, property, receiver) {
          if (property === "delete") {
            return async (key: string | string[]) => {
              deletes.push(...(Array.isArray(key) ? key : [key]));
              return target.delete(key);
            };
          }
          const value = Reflect.get(target, property, receiver);
          return typeof value === "function" ? value.bind(target) : value;
        },
      }) as R2Bucket;
      const env = envWith(database.d1, bucket);
      const id = (index + 1).toString(16).repeat(32);
      const objectKey = `attachments/permutation-${index}`;
      const multipart = await storage.bucket.createMultipartUpload(objectKey);
      const uploaded = await multipart.uploadPart(1, new Uint8Array([index]));
      database.exec(
        `INSERT INTO attachment_objects
           (id, object_key, size_bytes, expires_at, content_expires_at,
            created_at, fetch_token_sha256_hex, state, upload_id)
         VALUES (?, ?, 1, ?, ?, ?, ?, 'uploading', ?)`,
        id,
        objectKey,
        now + 10,
        now + 3600,
        now,
        DIGEST,
        multipart.uploadId,
      );
      database.exec(
        `INSERT INTO attachment_parts
           (attachment_id, part_number, size_bytes, etag)
         VALUES (?, 1, 1, ?)`,
        id,
        uploaded.etag,
      );
      const claim = await acquireAttachmentCompletionClaim(
        env,
        id,
        "c".repeat(32),
        now,
      );
      expect(claim).not.toBeNull();

      const reclaimAt =
        now + Math.max(11, ATTACHMENT_SWEEP_LEASE_SECONDS + 1);
      vi.setSystemTime(reclaimAt * 1000);
      let r2CompletionSucceeded = false;
      for (const action of order) {
        if (action === "complete") {
          try {
            await multipart.complete([
              { partNumber: 1, etag: uploaded.etag },
            ]);
            r2CompletionSucceeded = true;
          } catch {
            r2CompletionSucceeded = false;
          }
        } else if (action === "sweep") {
          await sweepExpiredAttachments(env);
        } else {
          await finalizeAttachmentReadyClaim(env, claim!, reclaimAt);
        }
      }

      const row = database.raw.prepare(
        `SELECT object_key, size_bytes, state
           FROM attachment_objects WHERE id = ?`,
      ).get(id) as {
        object_key: string;
        size_bytes: number;
        state: string;
      } | undefined;
      const object = await storage.bucket.head(objectKey);
      expect(row?.state === "ready" && object === null).toBe(false);
      if (row?.state === "ready") {
        expect(object?.size).toBe(row.size_bytes);
      } else {
        expect(row).toBeUndefined();
        expect(object).toBeNull();
      }
      if (r2CompletionSucceeded) {
        expect(row?.state).toBe("ready");
        expect(object?.size).toBe(1);
        expect(deletes).not.toContain(objectKey);
      }
    }
  });
});
