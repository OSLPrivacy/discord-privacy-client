import { describe, expect, it, vi } from "vitest";
import worker from "../src/index.js";
import type { Env } from "../src/env.js";
import {
  BLOB_SWEEP_MAX_D1_QUERIES,
  sweepExpired,
} from "../src/lib/sweep.js";

type ExpiredRow = { blob_id: string; fetch_digest_sha256_hex: string };

function sweepEnv(expiredRows: number) {
  const rows: ExpiredRow[] = Array.from({ length: expiredRows }, (_, index) => ({
    blob_id: index.toString(16).padStart(32, "0"),
    fetch_digest_sha256_hex: (index + 1).toString(16).padStart(64, "0"),
  }));
  let d1Queries = 0;
  const deletedPayloads = vi.fn(async (_key: string | string[]) => undefined);

  const db = {
    prepare(sql: string) {
      d1Queries += 1;
      return {
        bind(...parameters: unknown[]) {
          return {
            async all<T>() {
              if (sql.includes("FROM blob_capability_index")) {
                return { results: rows.slice(0, 100) as T[] };
              }
              return { results: [] as T[] };
            },
            async first<T>() {
              void parameters;
              return null as T | null;
            },
            async run() {
              if (sql.includes("DELETE FROM blob_capability_index")) {
                const ids = parameters.slice(1) as string[];
                const selected = new Set(ids);
                for (let index = rows.length - 1; index >= 0; index--) {
                  if (selected.has(rows[index]!.blob_id)) rows.splice(index, 1);
                }
                return { meta: { changes: ids.length } };
              }
              return { meta: { changes: 0 } };
            },
          };
        },
      };
    },
  };

  return {
    env: {
      DB: db,
      PAYLOADS: { delete: deletedPayloads },
    } as unknown as Env,
    d1Queries: () => d1Queries,
    deletedPayloads,
    remainingRows: () => rows.length,
  };
}

const scheduledEvent = { cron: "*/5 * * * *", type: "scheduled", scheduledTime: 0 } as ScheduledEvent;
const executionContext = { waitUntil() {}, passThroughOnException() {} } as ExecutionContext;

describe("bounded blob TTL sweep", () => {
  it("drains 5,000 expired rows across scheduled invocations without approaching D1's limit", async () => {
    const fixture = sweepEnv(5_000);
    const errors = vi.spyOn(console, "error").mockImplementation(() => undefined);

    await worker.scheduled(scheduledEvent, fixture.env, executionContext);
    expect(fixture.d1Queries()).toBeLessThanOrEqual(900);
    expect(fixture.remainingRows()).toBe(0);
    expect(fixture.deletedPayloads).toHaveBeenCalledTimes(5_000);

    await worker.scheduled(scheduledEvent, fixture.env, executionContext);
    expect(fixture.remainingRows()).toBe(0);
    errors.mockRestore();
  });

  it("stops before the reserved D1 budget and leaves the remainder for the next cron", async () => {
    const fixture = sweepEnv(100_000);

    await expect(sweepExpired(fixture.env)).resolves.toBe(26_600);
    expect(fixture.d1Queries()).toBe(BLOB_SWEEP_MAX_D1_QUERIES);
    expect(fixture.d1Queries()).toBeLessThanOrEqual(900);
    expect(fixture.remainingRows()).toBe(73_400);
  });
});
