/// TASK 0584 — prove a normal 20 MiB transfer survives a separate account's
/// fastest possible upload, while that hammer never exceeds the published cap.
///
/// This is intentionally an end-to-end stream exercise of the limiter used by
/// the Worker routes, with a deterministic clock in place of 20 seconds of
/// wall time.  Each account gets its own clock but shares the same D1-shaped
/// store, just as two addresses share the deployed service.

import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  UPLOAD_BYTES_PER_SECOND,
  limitTransferStream,
} from "../src/lib/transfer-limits.js";

const MIB = 1024 * 1024;
const NORMAL_FILE_BYTES = 20 * MIB;
const SECRET = "e".repeat(48);

class TransferLimitDb {
  readonly windows = new Map<string, number>();

  prepare(sql: string) {
    return {
      bind: (...values: (string | number)[]) => ({
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
  return { DB: db as unknown as D1Database, RATE_LIMIT_HASH_KEY: SECRET } as Env;
}

function bytes(length: number): Uint8Array {
  return Uint8Array.from({ length }, (_, index) => (index * 31 + 17) & 0xff);
}

function fastSource(payload: Uint8Array): ReadableStream<Uint8Array> {
  return new ReadableStream<Uint8Array>({
    start(controller) {
      // A single eager 20 MiB source is the hammer: the consumer asks for all
      // bytes immediately, leaving pacing entirely to the production limiter.
      controller.enqueue(payload);
      controller.close();
    },
  });
}

function virtualClock() {
  let milliseconds = 100;
  const waits: number[] = [];
  return {
    now: () => milliseconds,
    sleep: async (wait: number) => {
      waits.push(wait);
      milliseconds += wait;
    },
    elapsed: () => milliseconds - 100,
    waits,
  };
}

async function readAndFingerprint(stream: ReadableStream<Uint8Array>): Promise<{ bytes: number; sha256: string }> {
  const reader = stream.getReader();
  const hash = createHash("sha256");
  let total = 0;
  while (true) {
    const next = await reader.read();
    if (next.done) break;
    total += next.value.byteLength;
    hash.update(next.value);
  }
  return { bytes: total, sha256: hash.digest("hex") };
}

describe("TASK 0584 transfer-speed normal-use check", () => {
  it("finishes a normal 20 MiB send intact while another account hammers at, never above, 1 MiB/s", async () => {
    const db = new TransferLimitDb();
    const original = bytes(NORMAL_FILE_BYTES);
    const originalFingerprint = createHash("sha256").update(original).digest("hex");
    const normalClock = virtualClock();
    const hammerClock = virtualClock();

    const normal = limitTransferStream(fastSource(original), env(db), "198.51.100.10", "upload", {
      bytesPerSecond: UPLOAD_BYTES_PER_SECOND,
      now: normalClock.now,
      sleep: normalClock.sleep,
    });
    const hammer = limitTransferStream(fastSource(bytes(NORMAL_FILE_BYTES)), env(db), "198.51.100.11", "upload", {
      bytesPerSecond: UPLOAD_BYTES_PER_SECOND,
      now: hammerClock.now,
      sleep: hammerClock.sleep,
    });

    const [normalResult, hammerResult] = await Promise.all([
      readAndFingerprint(normal),
      readAndFingerprint(hammer),
    ]);

    // The normal account got every byte and its post-transfer fingerprint is
    // exactly its original fingerprint, even while the other account flooded.
    expect(normalResult).toEqual({ bytes: NORMAL_FILE_BYTES, sha256: originalFingerprint });

    // Twenty 1 MiB chunks per account consume twenty distinct one-second
    // windows. The hammer is therefore slowed, and no window is allowed more
    // than the stated 1 MiB/s upload cap.
    expect(hammerResult.bytes).toBe(NORMAL_FILE_BYTES);
    expect(db.windows).toHaveLength(40);
    expect([...db.windows.values()].every((used) => used > 0 && used <= UPLOAD_BYTES_PER_SECOND)).toBe(true);
    // The first MiB enters the partly elapsed initial window (t=100ms), then
    // the other nineteen need later windows: 18,900ms of enforced delay.
    expect(hammerClock.elapsed()).toBeGreaterThanOrEqual(18_900);
    expect(hammerClock.waits).toHaveLength(19);

    console.info(
      `TASK 0584 normal_bytes=${normalResult.bytes} normal_sha256=${normalResult.sha256} `
      + `hammer_bytes=${hammerResult.bytes} hammer_windows=${db.windows.size} `
      + `hammer_max_window_bytes=${Math.max(...db.windows.values())} `
      + `upload_cap_bytes_per_second=${UPLOAD_BYTES_PER_SECOND} hammer_virtual_ms=${hammerClock.elapsed()}`,
    );
  });
});
