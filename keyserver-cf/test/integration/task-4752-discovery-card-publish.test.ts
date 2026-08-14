import { env, SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  addDiscoveryEpochWeeks,
  buildDiscoveryCard,
  currentDiscoveryEpochStamp,
  discoveryEpochIndex,
} from "../../src/lib/discovery-card.js";
import { base64Encode } from "./helpers.js";

const DB = (env as unknown as { DB: D1Database }).DB;

async function accountRowCount(accountId: string): Promise<number> {
  const row = await DB.prepare(
    "SELECT COUNT(*) AS count FROM discovery_cards WHERE writer_account_id = ?",
  ).bind(accountId).first<{ count: number }>();
  return row?.count ?? 0;
}

async function publish(setting: "allowed" | "shared-room") {
  const res = await SELF.fetch("http://test/v1/discovery-cards/publish", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      account_id: "task-4752-account",
      app_id: "discord",
      account_handle: "task-4752@example",
      setting,
      sealed_note: base64Encode(new TextEncoder().encode(`sealed ${setting}`)),
    }),
  });
  const body = await res.json() as {
    removed: number;
    wrote: number;
    card: { drawer_name: string; label: string };
  };
  expect(res.status).toBe(201);
  return body;
}

async function takeBack() {
  const res = await SELF.fetch("http://test/v1/discovery-cards/take-back", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ account_id: "task-4752-account" }),
  });
  const body = await res.json() as { removed: number };
  expect(res.status).toBe(200);
  return body;
}

describe("TASK4752 publish a card and take it back", () => {
  it("publishes, takes back, switches, and repeats without duplicate live cards", async () => {
    await DB.prepare(
      "DELETE FROM discovery_cards WHERE writer_account_id IN (?, ?)",
    ).bind("task-4752-account", "task-4752-other").run();

    const initial = await accountRowCount("task-4752-account");
    expect(initial).toBe(0);

    const first = await publish("allowed");
    const afterFirst = await accountRowCount("task-4752-account");
    expect(first.wrote).toBe(1);
    expect(afterFirst).toBe(1);
    console.log(`TASK4752 publish allowed wrote ${first.wrote} rows ${initial}->${afterFirst}`);

    const back = await takeBack();
    const afterBack = await accountRowCount("task-4752-account");
    expect(back.removed).toBe(1);
    expect(afterBack).toBe(0);
    console.log(`TASK4752 take-back removed ${back.removed} rows ${afterBack}`);

    const allowedAgain = await publish("allowed");
    const afterAllowedAgain = await accountRowCount("task-4752-account");
    expect(allowedAgain.wrote).toBe(1);
    expect(afterAllowedAgain).toBe(1);

    const switched = await publish("shared-room");
    const afterSwitch = await accountRowCount("task-4752-account");
    expect(switched.removed).toBe(1);
    expect(switched.wrote).toBe(1);
    expect(afterSwitch).toBe(1);
    expect(switched.card.label).not.toBe(allowedAgain.card.label);
    console.log(`TASK4752 switch removed ${switched.removed} wrote ${switched.wrote} rows ${afterAllowedAgain}->${afterSwitch}`);

    const repeat = await publish("shared-room");
    const afterRepeat = await accountRowCount("task-4752-account");
    expect(repeat.wrote).toBe(1);
    expect(afterRepeat).toBe(1);
    console.log(`TASK4752 repeat wrote ${repeat.wrote} rows ${afterSwitch}->${afterRepeat}`);
  });

  it("take-back deletes this account across drawers and both kept weeks", async () => {
    await DB.prepare(
      "DELETE FROM discovery_cards WHERE writer_account_id IN (?, ?)",
    ).bind("task-4752-account", "task-4752-other").run();

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
        app_id: "discord",
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
      const writer = index === 2 ? "task-4752-other" : "task-4752-account";
      const epochIndex = discoveryEpochIndex(card.discovery_epoch);
      if (epochIndex === null) throw new Error("test epoch is invalid");
      await DB.prepare(
        `INSERT INTO discovery_cards
           (drawer_name, label, sealed_note, discovery_epoch, discovery_epoch_index,
            updated_at, writer_account_id, writer_app_id, writer_setting)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      ).bind(
        card.drawer_name,
        card.label,
        card.sealed_note,
        card.discovery_epoch,
        epochIndex,
        1,
        writer,
        "discord",
        "allowed",
      ).run();
    }

    expect(await accountRowCount("task-4752-account")).toBe(2);
    const back = await takeBack();
    expect(back.removed).toBe(2);
    expect(await accountRowCount("task-4752-account")).toBe(0);
    expect(await accountRowCount("task-4752-other")).toBe(1);
    console.log(`TASK4752 two-week-all-drawers removed ${back.removed} survivor_other=${await accountRowCount("task-4752-other")}`);
  });
});
