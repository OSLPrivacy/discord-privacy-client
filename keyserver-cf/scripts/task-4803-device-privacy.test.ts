import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import {
  DEVICE_LOOKUP_RESPONSE_BYTES,
  handleDevicesLookup,
} from "../src/endpoints/devices.js";
import type { Env } from "../src/env.js";

type DeviceRow = { device_id: string; prekey_bundle: string };
type DeviceLookupBody = { devices: DeviceRow[]; pad: string };

function bytes(value: string): number {
  return new TextEncoder().encode(value).length;
}

function limiter(
  fn: ({ key }: { key: string }) => Promise<{ success: boolean }>,
): RateLimit {
  return { limit: fn } as unknown as RateLimit;
}

function envFor(args: {
  rowsByUser?: Map<string, DeviceRow[]>;
  liveUsers?: Set<string>;
  endpointLimit?: ReturnType<typeof vi.fn>;
  mutationLimit?: ReturnType<typeof vi.fn>;
  publicGetLimit?: ReturnType<typeof vi.fn>;
} = {}): Env & { prepare: ReturnType<typeof vi.fn> } {
  const rowsByUser = args.rowsByUser ?? new Map<string, DeviceRow[]>();
  const liveUsers = args.liveUsers ?? new Set<string>();
  const all = vi.fn(async (userId: string) => ({
    results: liveUsers.has(userId) ? rowsByUser.get(userId) ?? [] : [],
  }));
  const prepare = vi.fn(() => ({
    bind: (userId: string) => ({ all: () => all(userId) }),
  }));
  const endpointLimit = args.endpointLimit ?? vi.fn(async () => ({ success: true }));
  const mutationLimit = args.mutationLimit ?? vi.fn(async () => ({ success: true }));
  const publicGetLimit = args.publicGetLimit ?? vi.fn(async () => ({ success: true }));
  return {
    DB: { prepare } as unknown as D1Database,
    RATE_LIMIT_120: limiter(endpointLimit),
    RATE_LIMIT_1200: limiter(publicGetLimit),
    RATE_LIMIT_3600: limiter(mutationLimit),
    prepare,
  } as Env & { prepare: ReturnType<typeof vi.fn> };
}

function post(user_id: string, ip: string): Request {
  return new Request("http://test/v1/devices/lookup", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": ip },
    body: JSON.stringify({ user_id }),
  });
}

async function lookup(
  env: Env,
  user_id: string,
  ip: string,
): Promise<{ response: Response; text: string; parsed: DeviceLookupBody }> {
  const response = await handleDevicesLookup(post(user_id, ip), env);
  const text = await response.text();
  return { response, text, parsed: JSON.parse(text) as DeviceLookupBody };
}

describe("task 4803 device lookup privacy", () => {
  it("has removed the account-carrying GET device route from dispatch", () => {
    const source = readFileSync(new URL("../src/index.ts", import.meta.url), "utf8");
    expect(source).not.toMatch(/\/v1\/devices\/\(\[\^\/\]\+\)/);
    expect(source).not.toMatch(/devicesUserId/);
    expect(source).toContain('path === "/v1/devices/lookup"');
    console.log("task4803_source_get_device_route_matches=0 post_route=/v1/devices/lookup");
  });

  it("POSTs real, deleted and made-up accounts at status 200 and one byte size", async () => {
    const real = "task-4803-live";
    const deleted = "task-4803-deleted";
    const madeUp = "task-4803-made-up";
    const env = envFor({
      liveUsers: new Set([real]),
      rowsByUser: new Map([
        [
          real,
          [
            { device_id: "desktop", prekey_bundle: "bundle-desktop" },
            { device_id: "phone", prekey_bundle: "bundle-phone" },
          ],
        ],
        [deleted, [{ device_id: "stale-phone", prekey_bundle: "stale-bundle" }]],
      ]),
    });

    const realLookup = await lookup(env, real, "198.51.100.12");
    const deletedLookup = await lookup(env, deleted, "198.51.100.13");
    const madeUpLookup = await lookup(env, madeUp, "198.51.100.14");

    expect(realLookup.response.status).toBe(200);
    expect(deletedLookup.response.status).toBe(200);
    expect(madeUpLookup.response.status).toBe(200);
    expect(realLookup.parsed.devices).toHaveLength(2);
    expect(deletedLookup.parsed.devices).toHaveLength(0);
    expect(madeUpLookup.parsed.devices).toHaveLength(0);
    expect([
      bytes(realLookup.text),
      bytes(deletedLookup.text),
      bytes(madeUpLookup.text),
    ]).toEqual([
      DEVICE_LOOKUP_RESPONSE_BYTES,
      DEVICE_LOOKUP_RESPONSE_BYTES,
      DEVICE_LOOKUP_RESPONSE_BYTES,
    ]);
    console.log(
      `task4803_post_statuses=${realLookup.response.status},${deletedLookup.response.status},${madeUpLookup.response.status} `
      + `post_lengths=${bytes(realLookup.text)},${bytes(deletedLookup.text)},${bytes(madeUpLookup.text)} `
      + `device_counts=${realLookup.parsed.devices.length},${deletedLookup.parsed.devices.length},${madeUpLookup.parsed.devices.length}`,
    );
  });

  it("pads twenty accounts holding 1, 2, 3 and 4 devices to one byte size", async () => {
    const rowsByUser = new Map<string, DeviceRow[]>();
    const liveUsers = new Set<string>();
    for (let account = 0; account < 20; account++) {
      const user = `task-4803-fanout-${account}`;
      const count = (account % 4) + 1;
      liveUsers.add(user);
      rowsByUser.set(
        user,
        Array.from({ length: count }, (_, device) => ({
          device_id: `device-${device + 1}`,
          prekey_bundle: `bundle-${account}-${device + 1}`,
        })),
      );
    }
    const env = envFor({ liveUsers, rowsByUser });
    const lengths = new Set<number>();
    const counts = new Set<number>();

    for (let account = 0; account < 20; account++) {
      const user = `task-4803-fanout-${account}`;
      const result = await lookup(env, user, `198.51.100.${20 + account}`);
      expect(result.response.status).toBe(200);
      counts.add(result.parsed.devices.length);
      lengths.add(bytes(result.text));
    }

    expect([...counts].sort()).toEqual([1, 2, 3, 4]);
    expect([...lengths]).toEqual([DEVICE_LOOKUP_RESPONSE_BYTES]);
    console.log(
      `task4803_twenty_accounts=20 device_counts=${[...counts].sort().join(",")} `
      + `unique_byte_counts=${lengths.size} bytes=${[...lengths].join(",")}`,
    );
  });

  it("returns 429 on the 61st call from one address when the native limiter denies", async () => {
    let endpointCalls = 0;
    const endpointLimit = vi.fn(async ({ key }: { key: string }) => {
      expect(key).toBe("devices-lookup-ip:198.51.100.61");
      endpointCalls += 1;
      return { success: endpointCalls <= 60 };
    });
    const env = envFor({
      endpointLimit,
      liveUsers: new Set(["task-4803-rate"]),
      rowsByUser: new Map([["task-4803-rate", []]]),
    });

    let last: Response | null = null;
    for (let i = 0; i < 61; i++) {
      last = await handleDevicesLookup(post("task-4803-rate", "198.51.100.61"), env);
    }

    expect(last?.status).toBe(429);
    const body = await last?.text();
    expect(JSON.parse(body ?? "")).toEqual({ error: "rate_limited" });
    expect(endpointLimit).toHaveBeenCalledTimes(61);
    expect(env.prepare).toHaveBeenCalledTimes(60);
    console.log(
      `task4803_rate_attempts=61 final_status=${last?.status} body=${body} `
      + `endpoint_limit_calls=${endpointLimit.mock.calls.length} db_prepares=${env.prepare.mock.calls.length}`,
    );
  });
});
