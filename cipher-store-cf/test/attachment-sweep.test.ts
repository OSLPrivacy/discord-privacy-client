import { readFile } from "node:fs/promises";
import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import {
  ATTACHMENT_SWEEP_BATCH_SIZE,
  MAX_LIVE_ATTACHMENT_ROWS,
} from "../src/lib/attachment-limits.js";
import { sweepExpiredAttachments } from "../src/lib/sweep.js";

describe("attachment quota and expiry sweep", () => {
  it("uses Wrangler-splittable DDL for digest-only metadata", async () => {
    const migration = await readFile(
      new URL("../migrations/0004_attachment_capability_digests_and_quota.sql", import.meta.url),
      "utf8",
    );
    expect(migration).toContain("fetch_token_sha256_hex");
    expect(migration).not.toMatch(/\bfetch_token\s+TEXT\b/);
    expect(migration).not.toMatch(/CREATE\s+TRIGGER/i);
    expect(migration).not.toMatch(/^\s*(BEGIN|END)\s*;/im);
    const statements = migration.split(";").map((value) => value.trim()).filter(Boolean);
    expect(statements).toHaveLength(5);
    for (const statement of statements) {
      expect(statement).toMatch(/^(?:(?:--[^\n]*\n)|\s)*(?:DROP TABLE|CREATE TABLE|CREATE INDEX)/);
    }
  });

  it("drains a full-cap expired backlog in bounded bulk batches", async () => {
    const rows = Array.from({ length: MAX_LIVE_ATTACHMENT_ROWS }, (_, index) => ({
      id: index.toString(16).padStart(32, "0"),
      object_key: `attachments/${index}`,
      upload_id: null,
    }));
    const deletedKeys: string[] = [];
    const prepare = vi.fn((sql: string) => ({
      bind: (...values: unknown[]) => ({
        all: async () => ({ results: rows.slice(0, ATTACHMENT_SWEEP_BATCH_SIZE) }),
        run: async () => {
          if (sql.includes("DELETE FROM attachment_objects")) {
            const ids = new Set(values.slice(1).map(String));
            for (let index = rows.length - 1; index >= 0; index--) {
              if (ids.has(rows[index]!.id)) rows.splice(index, 1);
            }
          }
          return { success: true };
        },
      }),
    }));
    const remove = vi.fn(async (keys: string | string[]) => {
      deletedKeys.push(...(Array.isArray(keys) ? keys : [keys]));
    });
    const env = {
      DB: { prepare },
      ATTACHMENTS: { delete: remove, head: vi.fn() },
    } as unknown as Env;

    await expect(sweepExpiredAttachments(env)).resolves.toBe(MAX_LIVE_ATTACHMENT_ROWS);
    expect(rows).toHaveLength(0);
    expect(deletedKeys).toHaveLength(MAX_LIVE_ATTACHMENT_ROWS);
    expect(remove).toHaveBeenCalledTimes(Math.ceil(MAX_LIVE_ATTACHMENT_ROWS / ATTACHMENT_SWEEP_BATCH_SIZE));
  });

  it("keeps retryable metadata if an R2 batch deletion fails", async () => {
    const rows = [{ id: "0".repeat(32), object_key: "attachments/0", upload_id: null }];
    const run = vi.fn();
    const env = {
      DB: {
        prepare: vi.fn(() => ({
          bind: () => ({ all: async () => ({ results: rows }), run }),
        })),
      },
      ATTACHMENTS: {
        delete: vi.fn(async () => { throw new Error("r2 unavailable"); }),
        head: vi.fn(),
      },
    } as unknown as Env;

    await expect(sweepExpiredAttachments(env)).rejects.toThrow("r2 unavailable");
    expect(run).not.toHaveBeenCalled();
  });

  it("aborts an expired incomplete multipart upload before deleting its metadata", async () => {
    const rows = [{
      id: "1".repeat(32),
      object_key: "attachments/incomplete",
      upload_id: "opaque-upload-id",
    }];
    const abort = vi.fn(async () => undefined);
    const metadataDelete = vi.fn(async () => {
      rows.splice(0, rows.length);
      return { success: true };
    });
    const env = {
      DB: {
        prepare: vi.fn((sql: string) => ({
          bind: () => ({
            all: async () => ({ results: [...rows] }),
            run: sql.includes("DELETE FROM attachment_objects")
              ? metadataDelete
              : vi.fn(),
          }),
        })),
      },
      ATTACHMENTS: {
        head: vi.fn(async () => null),
        resumeMultipartUpload: vi.fn(() => ({ abort })),
        delete: vi.fn(async () => undefined),
      },
    } as unknown as Env;

    await expect(sweepExpiredAttachments(env)).resolves.toBe(1);
    expect(abort).toHaveBeenCalledOnce();
    expect(metadataDelete).toHaveBeenCalledOnce();
  });
});
