/// A real-SQLite D1 shim for tests that must prove *database-level* admission.
///
/// The hand-rolled `prepare` fakes elsewhere in this suite model only the
/// statements they were written for, so they cannot demonstrate a quota being
/// exhausted — they would only re-assert the assertion. This shim runs the
/// actual `migrations/*.sql` against `node:sqlite` and executes the Worker's
/// real SQL, so CHECK constraints and the conditional INSERT predicates behave
/// exactly as they do in production.
///
/// Concurrency fidelity matters for the rate-limiter tests: every call yields
/// to the microtask queue *before* the statement runs and never inside it. That
/// is D1's contract — statements interleave with each other, but a single
/// statement is atomic.

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";

const MIGRATIONS_DIR = fileURLToPath(new URL("../../migrations", import.meta.url));

type Bindable = string | number | bigint | null | Uint8Array;

function toBindable(value: unknown): Bindable {
  if (value === null || value === undefined) return null;
  if (typeof value === "string" || typeof value === "number" || typeof value === "bigint") {
    return value;
  }
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (typeof value === "boolean") return value ? 1 : 0;
  throw new Error(`unsupported D1 bind value: ${Object.prototype.toString.call(value)}`);
}

export interface TestD1 {
  d1: D1Database;
  raw: DatabaseSync;
  /** Direct helper for arranging state the Worker cannot reach cheaply. */
  exec(sql: string, ...values: unknown[]): void;
  count(sql: string, ...values: unknown[]): number;
}

export function migratedD1(): TestD1 {
  const raw = new DatabaseSync(":memory:");
  const files = readdirSync(MIGRATIONS_DIR)
    .filter((name) => name.endsWith(".sql"))
    .sort();
  for (const name of files) {
    raw.exec(readFileSync(join(MIGRATIONS_DIR, name), "utf8"));
  }

  const bound = (sql: string, values: unknown[]) => {
    const params = values.map(toBindable);
    return {
      async run() {
        await Promise.resolve();
        const result = raw.prepare(sql).run(...params);
        return { success: true, meta: { changes: Number(result.changes) } };
      },
      async first<T>(): Promise<T | null> {
        await Promise.resolve();
        return (raw.prepare(sql).get(...params) ?? null) as T | null;
      },
      async all<T>() {
        await Promise.resolve();
        return { success: true, results: raw.prepare(sql).all(...params) as T[], meta: {} };
      },
    };
  };

  const prepare = (sql: string) => ({
    bind: (...values: unknown[]) => bound(sql, values),
    ...bound(sql, []),
  });

  const d1 = {
    prepare,
    async batch(statements: Array<{ run(): Promise<unknown> }>) {
      const out = [];
      for (const statement of statements) out.push(await statement.run());
      return out;
    },
  } as unknown as D1Database;

  return {
    d1,
    raw,
    exec(sql: string, ...values: unknown[]) {
      raw.prepare(sql).run(...values.map(toBindable));
    },
    count(sql: string, ...values: unknown[]) {
      const row = raw.prepare(sql).get(...values.map(toBindable)) as Record<string, unknown>;
      return Number(Object.values(row ?? { c: 0 })[0] ?? 0);
    },
  };
}

/// workerd rejects a streamed R2 body that has no known length, and the output
/// of `pipeThrough` has none. A double that quietly accepts one is how an
/// attachment upload path shipped broken for its entire life while every test
/// stayed green — so this double refuses exactly what production refuses, with
/// the same message. If a test fails here, that is the point.
function knownLengthBody(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  throw new TypeError(
    "Provided readable stream must have a known length "
    + "(request/response body or readable half of FixedLengthStream)",
  );
}

/// Minimal in-memory R2 with multipart support, sufficient for the attachment
/// endpoints. Tracks aborted uploads so a test can prove reclamation happened.
///
/// Deliberately strict where production is strict: bodies must carry a known
/// length, and `onlyIf: { etagDoesNotMatch: "*" }` behaves as a real
/// precondition rather than being ignored.
export function memoryR2() {
  const objects = new Map<string, Uint8Array>();
  const uploads = new Map<string, { key: string; aborted: boolean; parts: Map<number, Uint8Array> }>();
  let uploadSeq = 0;

  const resume = (key: string, uploadId: string) => ({
    uploadId,
    key,
    async uploadPart(partNumber: number, body: unknown) {
      const bytes = knownLengthBody(body);
      uploads.get(uploadId)?.parts.set(partNumber, bytes);
      return { partNumber, etag: `etag-${uploadId}-${partNumber}` };
    },
    async complete(parts: Array<{ partNumber: number; etag: string }>) {
      const upload = uploads.get(uploadId)!;
      const ordered = parts.map((part) => upload.parts.get(part.partNumber) ?? new Uint8Array());
      const total = ordered.reduce((sum, chunk) => sum + chunk.byteLength, 0);
      const joined = new Uint8Array(total);
      let offset = 0;
      for (const chunk of ordered) {
        joined.set(chunk, offset);
        offset += chunk.byteLength;
      }
      objects.set(key, joined);
      uploads.delete(uploadId);
      return { key, size: joined.byteLength } as R2Object;
    },
    async abort() {
      const upload = uploads.get(uploadId);
      if (upload) upload.aborted = true;
      uploads.delete(uploadId);
    },
  });

  const bucket = {
    async createMultipartUpload(key: string) {
      const uploadId = `upload-${++uploadSeq}`;
      uploads.set(uploadId, { key, aborted: false, parts: new Map() });
      return resume(key, uploadId);
    },
    resumeMultipartUpload: (key: string, uploadId: string) => resume(key, uploadId),
    async put(
      key: string,
      value: unknown,
      options?: { onlyIf?: { etagDoesNotMatch?: string } },
    ) {
      const bytes = knownLengthBody(value);
      // A real conditional put returns null when the precondition fails; the
      // caller treats that as an id collision rather than a success.
      if (options?.onlyIf?.etagDoesNotMatch === "*" && objects.has(key)) return null;
      objects.set(key, bytes);
      return { key, size: bytes.byteLength } as R2Object;
    },
    async get(key: string) {
      const bytes = objects.get(key);
      if (!bytes) return null;
      return { key, size: bytes.byteLength, body: new Response(bytes).body! } as R2ObjectBody;
    },
    async head(key: string) {
      const bytes = objects.get(key);
      return bytes ? ({ key, size: bytes.byteLength } as R2Object) : null;
    },
    async delete(key: string | string[]) {
      for (const item of Array.isArray(key) ? key : [key]) objects.delete(item);
    },
  } as unknown as R2Bucket;

  return { bucket, objects, uploads, liveUploads: () => uploads.size };
}
