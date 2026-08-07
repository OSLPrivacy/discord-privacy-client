import { describe, expect, it } from "vitest";
import {
  addDiscoveryEpochWeeks,
  buildDiscoveryCard,
  currentDiscoveryEpochStamp,
  discoveryDrawerName,
  discoveryLabel,
  handleDiscoveryCardPost,
  handleDiscoveryCardRead,
} from "../src/lib/discovery-card.js";

interface StoredDiscoveryCard {
  drawer_name: string;
  label: string;
  sealed_note: string;
  discovery_epoch: string;
  discovery_epoch_index: number;
  updated_at: number;
}

class DiscoveryCardDb {
  readonly rows = new Map<string, StoredDiscoveryCard>();

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
            if (/INSERT INTO discovery_cards/u.test(sql)) {
              const row = {
                drawer_name: args[0] as string,
                label: args[1] as string,
                sealed_note: args[2] as string,
                discovery_epoch: args[3] as string,
                discovery_epoch_index: args[4] as number,
                updated_at: args[5] as number,
              };
              db.rows.set(`${row.drawer_name}:${row.label}`, row);
              return { meta: { changes: 1 } } as D1Result;
            }
            throw new Error(`unexpected run SQL: ${sql}`);
          },
          async first<T>() {
            if (/SELECT drawer_name, label, sealed_note, discovery_epoch/u.test(sql)) {
              const key = `${args[0] as string}:${args[1] as string}`;
              const row = db.rows.get(key);
              if (!row) return null;
              return {
                drawer_name: row.drawer_name,
                label: row.label,
                sealed_note: row.sealed_note,
                discovery_epoch: row.discovery_epoch,
              } as T;
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

function jsonRequest(body: Record<string, unknown>): Request {
  return new Request("http://test/v1/discovery-cards", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

function base64(bytes: Uint8Array): string {
  let raw = "";
  for (const b of bytes) raw += String.fromCharCode(b);
  return btoa(raw);
}

describe("TASK4751 blinded discovery cards", () => {
  it("stores the four-part card, rejects missing parts, and refuses stale epochs", async () => {
    const store = db();
    const epoch = currentDiscoveryEpochStamp();
    const card = await buildDiscoveryCard({
      app_id: "discord",
      account_handle: "alice.example",
      setting_material: "phonebook-and-public-name:alice.example",
      sealed_note: base64(new TextEncoder().encode("sealed for the intended reader")),
      discovery_epoch: epoch,
    });

    const stored = await handleDiscoveryCardPost(jsonRequest({ ...card }), store);
    expect(stored.status).toBe(201);
    expect(await stored.json()).toMatchObject(card);

    const read = await handleDiscoveryCardRead(
      jsonRequest({ drawer_name: card.drawer_name, label: card.label }),
      store,
    );
    expect(read.status).toBe(200);
    const readJson = await read.json() as Record<string, string>;
    expect(readJson).toMatchObject(card);
    for (const field of ["drawer_name", "label", "sealed_note", "discovery_epoch"] as const) {
      expect(readJson[field]).toEqual(expect.any(String));
      expect(readJson[field].length).toBeGreaterThan(0);
    }

    const missingRefusals: Record<string, string> = {};
    for (const field of ["drawer_name", "label", "sealed_note", "discovery_epoch"] as const) {
      const broken: Record<string, unknown> = { ...card };
      delete broken[field];
      const res = await handleDiscoveryCardPost(jsonRequest(broken), store);
      const body = await res.json() as { error: string };
      missingRefusals[field] = body.error;
      expect(res.status).toBe(400);
      expect(body.error).toBe(`missing ${field}`);
    }

    const stale = await handleDiscoveryCardPost(
      jsonRequest({ ...card, discovery_epoch: addDiscoveryEpochWeeks(epoch, -2) }),
      store,
    );
    const staleBody = await stale.json() as { error: string };
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
    for (let offset = 0; offset < 100_000; offset += 1000) {
      const drawers = await Promise.all(
        Array.from({ length: 1000 }, (_, i) =>
          discoveryDrawerName("discord", `made-up-${offset + i}`),
        ),
      );
      for (const drawer of drawers) {
        counts.set(drawer, (counts.get(drawer) ?? 0) + 1);
      }
    }
    const minPerDrawer = Math.min(...counts.values());
    expect(counts.size).toBeLessThanOrEqual(4096);
    expect(minPerDrawer).toBeGreaterThanOrEqual(8);
    console.log(`TASK4751 drawers_for_100000=${counts.size} min_per_drawer=${minPerDrawer}`);
  }, 30_000);

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
