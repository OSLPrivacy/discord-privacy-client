/// Transfer-limit regression tests.
///
/// This runs the shipped limiter through a deterministic D1-shaped store. The
/// worker pool cannot boot in this checkout because an unrelated pre-existing
/// blob endpoint parse error prevents loading the Worker entry module; keeping
/// this focused node target means the transfer state machine is still executed.

import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  limitTransferStream,
  withUploadTransferLimit,
} from "../src/lib/transfer-limits.js";

const SECRET = "t".repeat(48);

class TransferLimitDb {
  readonly slots = new Map<string, { expiresAt: number }>();
  readonly windows = new Map<string, number>();

  prepare(sql: string) {
    return {
      bind: (...values: (string | number)[]) => ({
        run: async () => {
          if (sql.startsWith("DELETE FROM transfer_upload_slots WHERE client_key")) {
            const [key, second] = values;
            if (sql.includes("expires_at <=")) {
              for (const [id, slot] of this.slots) {
                if (id.startsWith(`${key}|`) && slot.expiresAt <= Number(second)) this.slots.delete(id);
              }
            } else {
              this.slots.delete(`${key}|${second}`);
            }
            return { meta: { changes: 1 } };
          }
          if (sql.startsWith("INSERT INTO transfer_upload_slots")) {
            const [key, slotId, expiresAt, countKey, now, maximum] = values;
            const active = Array.from(this.slots.entries())
              .filter(([id, slot]) => id.startsWith(`${countKey}|`) && slot.expiresAt > Number(now)).length;
            if (active >= Number(maximum)) return { meta: { changes: 0 } };
            this.slots.set(`${key}|${slotId}`, { expiresAt: Number(expiresAt) });
            return { meta: { changes: 1 } };
          }
          return { meta: { changes: 0 } };
        },
        first: async <T>() => {
          if (!sql.startsWith("INSERT INTO transfer_bandwidth_windows")) return null as T | null;
          const [key, direction, windowStart, bytes, limit] = values;
          const id = `${key}|${direction}|${windowStart}`;
          const used = this.windows.get(id) ?? 0;
          if (used + Number(bytes) > Number(limit)) return null as T | null;
          const usedBytes = used + Number(bytes);
          this.windows.set(id, usedBytes);
          return { used_bytes: usedBytes } as T;
        },
      }),
    };
  }
}

function env(db: TransferLimitDb): Env {
  return {
    DB: db as unknown as D1Database,
    RATE_LIMIT_HASH_KEY: SECRET,
  } as Env;
}

function request(): Request {
  return new Request("https://cipher.test/v1/attachment", { method: "POST" });
}

function streamOf(...chunks: Uint8Array[]): ReadableStream<Uint8Array> {
  return new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(chunk);
      controller.close();
    },
  });
}

describe("per-client transfer limits", () => {
  it("refuses a fourth concurrent upload by the named concurrency limit", async () => {
    const db = new TransferLimitDb();
    const person = "198.51.100.83";
    let opened = 0;
    let release!: () => void;
    const hold = new Promise<void>((resolve) => { release = resolve; });
    let allOpen!: () => void;
    const openedThree = new Promise<void>((resolve) => { allOpen = resolve; });
    const handler = async (): Promise<Response> => {
      opened += 1;
      if (opened === 3) allOpen();
      await hold;
      return new Response(null, { status: 201 });
    };

    const running = Array.from(
      { length: 3 },
      () => withUploadTransferLimit(request(), env(db), person, handler),
    );
    await openedThree;

    const fourth = await withUploadTransferLimit(request(), env(db), person, handler);
    expect(fourth.status).toBe(429);
    expect(await fourth.json()).toMatchObject({ error: "upload_concurrency_limit" });

    release();
    await expect(Promise.all(running)).resolves.toHaveLength(3);
    expect(JSON.stringify([...db.slots.keys()])).not.toContain(person);
  });

  it("slows a download over its byte budget instead of cutting it off", async () => {
    const db = new TransferLimitDb();
    let clock = 100;
    const sleeps: number[] = [];
    const limited = limitTransferStream(
      streamOf(new Uint8Array([1, 2, 3, 4]), new Uint8Array([5])),
      env(db),
      "198.51.100.84",
      "download",
      {
        bytesPerSecond: 4,
        now: () => clock,
        sleep: async (milliseconds) => {
          sleeps.push(milliseconds);
          clock += milliseconds;
        },
      },
    );
    const reader = limited.getReader();

    expect((await reader.read()).value).toEqual(new Uint8Array([1, 2, 3, 4]));
    const second = await reader.read();
    expect(sleeps).toEqual([900]);
    expect(second.done).toBe(false);
    expect(second.value).toEqual(new Uint8Array([5]));
    expect((await reader.read()).done).toBe(true);
  });
});
