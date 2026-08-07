import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  addDiscoveryEpochWeeks,
  buildDiscoveryCard,
  currentDiscoveryEpochStamp,
  discoveryDrawerName,
  discoveryLabel,
} from "../../src/lib/discovery-card.js";
import { base64Encode } from "./helpers.js";

async function postCard(body: Record<string, unknown>): Promise<Response> {
  return await SELF.fetch("http://test/v1/discovery-cards", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

describe("TASK4751 blinded discovery cards", () => {
  it("stores the four-part card, rejects missing parts, and refuses stale epochs", async () => {
    const epoch = currentDiscoveryEpochStamp();
    const card = await buildDiscoveryCard({
      app_id: "discord",
      account_handle: "alice.example",
      setting_material: "phonebook-and-public-name:alice.example",
      sealed_note: base64Encode(new TextEncoder().encode("sealed for the intended reader")),
      discovery_epoch: epoch,
    });

    const stored = await postCard({ ...card });
    expect(stored.status).toBe(201);
    const storedJson = await stored.json();
    expect(storedJson).toMatchObject(card);

    const read = await SELF.fetch("http://test/v1/discovery-cards/read", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        drawer_name: card.drawer_name,
        label: card.label,
      }),
    });
    expect(read.status).toBe(200);
    const readJson = await read.json();
    expect(readJson).toMatchObject(card);
    for (const field of ["drawer_name", "label", "sealed_note", "discovery_epoch"] as const) {
      expect(readJson[field]).toEqual(expect.any(String));
      expect(readJson[field].length).toBeGreaterThan(0);
    }

    const missingRefusals: Record<string, string> = {};
    for (const field of ["drawer_name", "label", "sealed_note", "discovery_epoch"] as const) {
      const broken: Record<string, unknown> = { ...card };
      delete broken[field];
      const res = await postCard(broken);
      const body = (await res.json()) as { error: string };
      missingRefusals[field] = body.error;
      expect(res.status).toBe(400);
      expect(body.error).toBe(`missing ${field}`);
    }

    const stale = await postCard({
      ...card,
      discovery_epoch: addDiscoveryEpochWeeks(epoch, -2),
    });
    const staleBody = (await stale.json()) as { error: string };
    expect(stale.status).toBe(400);
    expect(staleBody.error).toBe("stale discovery epoch");

    console.log(
      `TASK4751 example drawer_name=${readJson.drawer_name} label=${readJson.label} ` +
      `sealed_note_len=${readJson.sealed_note.length} discovery_epoch=${readJson.discovery_epoch}`,
    );
    console.log(`TASK4751 missing_refusals=${JSON.stringify(missingRefusals)}`);
    console.log(`TASK4751 stale_refusal=${staleBody.error}`);
  });

  it("keeps made-up handles clustered into short drawers", async () => {
    const counts = new Map<string, number>();
    for (let i = 0; i < 100_000; i += 1) {
      const drawer = await discoveryDrawerName("discord", `made-up-${i}`);
      counts.set(drawer, (counts.get(drawer) ?? 0) + 1);
    }
    const minPerDrawer = Math.min(...counts.values());
    expect(counts.size).toBeLessThanOrEqual(4096);
    expect(minPerDrawer).toBeGreaterThanOrEqual(8);
    console.log(
      `TASK4751 drawers_for_100000=${counts.size} min_per_drawer=${minPerDrawer}`,
    );
  });

  it("rotates labels weekly while drawer names stay stable", async () => {
    const handle = "same-handle@example";
    const drawer = await discoveryDrawerName("discord", handle);
    const labels = new Set<string>();
    const drawers = new Set<string>();
    let epoch = "2026-W01";
    for (let i = 0; i < 52; i += 1) {
      drawers.add(await discoveryDrawerName("discord", handle));
      labels.add(await discoveryLabel(`phonebook-and-public-name:${handle}`, epoch));
      epoch = addDiscoveryEpochWeeks(epoch, 1);
    }
    expect(labels.size).toBe(52);
    expect(drawers.size).toBe(1);
    expect(drawer).toBe([...drawers][0]);
    console.log(
      `TASK4751 labels_across_52=${labels.size} label_repeats=${52 - labels.size} ` +
      `drawers_across_52=${drawers.size} drawer=${drawer}`,
    );
  });
});
