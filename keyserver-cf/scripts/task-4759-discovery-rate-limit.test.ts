import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import {
  DISCOVERY_CARDS_ASK_MAX_PER_MINUTE,
  DISCOVERY_CARDS_PUBLISH_MAX_PER_MINUTE,
  handleDiscoveryCardsPublishPost,
  handleDiscoveryCardsRead,
} from "../src/endpoints/discovery-cards.js";
import {
  buildDiscoveryCard,
  currentDiscoveryEpochStamp,
  discoveryEpochIndex,
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
            if (/DELETE FROM discovery_cards WHERE writer_account_id = \?/u.test(sql)) {
              const accountId = args[0] as string;
              let changes = 0;
              for (const [key, row] of db.rows) {
                if (row.writer_account_id === accountId) {
                  db.rows.delete(key);
                  changes += 1;
                }
              }
              return { meta: { changes } } as D1Result;
            }
            if (/INSERT INTO discovery_cards/u.test(sql)) {
              const hasWriterColumns = /writer_account_id/u.test(sql);
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

function nativeLimiter(threshold: number): RateLimit {
  const callsByKey = new Map<string, number>();
  return {
    async limit({ key }: { key: string }) {
      const calls = (callsByKey.get(key) ?? 0) + 1;
      callsByKey.set(key, calls);
      return { success: calls <= threshold };
    },
  } as RateLimit;
}

function env(store: D1Database): Env {
  return {
    DB: store,
    RATE_LIMIT_5: nativeLimiter(5),
    RATE_LIMIT_10: nativeLimiter(10),
    RATE_LIMIT_120: nativeLimiter(120),
    RATE_LIMIT_1200: nativeLimiter(1200),
    RATE_LIMIT_3600: nativeLimiter(3600),
  } as unknown as Env;
}

function publishRequest(index: number): Request {
  return new Request("http://test/v1/discovery-cards/publish", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "cf-connecting-ip": "198.51.100.59",
    },
    body: JSON.stringify({
      account_id: `task-4759-publisher-${index}`,
      app_id: "discord",
      account_handle: `task-4759-${index}@example`,
      setting: "allowed",
      sealed_note: `sealed publish ${index}`,
    }),
  });
}

function readRequest(card: { drawer_name: string; label: string }): Request {
  return new Request("http://test/v1/discovery-cards/read", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "cf-connecting-ip": "198.51.100.60",
    },
    body: JSON.stringify({
      drawer_name: card.drawer_name,
      label: card.label,
    }),
  });
}

async function storedReadableCard(store: DiscoveryCardDb) {
  const epoch = currentDiscoveryEpochStamp();
  const epochIndex = discoveryEpochIndex(epoch);
  if (epochIndex === null) throw new Error("test epoch is invalid");
  const card = await buildDiscoveryCard({
    app_id: "discord",
    account_handle: "task-4759-reader@example",
    setting_material: "discord:task-4759-reader:allowed",
    sealed_note: "sealed readable",
    discovery_epoch: epoch,
  });
  store.rows.set(`${card.drawer_name}:${card.label}`, {
    ...card,
    discovery_epoch_index: epochIndex,
    updated_at: 1,
    writer_account_id: "task-4759-reader",
    writer_app_id: "discord",
    writer_setting: "allowed",
  });
  return card;
}

describe("TASK4759 discovery native rate limits", () => {
  it("lets the 5th publish through and refuses the 6th", async () => {
    const store = new DiscoveryCardDb();
    const routeEnv = env(store as unknown as D1Database);
    let request5 = 0;
    let request6 = 0;

    for (let i = 1; i <= 6; i += 1) {
      const response = await handleDiscoveryCardsPublishPost(publishRequest(i), routeEnv);
      if (i === 5) request5 = response.status;
      if (i === 6) request6 = response.status;
    }

    expect(DISCOVERY_CARDS_PUBLISH_MAX_PER_MINUTE).toBe(5);
    expect(request5).toBe(201);
    expect(request6).toBe(429);
    console.log(`TASK4759 publish_threshold=${DISCOVERY_CARDS_PUBLISH_MAX_PER_MINUTE}`);
    console.log(`TASK4759 publish_request_5_status=${request5}`);
    console.log(`TASK4759 publish_request_6_status=${request6}`);
  });

  it("lets asking traffic through at 1199 and refuses the 1201st request", async () => {
    const store = new DiscoveryCardDb();
    const card = await storedReadableCard(store);
    const routeEnv = env(store as unknown as D1Database);
    let request1199 = 0;
    let request1200 = 0;
    let request1201 = 0;

    for (let i = 1; i <= 1201; i += 1) {
      const response = await handleDiscoveryCardsRead(readRequest(card), routeEnv);
      if (i === 1199) request1199 = response.status;
      if (i === 1200) request1200 = response.status;
      if (i === 1201) request1201 = response.status;
    }

    expect(DISCOVERY_CARDS_ASK_MAX_PER_MINUTE).toBe(1200);
    expect(request1199).toBe(200);
    expect(request1200).toBe(200);
    expect(request1201).toBe(429);
    console.log(`TASK4759 ask_threshold=${DISCOVERY_CARDS_ASK_MAX_PER_MINUTE}`);
    console.log(`TASK4759 ask_request_1199_status=${request1199}`);
    console.log(`TASK4759 ask_request_1201_status=${request1201}`);
  });

  it("fails closed on request 1 when publishing is set to unsupported threshold 6", async () => {
    const store = new DiscoveryCardDb();
    const routeEnv = env(store as unknown as D1Database);
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    try {
      const response = await handleDiscoveryCardsPublishPost(
        publishRequest(1),
        routeEnv,
        6,
      );
      expect(response.status).toBe(429);
      console.log("TASK4759 publish_threshold=6");
      console.log(`TASK4759 publish_request_1_status=${response.status}`);
    } finally {
      errorSpy.mockRestore();
    }
  });
});
