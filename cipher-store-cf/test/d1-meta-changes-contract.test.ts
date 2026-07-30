/// D1 meta.changes contract measured against real D1 under
/// @cloudflare/vitest-pool-workers. These values are observations from this
/// pool, not branch guarantees taken from Cloudflare documentation.
///
/// A. INSERT ... SELECT ... WHERE <true predicate>
///    run().meta.changes = 1; with RETURNING, first() returns a row.
/// B. INSERT ... SELECT ... WHERE <false predicate>
///    run().meta.changes = 0; with RETURNING, first() returns null.
/// C. INSERT ... ON CONFLICT DO UPDATE ... WHERE <true>
///    run().meta.changes = 1; with RETURNING, first() returns a row.
/// D. INSERT ... ON CONFLICT DO UPDATE ... WHERE <false>
///    run().meta.changes = 0; with RETURNING, first() returns null.

import { env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";

interface ContractRow {
  id: string;
  used: number;
  label: string;
}

async function row(id: string): Promise<ContractRow | null> {
  return await env.DB.prepare(
    "SELECT id, used, label FROM d1_meta_changes_contract WHERE id = ?",
  ).bind(id).first<ContractRow>();
}

async function rowCount(): Promise<number> {
  const result = await env.DB.prepare(
    "SELECT COUNT(*) AS count FROM d1_meta_changes_contract",
  ).first<{ count: number }>();
  return result?.count ?? 0;
}

describe("D1 meta.changes contract", () => {
  beforeEach(async () => {
    await env.DB.prepare("DROP TABLE IF EXISTS d1_meta_changes_contract").run();
    await env.DB.prepare(
      `CREATE TABLE d1_meta_changes_contract (
        id TEXT PRIMARY KEY,
        used INTEGER NOT NULL,
        label TEXT NOT NULL
      )`,
    ).run();
  });

  it("A: reports one change when INSERT SELECT admits a row", async () => {
    const inserted = await env.DB.prepare(
      `INSERT INTO d1_meta_changes_contract (id, used, label)
       SELECT 'a-run', 1, 'inserted'
        WHERE 1 = 1`,
    ).run();

    expect(inserted.meta.changes).toBe(1);
    expect(await row("a-run")).toEqual({ id: "a-run", used: 1, label: "inserted" });

    const returned = await env.DB.prepare(
      `INSERT INTO d1_meta_changes_contract (id, used, label)
       SELECT 'a-returning', 2, 'returned'
        WHERE 1 = 1
       RETURNING id, used, label`,
    ).first<ContractRow>();

    expect(returned).toEqual({ id: "a-returning", used: 2, label: "returned" });
    expect(await row("a-returning")).toEqual(returned);
    expect(await rowCount()).toBe(2);
  });

  it("B: reports zero changes when INSERT SELECT predicate rejects a row", async () => {
    const inserted = await env.DB.prepare(
      `INSERT INTO d1_meta_changes_contract (id, used, label)
       SELECT 'b-run', 1, 'inserted'
        WHERE 1 = 0`,
    ).run();

    expect(inserted.meta.changes).toBe(0);
    expect(await row("b-run")).toBeNull();

    const returned = await env.DB.prepare(
      `INSERT INTO d1_meta_changes_contract (id, used, label)
       SELECT 'b-returning', 2, 'returned'
        WHERE 1 = 0
       RETURNING id, used, label`,
    ).first<ContractRow>();

    expect(returned).toBeNull();
    expect(await row("b-returning")).toBeNull();
    expect(await rowCount()).toBe(0);
  });

  it("C: reports one change when conflict update predicate applies", async () => {
    await env.DB.prepare(
      "INSERT INTO d1_meta_changes_contract (id, used, label) VALUES ('c-run', 1, 'seed')",
    ).run();
    await env.DB.prepare(
      "INSERT INTO d1_meta_changes_contract (id, used, label) VALUES ('c-returning', 10, 'seed')",
    ).run();

    const updated = await env.DB.prepare(
      `INSERT INTO d1_meta_changes_contract (id, used, label)
       VALUES ('c-run', 99, 'excluded')
       ON CONFLICT(id) DO UPDATE SET
         used = d1_meta_changes_contract.used + 1,
         label = 'updated'
        WHERE d1_meta_changes_contract.used < 2`,
    ).run();

    expect(updated.meta.changes).toBe(1);
    expect(await row("c-run")).toEqual({ id: "c-run", used: 2, label: "updated" });

    const returned = await env.DB.prepare(
      `INSERT INTO d1_meta_changes_contract (id, used, label)
       VALUES ('c-returning', 99, 'excluded')
       ON CONFLICT(id) DO UPDATE SET
         used = d1_meta_changes_contract.used + 1,
         label = 'returned'
        WHERE d1_meta_changes_contract.used < 11
       RETURNING id, used, label`,
    ).first<ContractRow>();

    expect(returned).toEqual({ id: "c-returning", used: 11, label: "returned" });
    expect(await row("c-returning")).toEqual(returned);
    expect(await rowCount()).toBe(2);
  });

  it("D: reports zero changes when conflict update predicate skips", async () => {
    await env.DB.prepare(
      "INSERT INTO d1_meta_changes_contract (id, used, label) VALUES ('d-run', 2, 'seed')",
    ).run();
    await env.DB.prepare(
      "INSERT INTO d1_meta_changes_contract (id, used, label) VALUES ('d-returning', 11, 'seed')",
    ).run();

    const updated = await env.DB.prepare(
      `INSERT INTO d1_meta_changes_contract (id, used, label)
       VALUES ('d-run', 99, 'excluded')
       ON CONFLICT(id) DO UPDATE SET
         used = d1_meta_changes_contract.used + 1,
         label = 'updated'
        WHERE d1_meta_changes_contract.used < 2`,
    ).run();

    expect(updated.meta.changes).toBe(0);
    expect(await row("d-run")).toEqual({ id: "d-run", used: 2, label: "seed" });

    const returned = await env.DB.prepare(
      `INSERT INTO d1_meta_changes_contract (id, used, label)
       VALUES ('d-returning', 99, 'excluded')
       ON CONFLICT(id) DO UPDATE SET
         used = d1_meta_changes_contract.used + 1,
         label = 'returned'
        WHERE d1_meta_changes_contract.used < 11
       RETURNING id, used, label`,
    ).first<ContractRow>();

    expect(returned).toBeNull();
    expect(await row("d-returning")).toEqual({ id: "d-returning", used: 11, label: "seed" });
    expect(await rowCount()).toBe(2);
  });
});
