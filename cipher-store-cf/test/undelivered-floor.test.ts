import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import { handleUpload } from "../src/endpoints/blob.js";
import { MAX_LIVE_BLOB_ROWS } from "../src/lib/blob-limits.js";

const HEX_64 = "a".repeat(64);

function fullPoolEnv(statements: string[]): Env {
  return {
    DB: {
      prepare(sql: string) {
        statements.push(sql);
        return {
          bind() {
            return {
              first: async () => sql.includes("SELECT 1")
                ? null
                : { rows: MAX_LIVE_BLOB_ROWS, bytes: 1 },
            };
          },
        };
      },
    } as unknown as D1Database,
    get PAYLOADS(): never {
      throw new Error("a capacity refusal must not write or remove an R2 object");
    },
  } as Env;
}

describe("undelivered storage floor", () => {
  it("refuses a full pool without evicting an existing undelivered blob", async () => {
    const statements: string[] = [];
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      headers: {
        "x-osl-ttl-seconds": "604800",
        "x-osl-blob-id": "1".repeat(32),
        "x-osl-fetch-digest": HEX_64,
        "x-osl-ack-digest": "b".repeat(64),
        "x-osl-manage-digest": "c".repeat(64),
        "x-osl-delivery-tag": "d".repeat(32),
        "x-osl-object-class": "single-ack",
      },
      body: new Uint8Array([1]),
    });

    const response = await handleUpload(request, fullPoolEnv(statements));

    expect(response.status).toBe(503);
    expect(await response.json()).toMatchObject({ error: "storage_capacity" });
    // The simulated D1 row count remains MAX_LIVE_BLOB_ROWS: no mutation was
    // even prepared, so replacing the refusal with delete-oldest-and-insert
    // would make this assertion fail.
    expect(statements).not.toContainEqual(expect.stringMatching(/\bDELETE\b|\bINSERT\b/i));
    expect(statements).toHaveLength(2);
  });
});
