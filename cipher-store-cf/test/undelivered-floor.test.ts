/// RESTATED 2026-08-05 for D-256. This spec was written on 2026-08-02, the day
/// after `cc619a55e` replaced the atomic capacity gate with a SELECT-then-
/// INSERT, and it recorded that shape: "no mutation was even prepared", exactly
/// two statements. Under the atomic gate the audit's HIGH-2 finding requires,
/// the conditional INSERT *is* prepared -- its COUNT/SUM predicates are how the
/// refusal happens, and they cannot race another insert the way a preceding
/// read can. The pre-regression code prepared it too.
///
/// So the assertions below are stated as the property rather than as the
/// statement count of the design that regressed, and they are tightened while
/// they are being restated: no DELETE at all (the eviction claim, unchanged),
/// exactly one mutation and it must carry every guard predicate, and the
/// aggregate re-read that chooses the refusal message may not mention the blob
/// id (D-255 -- what the caller is told must not depend on which id it named).

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
        const first = async () => sql.includes("INSERT")
          ? null
          : { rows: MAX_LIVE_BLOB_ROWS, bytes: 1 };
        // A guarded INSERT against a full pool writes no row, and real D1
        // reports that as `meta.changes = 0` rather than by throwing -- see
        // `test/d1-meta-changes-contract.test.ts` case B, measured in the
        // Workers pool. The double answers the way D1 does.
        const run = async () => ({ success: true, meta: { changes: 0 } });
        return { first, run, bind: () => ({ first, run }) };
      },
    } as unknown as D1Database,
    get PAYLOADS(): never {
      throw new Error("a capacity refusal must not write or remove an R2 object");
    },
    // ATTACHMENTS, RATE_LIMIT and RATE_LIMIT_HASH_KEY were missing outright and
    // the trailing `as Env` hid it. Given as throwing getters they extend the
    // claim this fixture already makes about PAYLOADS -- a capacity refusal
    // touches no storage at all -- rather than merely satisfying the compiler.
    get ATTACHMENTS(): never {
      throw new Error("a capacity refusal must not touch R2 attachments");
    },
    get RATE_LIMIT(): never {
      throw new Error("a capacity refusal must not touch the rate-limit KV namespace");
    },
    get RATE_LIMIT_HASH_KEY(): never {
      throw new Error("a capacity refusal must not read the rate-limit hash key");
    },
  };
}

describe("undelivered storage floor", () => {
  it("refuses a full pool without evicting an existing undelivered blob", async () => {
    const statements: string[] = [];
    const request = new Request("https://cipher.test/v1/blob", {
      method: "PUT",
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

    // The simulated D1 row count remains MAX_LIVE_BLOB_ROWS. Nothing may be
    // removed to make room: delete-oldest-and-insert still fails here.
    expect(statements).not.toContainEqual(expect.stringMatching(/\bDELETE\b/i));

    // Exactly one mutation, and it must be unable to write on its own. An
    // unconditional `INSERT ... VALUES`, or one that dropped any guard,
    // fails every clause below.
    const mutations = statements.filter((sql) => /\bINSERT\b/i.test(sql));
    expect(mutations).toHaveLength(1);
    expect(mutations[0]).toMatch(/WHERE\s+NOT\s+EXISTS/i);
    expect(mutations[0]).toMatch(/SELECT\s+COUNT\(\*\)\s+FROM\s+blob_capability_index/i);
    expect(mutations[0]).toMatch(/SUM\(size_bytes\)/i);

    // Two statements total: the guarded write, then the aggregate re-read that
    // decides the message. That re-read is not allowed to name the blob id --
    // if it did, a caller could tell a taken id from an unused one by which
    // refusal it got back.
    expect(statements).toHaveLength(2);
    const reread = statements.find((sql) => !/\bINSERT\b/i.test(sql))!;
    expect(reread).not.toMatch(/blob_id/i);
  });
});
