import { describe, expect, it } from "vitest";
import {
  addDiscoveryEpochWeeks,
  buildDiscoveryCard,
  currentDiscoveryEpochStamp,
  discoveryEpochIndex,
  handleDiscoveryCardsPublish,
  handleDiscoveryCardsTakeBack,
  handleDiscoveryRepliesEnable,
} from "../src/lib/discovery-card.js";

interface StoredDiscoveryCard {
  drawer_name: string;
  label: string;
  sealed_note: string;
  discovery_epoch: string;
  discovery_epoch_index: number;
  updated_at: number;
  writer_account_id: string | null;
  writer_app_id: string | null;
  writer_setting: string | null;
}

class DiscoveryCardDb {
  readonly rows = new Map<string, StoredDiscoveryCard>();
  readonly enabled = new Map<string, boolean>();

  async batch<T = unknown>(statements: D1PreparedStatement[]): Promise<D1Result<T>[]> {
    const results: D1Result<T>[] = [];
    for (const statement of statements) results.push(await statement.run<T>());
    return results;
  }

  prepare(sql: string): D1PreparedStatement {
    const db = this;
    return {
      bind(...args: unknown[]) {
        return {
          async run() {
            if (/DELETE FROM discovery_cards WHERE discovery_epoch_index < \?/u.test(sql)) {
              const threshold = args[0] as number;
              let changes = 0;
              for (const [key, row] of db.rows) {
                if (row.discovery_epoch_index < threshold) {
                  db.rows.delete(key);
                  changes += 1;
                }
              }
              return { meta: { changes } } as D1Result;
            }
            if (/INSERT OR IGNORE INTO discovery_reply_state/u.test(sql)) {
              const accountId = args[0] as string;
              if (!db.enabled.has(accountId)) db.enabled.set(accountId, true);
              return { meta: { changes: 1 } } as D1Result;
            }
            if (/INSERT INTO discovery_reply_state/u.test(sql)) {
              const accountId = args[0] as string;
              db.enabled.set(accountId, /VALUES \(\?1, 1,/u.test(sql));
              return { meta: { changes: 1 } } as D1Result;
            }
            if (/DELETE FROM discovery_cards[\s\S]*writer_account_id = \?/u.test(sql)) {
              const accountId = args[0] as string;
              if (/AND EXISTS/u.test(sql) && db.enabled.get(accountId) !== true) {
                return { meta: { changes: 0 } } as D1Result;
              }
              let changes = 0;
              for (const [key, row] of db.rows) {
                const scopedPublishDelete = /writer_app_id/u.test(sql);
                if (
                  row.writer_account_id === accountId
                  && (!scopedPublishDelete
                    || (row.writer_app_id === args[1]
                      && row.discovery_epoch_index === args[2]))
                ) {
                  db.rows.delete(key);
                  changes += 1;
                }
              }
              return { meta: { changes } } as D1Result;
            }
            if (/INSERT INTO discovery_cards/u.test(sql)) {
              const hasWriterColumns = /writer_account_id/u.test(sql);
              if (/WHERE EXISTS/u.test(sql) && db.enabled.get(args[6] as string) !== true) {
                return { meta: { changes: 0 } } as D1Result;
              }
              const row = {
                drawer_name: args[0] as string,
                label: args[1] as string,
                sealed_note: args[2] as string,
                discovery_epoch: args[3] as string,
                discovery_epoch_index: args[4] as number,
                updated_at: args[5] as number,
                writer_account_id: hasWriterColumns ? args[6] as string : null,
                writer_app_id: hasWriterColumns ? args[7] as string : null,
                writer_setting: hasWriterColumns ? args[8] as string : null,
              };
              db.rows.set(`${row.drawer_name}:${row.label}`, row);
              return { meta: { changes: 1 } } as D1Result;
            }
            throw new Error(`unexpected run SQL: ${sql}`);
          },
          async first<T>() {
            if (/SELECT COUNT\(\*\) AS count FROM discovery_cards/u.test(sql)) {
              const accountId = args[0] as string;
              let count = 0;
              for (const row of db.rows.values()) {
                if (row.writer_account_id === accountId) count += 1;
              }
              return { count } as T;
            }
            throw new Error(`unexpected first SQL: ${sql}`);
          },
        };
      },
    } as D1PreparedStatement;
  }
}

function db(): D1Database {
  return new DiscoveryCardDb() as unknown as D1Database;
}

function jsonRequest(url: string, body: Record<string, unknown>): Request {
  return new Request(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function accountRowCount(store: D1Database, accountId: string): Promise<number> {
  const row = await store
    .prepare("SELECT COUNT(*) AS count FROM discovery_cards WHERE writer_account_id = ?")
    .bind(accountId)
    .first<{ count: number }>();
  return row?.count ?? 0;
}

async function publish(store: D1Database, setting: "allowed" | "shared-room") {
  const response = await handleDiscoveryCardsPublish(
    jsonRequest("http://test/v1/discovery-cards/publish", {
      account_id: "task-4752-account",
      app_id: "discord",
      account_handle: "task-4752@example",
      setting,
      sealed_note: `sealed ${setting}`,
    }),
    store,
  );
  expect(response.status).toBe(201);
  return await response.json() as {
    removed: number;
    wrote: number;
    card: { label: string };
  };
}

async function takeBack(store: D1Database) {
  const response = await handleDiscoveryCardsTakeBack(
    jsonRequest("http://test/v1/discovery-cards/take-back", {
      account_id: "task-4752-account",
    }),
    store,
  );
  expect(response.status).toBe(200);
  return await response.json() as { removed: number };
}

async function enableReplies(store: D1Database) {
  const response = await handleDiscoveryRepliesEnable(
    jsonRequest("http://test/v1/discovery-cards/enable-replies", {
      account_id: "task-4752-account",
    }),
    store,
  );
  expect(response.status).toBe(200);
}

describe("TASK4752 publish a card and take it back", () => {
  it("publishes, takes back, switches, and repeats without duplicate live cards", async () => {
    const store = db();
    await enableReplies(store);
    const initial = await accountRowCount(store, "task-4752-account");
    expect(initial).toBe(0);

    const first = await publish(store, "allowed");
    const afterFirst = await accountRowCount(store, "task-4752-account");
    expect(first.wrote).toBe(1);
    expect(afterFirst).toBe(1);
    console.log(`TASK4752 publish allowed wrote ${first.wrote} rows ${initial}->${afterFirst}`);

    const back = await takeBack(store);
    const afterBack = await accountRowCount(store, "task-4752-account");
    expect(back.removed).toBe(1);
    expect(afterBack).toBe(0);
    console.log(`TASK4752 take-back removed ${back.removed} rows ${afterBack}`);

    await enableReplies(store);
    const allowedAgain = await publish(store, "allowed");
    const afterAllowedAgain = await accountRowCount(store, "task-4752-account");
    expect(allowedAgain.wrote).toBe(1);
    expect(afterAllowedAgain).toBe(1);

    const switched = await publish(store, "shared-room");
    const afterSwitch = await accountRowCount(store, "task-4752-account");
    expect(switched.removed).toBe(1);
    expect(switched.wrote).toBe(1);
    expect(afterSwitch).toBe(1);
    expect(switched.card.label).not.toBe(allowedAgain.card.label);
    console.log(
      `TASK4752 switch removed ${switched.removed} wrote ${switched.wrote} ` +
        `rows ${afterAllowedAgain}->${afterSwitch}`,
    );

    const repeat = await publish(store, "shared-room");
    const afterRepeat = await accountRowCount(store, "task-4752-account");
    expect(repeat.wrote).toBe(1);
    expect(afterRepeat).toBe(1);
    console.log(`TASK4752 repeat wrote ${repeat.wrote} rows ${afterSwitch}->${afterRepeat}`);
  });

  it("take-back deletes this account across drawers and both kept weeks", async () => {
    const store = db() as unknown as DiscoveryCardDb & D1Database;
    await enableReplies(store);
    const current = currentDiscoveryEpochStamp();
    const previous = addDiscoveryEpochWeeks(current, -1);
    const cards = [
      await buildDiscoveryCard({
        app_id: "discord",
        account_handle: "drawer-a@example",
        setting_material: "discord:task-4752-account:allowed:a",
        sealed_note: "sealed-a",
        discovery_epoch: current,
      }),
      await buildDiscoveryCard({
        app_id: "signal",
        account_handle: "drawer-b@example",
        setting_material: "discord:task-4752-account:allowed:b",
        sealed_note: "sealed-b",
        discovery_epoch: previous,
      }),
      await buildDiscoveryCard({
        app_id: "discord",
        account_handle: "other@example",
        setting_material: "discord:task-4752-other:allowed",
        sealed_note: "sealed-other",
        discovery_epoch: current,
      }),
    ];
    for (const [index, card] of cards.entries()) {
      const epochIndex = discoveryEpochIndex(card.discovery_epoch);
      if (epochIndex === null) throw new Error("test epoch is invalid");
      const writer = index === 2 ? "task-4752-other" : "task-4752-account";
      store.rows.set(`${card.drawer_name}:${card.label}`, {
        ...card,
        discovery_epoch_index: epochIndex,
        updated_at: 1,
        writer_account_id: writer,
        writer_app_id: "discord",
        writer_setting: "allowed",
      });
    }

    expect(await accountRowCount(store, "task-4752-account")).toBe(2);
    const back = await takeBack(store);
    expect(back.removed).toBe(2);
    expect(await accountRowCount(store, "task-4752-account")).toBe(0);
    expect(await accountRowCount(store, "task-4752-other")).toBe(1);
    console.log(
      `TASK4752 two-week-all-drawers removed ${back.removed} ` +
        `survivor_other=${await accountRowCount(store, "task-4752-other")}`,
    );
  });
});

describe("TASK4764 authoritative off ordering", () => {
  it("deletes every target in both retained weeks, preserves same-drawer protected bytes, and refuses publish after off", async () => {
    const store = db() as unknown as DiscoveryCardDb & D1Database;
    const current = currentDiscoveryEpochStamp();
    const previous = addDiscoveryEpochWeeks(current, -1);
    const target = "task-4764-target";
    const protectedAccount = "task-4764-protected";
    const rows: StoredDiscoveryCard[] = [];
    for (const [drawer, epoch] of [["a10", current], ["a10", previous], ["b20", current], ["b20", previous]] as const) {
      for (const [writer, prefix] of [[target, "TARGET"], [protectedAccount, "PROTECTED"]] as const) {
        const label = `${prefix}-${drawer}-${epoch}`.padEnd(22, "x");
        rows.push({
          drawer_name: drawer,
          label,
          sealed_note: `${prefix}-BYTES-${drawer}-${epoch}`,
          discovery_epoch: epoch,
          discovery_epoch_index: discoveryEpochIndex(epoch)!,
          updated_at: 1,
          writer_account_id: writer,
          writer_app_id: "discord",
          writer_setting: "allowed",
        });
      }
    }
    for (const row of rows) store.rows.set(`${row.drawer_name}:${row.label}`, row);

    const enable = await handleDiscoveryRepliesEnable(
      jsonRequest("http://test/v1/discovery-cards/enable-replies", { account_id: target }),
      store,
    );
    expect(enable.status).toBe(200);
    const beforeRace = await handleDiscoveryCardsPublish(
      jsonRequest("http://test/v1/discovery-cards/publish", {
        account_id: target,
        app_id: "discord",
        account_handle: "race-before-off",
        setting: "allowed",
        sealed_note: "TARGET-RACING-BYTES",
      }),
      store,
    );
    expect(beforeRace.status).toBe(201);

    const off = await handleDiscoveryCardsTakeBack(
      jsonRequest("http://test/v1/discovery-cards/take-back", { account_id: target }),
      store,
    );
    expect(off.status).toBe(200);
    expect(await accountRowCount(store, target)).toBe(0);
    expect(await accountRowCount(store, protectedAccount)).toBe(4);
    const protectedBefore = rows
      .filter((row) => row.writer_account_id === protectedAccount)
      .map((row) => `${row.drawer_name}:${row.label}:${row.sealed_note}`)
      .sort();
    const protectedAfter = Array.from(store.rows.values())
      .filter((row) => row.writer_account_id === protectedAccount)
      .map((row) => `${row.drawer_name}:${row.label}:${row.sealed_note}`)
      .sort();
    expect(protectedAfter).toEqual(protectedBefore);

    const afterOff = await handleDiscoveryCardsPublish(
      jsonRequest("http://test/v1/discovery-cards/publish", {
        account_id: target,
        app_id: "discord",
        account_handle: "race-after-off",
        setting: "allowed",
        sealed_note: "TARGET-AFTER-OFF-BYTES",
      }),
      store,
    );
    expect(afterOff.status).toBe(409);
    expect(await accountRowCount(store, target)).toBe(0);
    console.log("TASK4764 server_order target_live=0 protected_live=4 publish_after_off=409 drawers=a10,b20 weeks=2");
  });
});
