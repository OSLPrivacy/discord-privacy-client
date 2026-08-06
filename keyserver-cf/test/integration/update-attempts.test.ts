import { env, SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

const DB = (env as unknown as { DB: D1Database }).DB;
const URL = "http://test/v1/update-attempts";

async function countRecords(): Promise<number> {
  const row = await DB.prepare(
    "SELECT COUNT(*) AS count FROM update_attempt_records",
  ).first<{ count: number }>();
  return Number(row?.count ?? 0);
}

describe("POST /v1/update-attempts", () => {
  it("starts empty, then records one installed and one failed update attempt", async () => {
    const before = await countRecords();
    expect(before).toBe(0);
    console.log(`TASK3175_BEFORE_LINES=${before}`);

    const startedAt = Date.now();
    const installed = await SELF.fetch(URL, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-forwarded-for": "192.0.2.175",
      },
      body: JSON.stringify({
        fromVersion: "0.0.1",
        toVersion: "0.0.2",
        result: "installed",
      }),
    });
    expect(installed.status).toBe(200);
    const installedBody = (await installed.json()) as {
      status: string;
      fromVersion: string;
      toVersion: string;
      result: string;
      recordedAtUnixMs: number;
    };
    expect(installedBody).toMatchObject({
      status: "recorded",
      fromVersion: "0.0.1",
      toVersion: "0.0.2",
      result: "installed",
    });

    const failed = await SELF.fetch(URL, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-forwarded-for": "192.0.2.176",
      },
      body: JSON.stringify({
        fromVersion: "0.0.2",
        toVersion: "0.0.3",
        result: "failed",
      }),
    });
    expect(failed.status).toBe(200);
    const failedBody = (await failed.json()) as {
      status: string;
      fromVersion: string;
      toVersion: string;
      result: string;
      recordedAtUnixMs: number;
    };
    expect(failedBody).toMatchObject({
      status: "recorded",
      fromVersion: "0.0.2",
      toVersion: "0.0.3",
      result: "failed",
    });

    const endedAt = Date.now();
    const rows = await DB.prepare(
      `SELECT from_version, to_version, result, recorded_at_unix_ms
       FROM update_attempt_records
       ORDER BY id`,
    ).all<{
      from_version: string;
      to_version: string;
      result: string;
      recorded_at_unix_ms: number;
    }>();
    expect(rows.results).toEqual([
      {
        from_version: "0.0.1",
        to_version: "0.0.2",
        result: "installed",
        recorded_at_unix_ms: installedBody.recordedAtUnixMs,
      },
      {
        from_version: "0.0.2",
        to_version: "0.0.3",
        result: "failed",
        recorded_at_unix_ms: failedBody.recordedAtUnixMs,
      },
    ]);
    for (const row of rows.results) {
      expect(row.recorded_at_unix_ms).toBeGreaterThanOrEqual(startedAt);
      expect(row.recorded_at_unix_ms).toBeLessThanOrEqual(endedAt);
    }

    const results = rows.results.map((row) => row.result);
    console.log(`TASK3175_AFTER_LINES=${rows.results.length}`);
    console.log(`TASK3175_RESULTS=${results.join(",")}`);
    console.log(
      `TASK3175_FIRST from=${rows.results[0]!.from_version} to=${rows.results[0]!.to_version} result=${rows.results[0]!.result} time=${rows.results[0]!.recorded_at_unix_ms}`,
    );
    console.log(
      `TASK3175_SECOND from=${rows.results[1]!.from_version} to=${rows.results[1]!.to_version} result=${rows.results[1]!.result} time=${rows.results[1]!.recorded_at_unix_ms}`,
    );
  });
});
