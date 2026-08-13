import { env, SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  addDiscoveryEpochWeeks,
  buildDiscoveryCard,
  currentDiscoveryEpochStamp,
  discoveryEpochIndex,
} from "../../src/lib/discovery-card.js";

const DB = (env as unknown as { DB: D1Database }).DB;
const TARGET = "task-4764-target";
const PROTECTED = "task-4764-protected";

async function post(path: string, body: Record<string, unknown>): Promise<Response> {
  return await SELF.fetch(`http://test${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function insertCard(
  card: Awaited<ReturnType<typeof buildDiscoveryCard>>,
  writer: string,
): Promise<void> {
  const epochIndex = discoveryEpochIndex(card.discovery_epoch);
  if (epochIndex === null) throw new Error("invalid test epoch");
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

async function count(writer: string): Promise<number> {
  return (await DB.prepare(
    "SELECT COUNT(*) AS count FROM discovery_cards WHERE writer_account_id = ?",
  ).bind(writer).first<{ count: number }>())?.count ?? 0;
}

describe("TASK4764 authoritative master off", () => {
  it("withdraws both weeks, preserves same-drawer cards, orders a racing publish, and requires re-enable", async () => {
    await DB.batch([
      DB.prepare("DELETE FROM discovery_cards WHERE writer_account_id IN (?, ?)")
        .bind(TARGET, PROTECTED),
      DB.prepare("DELETE FROM discovery_reply_state WHERE account_id IN (?, ?)")
        .bind(TARGET, PROTECTED),
    ]);

    const current = currentDiscoveryEpochStamp();
    const prior = addDiscoveryEpochWeeks(current, -1);
    const targetCards = await Promise.all([
      buildDiscoveryCard({
        app_id: "discord", account_handle: "target-current", setting_material: "target-current",
        sealed_note: "TARGET-CURRENT-BYTES", discovery_epoch: current,
      }),
      buildDiscoveryCard({
        app_id: "discord", account_handle: "target-prior", setting_material: "target-prior",
        sealed_note: "TARGET-PRIOR-BYTES", discovery_epoch: prior,
      }),
    ]);
    const protectedCards = await Promise.all(targetCards.map((target, index) => buildDiscoveryCard({
      app_id: "discord",
      account_handle: index === 0 ? "target-current" : "target-prior",
      setting_material: `protected-${index}`,
      sealed_note: index === 0 ? "PROTECTED-CURRENT-BYTES" : "PROTECTED-PRIOR-BYTES",
      discovery_epoch: target.discovery_epoch,
    })));
    for (const card of targetCards) await insertCard(card, TARGET);
    for (const card of protectedCards) await insertCard(card, PROTECTED);
    await post("/v1/discovery-cards/enable-replies", { account_id: TARGET });

    expect(targetCards.map((card) => card.drawer_name)).toEqual(
      protectedCards.map((card) => card.drawer_name),
    );
    expect(await count(TARGET)).toBe(2);
    expect(await count(PROTECTED)).toBe(2);

    const racingPublish = post("/v1/discovery-cards/publish", {
      account_id: TARGET,
      app_id: "signal",
      account_handle: "racing-publish",
      setting: "allowed",
      sealed_note: "TARGET-RACING-BYTES",
    });
    const takeBack = post("/v1/discovery-cards/take-back", { account_id: TARGET });
    const [publishResponse, offResponse] = await Promise.all([racingPublish, takeBack]);
    expect([201, 409]).toContain(publishResponse.status);
    expect(offResponse.status).toBe(200);
    expect(await count(TARGET)).toBe(0);
    expect(await count(PROTECTED)).toBe(2);

    for (const [index, card] of targetCards.entries()) {
      const read = await post("/v1/discovery-cards/read", {
        drawer_name: card.drawer_name,
        label: card.label,
      });
      expect(read.status).toBe(404);
      console.log(`TASK4764 old-drawer target=${index} drawer=${card.drawer_name} matches=0`);
    }
    for (const [index, card] of protectedCards.entries()) {
      const read = await post("/v1/discovery-cards/read", {
        drawer_name: card.drawer_name,
        label: card.label,
      });
      expect(read.status).toBe(200);
      const found = await read.json() as { sealed_note: string };
      expect(found.sealed_note).toBe(card.sealed_note);
      console.log(`TASK4764 protected=${index} drawer=${card.drawer_name} bytes=${found.sealed_note}`);
    }

    const hostile = await post("/v1/discovery-cards/publish", {
      account_id: TARGET,
      app_id: "discord",
      account_handle: "hostile-after-off",
      setting: "allowed",
      sealed_note: "TARGET-HOSTILE-BYTES",
    });
    expect(hostile.status).toBe(409);
    expect(await count(TARGET)).toBe(0);
    console.log(`TASK4764 off removed=2 target_live=${await count(TARGET)} protected_live=${await count(PROTECTED)} hostile_publish=${hostile.status}`);

    expect((await post("/v1/discovery-cards/enable-replies", { account_id: TARGET })).status).toBe(200);
    const control = await post("/v1/discovery-cards/publish", {
      account_id: TARGET,
      app_id: "discord",
      account_handle: "explicit-re-enable",
      setting: "allowed",
      sealed_note: "TARGET-REENABLED-BYTES",
    });
    expect(control.status).toBe(201);
    expect(await count(TARGET)).toBe(1);
    console.log(`TASK4764 explicit_reenable publish=1 target_live=${await count(TARGET)}`);
  });
});
