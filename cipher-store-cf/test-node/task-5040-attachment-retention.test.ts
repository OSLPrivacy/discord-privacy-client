import { DatabaseSync } from "node:sqlite";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import {
  handleAttachmentFetch,
  handleAttachmentSessionCreate,
  handleAttachmentUpload,
} from "../src/endpoints/attachment.js";
import { ATTACHMENT_RETENTION_SECONDS } from "../src/lib/attachment-retention.js";
import { memoryR2 } from "../test/helpers/d1.js";

const START_SECONDS = 2_000_000_000;
const DAY_SECONDS = 24 * 60 * 60;
const FETCH_TOKEN = "5040".repeat(8);
const MARKED_BYTES = new TextEncoder().encode("TASK5040-PRO-MARKED-ATTACHMENT");

type Bindable = string | number | bigint | null | Uint8Array;

function bindable(value: unknown): Bindable {
  if (value === null || typeof value === "string" || typeof value === "number" || typeof value === "bigint") {
    return value;
  }
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (typeof value === "boolean") return value ? 1 : 0;
  throw new Error(`unsupported D1 value: ${String(value)}`);
}

function attachmentD1(): { d1: D1Database; count(): number } {
  const database = new DatabaseSync(":memory:");
  database.exec(`
    CREATE TABLE attachment_objects (
      id TEXT PRIMARY KEY,
      object_key TEXT NOT NULL,
      size_bytes INTEGER NOT NULL,
      expires_at INTEGER NOT NULL,
      content_expires_at INTEGER,
      created_at INTEGER NOT NULL,
      fetch_token_sha256_hex TEXT NOT NULL,
      state TEXT NOT NULL,
      upload_id TEXT,
      single_fetch INTEGER NOT NULL DEFAULT 0,
      reserved_until INTEGER
    )
  `);

  const bound = (sql: string, values: unknown[]) => {
    const parameters = values.map(bindable);
    return {
      async run() {
        const result = database.prepare(sql).run(...parameters);
        return { success: true, meta: { changes: Number(result.changes) } };
      },
      async first<T>(): Promise<T | null> {
        return (database.prepare(sql).get(...parameters) ?? null) as T | null;
      },
      async all<T>() {
        return { success: true, results: database.prepare(sql).all(...parameters) as T[], meta: {} };
      },
    };
  };

  return {
    d1: {
      prepare(sql: string) {
        return {
          bind: (...values: unknown[]) => bound(sql, values),
          ...bound(sql, []),
        };
      },
    } as unknown as D1Database,
    count() {
      const row = database.prepare("SELECT COUNT(*) AS count FROM attachment_objects").get() as { count: number };
      return row.count;
    },
  };
}

function uploadRequest(tier: "free" | "pro", ttlSeconds: number): Request {
  return new Request("https://cipher.test/v1/attachment", {
    method: "POST",
    headers: {
      "content-length": String(MARKED_BYTES.byteLength),
      "x-osl-account-tier": tier,
      "x-osl-fetch-token": FETCH_TOKEN,
      "x-osl-single-fetch": "0",
      "x-osl-ttl-seconds": String(ttlSeconds),
    },
    body: MARKED_BYTES,
  });
}

function sessionRequest(tier: "free" | "pro", ttlSeconds: number): Request {
  return new Request("https://cipher.test/v1/attachment/session", {
    method: "POST",
    headers: {
      "x-osl-account-tier": tier,
      "x-osl-fetch-token": FETCH_TOKEN,
      "x-osl-single-fetch": "0",
      "x-osl-size-bytes": String(MARKED_BYTES.byteLength),
      "x-osl-ttl-seconds": String(ttlSeconds),
    },
  });
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Task 5040 attachment retention", () => {
  it("keeps a Pro attachment readable on day 29 and refuses the same 30-day request on Free", async () => {
    vi.spyOn(Date, "now").mockReturnValue(START_SECONDS * 1000);
    const database = attachmentD1();
    const storage = memoryR2();
    const env = { DB: database.d1, ATTACHMENTS: storage.bucket } as Env;

    const uploaded = await handleAttachmentUpload(
      uploadRequest("pro", ATTACHMENT_RETENTION_SECONDS.pro),
      env,
    );
    expect(uploaded.status).toBe(201);
    const receipt = await uploaded.json() as { id: string; expires_at: number; size_bytes: number };
    expect(receipt.expires_at).toBe(START_SECONDS + 30 * DAY_SECONDS);
    expect(receipt.size_bytes).toBe(MARKED_BYTES.byteLength);

    vi.spyOn(Date, "now").mockReturnValue((START_SECONDS + 29 * DAY_SECONDS) * 1000);
    const fetched = await handleAttachmentFetch(new Request(
      `https://cipher.test/v1/attachment/${receipt.id}`,
      { headers: { "x-osl-fetch-token": FETCH_TOKEN } },
    ), env, receipt.id);
    expect(fetched.status).toBe(200);
    const readback = new Uint8Array(await fetched.arrayBuffer());
    expect(readback).toEqual(MARKED_BYTES);

    const refused = await handleAttachmentUpload(
      uploadRequest("free", ATTACHMENT_RETENTION_SECONDS.pro),
      env,
    );
    expect(refused.status).toBe(400);
    expect(await refused.json()).toEqual({
      error: "attachment_ttl_limit",
      message: "Free attachments have a 7-day limit (604800 seconds)",
    });
    expect(database.count()).toBe(1);
    expect(storage.objects.size).toBe(1);

    console.log(
      `TASK5040_PRO_UPLOAD status=${uploaded.status} lifetime_seconds=${receipt.expires_at - START_SECONDS}`,
    );
    console.log(
      `TASK5040_DAY29_READ status=${fetched.status} day=29 bytes=${readback.byteLength} exact=${String(new TextDecoder().decode(readback) === new TextDecoder().decode(MARKED_BYTES))}`,
    );
    console.log(
      "TASK5040_FREE_REFUSAL status=400 error=attachment_ttl_limit message=Free attachments have a 7-day limit (604800 seconds)",
    );
  });

  it("names each tier's ceiling for requests beyond it", async () => {
    const env = { DB: {} as D1Database, ATTACHMENTS: {} as R2Bucket } as Env;

    const free = await handleAttachmentUpload(uploadRequest("free", 8 * DAY_SECONDS), env);
    expect(await free.json()).toMatchObject({
      error: "attachment_ttl_limit",
      message: "Free attachments have a 7-day limit (604800 seconds)",
    });

    const pro = await handleAttachmentUpload(uploadRequest("pro", 31 * DAY_SECONDS), env);
    expect(await pro.json()).toMatchObject({
      error: "attachment_ttl_limit",
      message: "Pro attachments have a 30-day limit (2592000 seconds)",
    });

    console.log(
      "TASK5040B_FREE_8D_REFUSAL status=400 error=attachment_ttl_limit message=Free attachments have a 7-day limit (604800 seconds)",
    );
    console.log(
      "TASK5040B_PRO_31D_REFUSAL status=400 error=attachment_ttl_limit message=Pro attachments have a 30-day limit (2592000 seconds)",
    );
  });

  it("applies the same 30-day Pro and 7-day Free policy to multipart sessions", async () => {
    vi.spyOn(Date, "now").mockReturnValue(START_SECONDS * 1000);
    const database = attachmentD1();
    const storage = memoryR2();
    const env = { DB: database.d1, ATTACHMENTS: storage.bucket } as Env;

    const pro = await handleAttachmentSessionCreate(
      sessionRequest("pro", ATTACHMENT_RETENTION_SECONDS.pro),
      env,
    );
    expect(pro.status).toBe(201);
    await expect(pro.json()).resolves.toMatchObject({
      expires_at: START_SECONDS + ATTACHMENT_RETENTION_SECONDS.pro,
    });

    const free = await handleAttachmentSessionCreate(
      sessionRequest("free", ATTACHMENT_RETENTION_SECONDS.pro),
      env,
    );
    expect(free.status).toBe(400);
    await expect(free.json()).resolves.toEqual({
      error: "attachment_ttl_limit",
      message: "Free attachments have a 7-day limit (604800 seconds)",
    });
    expect(database.count()).toBe(1);
    expect(storage.liveUploads()).toBe(1);
  });
});
