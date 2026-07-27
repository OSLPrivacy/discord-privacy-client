import { afterEach, describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import { sweepExpiredAttachments } from "../src/lib/sweep.js";
import { memoryR2, migratedD1, type TestD1 } from "../test/helpers/d1.js";

const NOW = 1_800_000_000;
const DIGEST = "d".repeat(64);

function envWith(db: D1Database, bucket: R2Bucket): Env {
  return { DB: db, ATTACHMENTS: bucket } as Env;
}

function openBoundedAdoption(database: TestD1): void {
  database.exec(
    `UPDATE attachment_predecessor_adoption
        SET migration_started_at = ?,
            eligible_created_through = ?,
            max_claims_per_cycle = 100
      WHERE singleton = 1`,
    NOW - 100,
    NOW + 100,
  );
}

async function predecessorCompleting(
  database: TestD1,
  storage: ReturnType<typeof memoryR2>,
  id: string,
  objectKey: string,
  bytes: Uint8Array,
): Promise<{
  multipart: R2MultipartUpload;
  part: R2UploadedPart;
}> {
  const multipart = await storage.bucket.createMultipartUpload(objectKey);
  const part = await multipart.uploadPart(1, bytes);
  database.exec(
    `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at,
        created_at, fetch_token_sha256_hex, state, upload_id)
     VALUES (?, ?, ?, ?, ?, ?, ?, 'completing', ?)`,
    id,
    objectKey,
    bytes.byteLength,
    NOW - 1,
    NOW + 3600,
    NOW,
    DIGEST,
    multipart.uploadId,
  );
  database.exec(
    `INSERT INTO attachment_parts
       (attachment_id, part_number, size_bytes, etag)
     VALUES (?, 1, ?, ?)`,
    id,
    bytes.byteLength,
    part.etag,
  );
  return { multipart, part };
}

function d1WithOneMetadataDeleteFailure(real: D1Database): D1Database {
  let shouldFail = true;
  return new Proxy(real, {
    get(target, property, receiver) {
      if (property !== "prepare") {
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      }
      return (sql: string) => {
        const prepared = target.prepare(sql);
        if (
          !shouldFail
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
              return typeof value === "function"
                ? value.bind(statement)
                : value;
            }
            return (...values: unknown[]) => {
              const bound = statement.bind(...values);
              return new Proxy(bound, {
                get(boundStatement, boundProperty, boundReceiver) {
                  if (boundProperty === "first") {
                    return async () => {
                      shouldFail = false;
                      throw new Error("simulated crash before metadata CAS");
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

function d1WithOneAbsenceFenceFailure(real: D1Database): D1Database {
  let shouldFail = true;
  return new Proxy(real, {
    get(target, property, receiver) {
      if (property !== "prepare") {
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      }
      return (sql: string) => {
        const prepared = target.prepare(sql);
        if (!shouldFail || !/SET storage_fence_state/.test(sql)) return prepared;
        return new Proxy(prepared, {
          get(statement, statementProperty, statementReceiver) {
            if (statementProperty !== "bind") {
              const value = Reflect.get(statement, statementProperty, statementReceiver);
              return typeof value === "function" ? value.bind(statement) : value;
            }
            return (...values: unknown[]) => {
              const bound = statement.bind(...values);
              return new Proxy(bound, {
                get(boundStatement, boundProperty, boundReceiver) {
                  if (boundProperty === "first") {
                    return async () => {
                      shouldFail = false;
                      throw new Error("simulated crash before absence CAS");
                    };
                  }
                  const value = Reflect.get(boundStatement, boundProperty, boundReceiver);
                  return typeof value === "function" ? value.bind(boundStatement) : value;
                },
              });
            };
          },
        });
      };
    },
  }) as D1Database;
}

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((accept) => {
    resolve = accept;
  });
  return { promise, resolve };
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("bounded predecessor completing-row adoption", () => {
  it("refuses a missing adoption marker before any R2 observation or quota change", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW * 1000);
    const database = migratedD1();
    const storage = memoryR2();
    openBoundedAdoption(database);
    const id = "0".repeat(32);
    await predecessorCompleting(
      database,
      storage,
      id,
      "attachments/missing-adoption-marker",
      new Uint8Array([0]),
    );
    database.exec(
      "DELETE FROM attachment_predecessor_adoption WHERE singleton = 1",
    );
    const head = vi.fn((key: string) => storage.bucket.head(key));
    const bucket = new Proxy(storage.bucket, {
      get(target, property, receiver) {
        if (property === "head") return head;
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      },
    }) as R2Bucket;

    await expect(
      sweepExpiredAttachments(envWith(database.d1, bucket)),
    ).rejects.toThrow(/adoption marker is invalid/);
    expect(head).not.toHaveBeenCalled();
    expect(database.count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(1);
    expect(database.count("SELECT COUNT(*) AS c FROM attachment_sweep_claims")).toBe(0);
    expect(storage.liveUploads()).toBe(1);
  });

  it("promotes a correctly sized completed object without releasing quota", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW * 1000);
    const database = migratedD1();
    const storage = memoryR2();
    openBoundedAdoption(database);
    const id = "1".repeat(32);
    const objectKey = "attachments/predecessor-complete";
    const bytes = new Uint8Array([1, 2, 3]);
    const { multipart, part } = await predecessorCompleting(
      database,
      storage,
      id,
      objectKey,
      bytes,
    );
    await multipart.complete([{ partNumber: 1, etag: part.etag }]);

    await expect(
      sweepExpiredAttachments(envWith(database.d1, storage.bucket)),
    ).resolves.toEqual({ claimed: 1, completed: 1, failed: 0 });

    expect(
      database.raw.prepare(
        `SELECT state, upload_id, size_bytes
           FROM attachment_objects WHERE id = ?`,
      ).get(id),
    ).toEqual({
      state: "ready",
      upload_id: null,
      size_bytes: bytes.byteLength,
    });
    expect(await storage.bucket.head(objectKey)).toMatchObject({
      size: bytes.byteLength,
    });
    expect(database.count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(1);
    expect(
      database.count("SELECT COALESCE(SUM(size_bytes), 0) AS bytes FROM attachment_objects"),
    ).toBe(bytes.byteLength);
    expect(database.count("SELECT COUNT(*) AS c FROM attachment_sweep_claims")).toBe(0);
  });

  it("retains quota across a crash after abort, then retries without a second abort", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW * 1000);
    const database = migratedD1();
    const storage = memoryR2();
    openBoundedAdoption(database);
    const id = "2".repeat(32);
    const objectKey = "attachments/predecessor-incomplete";
    await predecessorCompleting(
      database,
      storage,
      id,
      objectKey,
      new Uint8Array([4, 5]),
    );
    const abort = vi.fn(async () => {
      await storage.bucket
        .resumeMultipartUpload(objectKey, "upload-1")
        .abort();
    });
    const bucket = new Proxy(storage.bucket, {
      get(target, property, receiver) {
        if (property === "resumeMultipartUpload") {
          return () => ({ abort });
        }
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      },
    }) as R2Bucket;
    const crashing = envWith(
      d1WithOneMetadataDeleteFailure(database.d1),
      bucket,
    );

    await expect(sweepExpiredAttachments(crashing)).resolves.toEqual({
      claimed: 1,
      completed: 0,
      failed: 1,
    });
    expect(abort).toHaveBeenCalledTimes(1);
    expect(database.count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(1);
    expect(
      database.raw.prepare(
        `SELECT worker_id, claim_token, lease_version, storage_fence_state
           FROM attachment_sweep_claims WHERE attachment_id = ?`,
      ).get(id),
    ).toMatchObject({
      worker_id: null,
      claim_token: null,
      lease_version: 2,
      storage_fence_state: "object_absent_confirmed",
    });

    database.exec(
      `UPDATE attachment_sweep_claims
          SET retry_not_before = 0
        WHERE attachment_id = ?`,
      id,
    );
    await expect(
      sweepExpiredAttachments(envWith(database.d1, bucket)),
    ).resolves.toEqual({ claimed: 1, completed: 1, failed: 0 });
    expect(abort).toHaveBeenCalledTimes(1);
    expect(database.count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
    expect(database.count("SELECT COUNT(*) AS c FROM attachment_parts")).toBe(0);
    expect(await storage.bucket.head(objectKey)).toBeNull();
    expect(storage.liveUploads()).toBe(0);
  });

  it("treats only terminal consumed-upload retry as fenced after absence-CAS crash", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW * 1000);
    const database = migratedD1();
    const storage = memoryR2();
    openBoundedAdoption(database);
    const id = "4".repeat(32);
    const objectKey = "attachments/predecessor-consumed-retry";
    await predecessorCompleting(database, storage, id, objectKey, new Uint8Array([7, 8]));
    let abortCalls = 0;
    const abort = vi.fn(async () => {
      abortCalls += 1;
      if (abortCalls === 1) {
        await storage.bucket.resumeMultipartUpload(objectKey, "upload-1").abort();
        return;
      }
      const error = Object.assign(new Error("upload already consumed"), {
        code: "NoSuchUpload",
      });
      throw error;
    });
    const bucket = new Proxy(storage.bucket, {
      get(target, property, receiver) {
        if (property === "resumeMultipartUpload") return () => ({ abort });
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      },
    }) as R2Bucket;

    await expect(
      sweepExpiredAttachments(envWith(d1WithOneAbsenceFenceFailure(database.d1), bucket)),
    ).resolves.toEqual({ claimed: 1, completed: 0, failed: 1 });
    expect(database.count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(1);

    database.exec(
      "UPDATE attachment_sweep_claims SET retry_not_before = 0 WHERE attachment_id = ?",
      id,
    );
    await expect(sweepExpiredAttachments(envWith(database.d1, bucket))).resolves.toEqual({
      claimed: 1,
      completed: 1,
      failed: 0,
    });
    expect(abort).toHaveBeenCalledTimes(2);
    expect(storage.liveUploads()).toBe(0);
    expect(database.count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
  });

  it("promotes when predecessor completion wins between HEAD and abort", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW * 1000);
    const database = migratedD1();
    const storage = memoryR2();
    openBoundedAdoption(database);
    const id = "3".repeat(32);
    const objectKey = "attachments/predecessor-completion-wins";
    const bytes = new Uint8Array([9]);
    const { multipart, part } = await predecessorCompleting(
      database,
      storage,
      id,
      objectKey,
      bytes,
    );
    const firstHead = deferred();
    const releaseHead = deferred();
    const deletes: string[] = [];
    let headCount = 0;
    const bucket = new Proxy(storage.bucket, {
      get(target, property, receiver) {
        if (property === "head") {
          return async (key: string) => {
            headCount += 1;
            if (headCount === 1) {
              firstHead.resolve();
              await releaseHead.promise;
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

    const sweep = sweepExpiredAttachments(envWith(database.d1, bucket));
    await firstHead.promise;
    await multipart.complete([{ partNumber: 1, etag: part.etag }]);
    releaseHead.resolve();
    await expect(sweep).resolves.toEqual({
      claimed: 1,
      completed: 1,
      failed: 0,
    });

    expect(
      database.raw.prepare(
        "SELECT state, size_bytes FROM attachment_objects WHERE id = ?",
      ).get(id),
    ).toEqual({ state: "ready", size_bytes: bytes.byteLength });
    expect(await storage.bucket.head(objectKey)).toMatchObject({
      size: bytes.byteLength,
    });
    expect(deletes).toEqual([]);
    expect(headCount).toBeGreaterThanOrEqual(2);
  });
});
