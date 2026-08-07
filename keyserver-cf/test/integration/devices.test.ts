import { SELF, env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import { canonicalUnregisterBytes } from "../../src/lib/canonical.js";
import {
  DEVICE_LOOKUP_RESPONSE_BYTES,
  handleDevicesLookup,
} from "../../src/endpoints/devices.js";
import type { Env } from "../../src/env.js";
import { registerTestUser, signEd25519 } from "./helpers.js";

type DeviceLookupBody = {
  devices: { device_id: string; prekey_bundle: string }[];
  pad: string;
};

let sequence = 0;
const userId = (label: string) => `devices-${label}-${Date.now()}-${sequence++}`;

function bytes(value: string): number {
  return new TextEncoder().encode(value).length;
}

async function lookup(user_id: string, ip: string): Promise<Response> {
  return SELF.fetch("http://test/v1/devices/lookup", {
    method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": ip },
    body: JSON.stringify({ user_id }),
  });
}

async function insertDevices(user_id: string, count: number): Promise<void> {
  const db = (env as unknown as { DB: D1Database }).DB;
  const now = new Date().toISOString();
  await db.batch(
    Array.from({ length: count }, (_, index) =>
      db.prepare(
        "INSERT INTO device_roster (user_id, device_id, prekey_bundle, registered_at) VALUES (?, ?, ?, ?)",
      ).bind(
        user_id,
        `device-${String(index + 1).padStart(2, "0")}`,
        `bundle-${user_id}-${index + 1}`,
        now,
      ),
    ),
  );
}

async function unregister(
  user_id: string,
  pair: { signingKey: CryptoKey },
): Promise<Response> {
  const timestamp_ms = Date.now();
  const signature_b64 = await signEd25519(
    pair.signingKey,
    canonicalUnregisterBytes({ user_id, timestamp_ms }),
  );
  return SELF.fetch(`http://test/v1/pubkeys/${encodeURIComponent(user_id)}`, {
    method: "DELETE",
    headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.62" },
    body: JSON.stringify({ signature_b64, timestamp_ms }),
  });
}

describe("device roster privacy", () => {
  it("removes the account-carrying GET route and serves the POST body route", async () => {
    const user = userId("post-route");
    await registerTestUser(SELF, user);
    await insertDevices(user, 2);

    const viaGet = await SELF.fetch(`http://test/v1/devices/${encodeURIComponent(user)}`, {
      headers: { "cf-connecting-ip": "203.0.113.63" },
    });
    const missingViaGet = await SELF.fetch(`http://test/v1/devices/${crypto.randomUUID()}`, {
      headers: { "cf-connecting-ip": "203.0.113.64" },
    });
    expect(viaGet.status).toBe(404);
    expect(missingViaGet.status).toBe(404);

    const response = await lookup(user, "203.0.113.65");
    expect(response.status).toBe(200);
    const text = await response.text();
    expect(bytes(text)).toBe(DEVICE_LOOKUP_RESPONSE_BYTES);
    const body = JSON.parse(text) as DeviceLookupBody;
    expect(body.devices).toEqual([
      { device_id: "device-01", prekey_bundle: `bundle-${user}-1` },
      { device_id: "device-02", prekey_bundle: `bundle-${user}-2` },
    ]);
  });

  it("uses one fixed byte count for live, deleted and made-up accounts", async () => {
    const live = userId("live");
    const deleted = userId("deleted");
    const madeUp = userId("made-up");
    const deletedPair = await registerTestUser(SELF, deleted);
    await registerTestUser(SELF, live);
    await insertDevices(live, 3);
    await insertDevices(deleted, 2);
    const deleteResponse = await unregister(deleted, deletedPair);
    expect(deleteResponse.status, await deleteResponse.text()).toBe(200);

    const liveResponse = await lookup(live, "203.0.113.66");
    const deletedResponse = await lookup(deleted, "203.0.113.67");
    const madeUpResponse = await lookup(madeUp, "203.0.113.68");

    expect(liveResponse.status).toBe(200);
    expect(deletedResponse.status).toBe(200);
    expect(madeUpResponse.status).toBe(200);

    const lengths = [
      bytes(await liveResponse.text()),
      bytes(await deletedResponse.text()),
      bytes(await madeUpResponse.text()),
    ];
    expect(lengths).toEqual([
      DEVICE_LOOKUP_RESPONSE_BYTES,
      DEVICE_LOOKUP_RESPONSE_BYTES,
      DEVICE_LOOKUP_RESPONSE_BYTES,
    ]);
  });

  it("pads twenty 1, 2, 3 and 4 device accounts to the same size", async () => {
    const lengths = new Set<number>();
    const counts = new Set<number>();

    for (let i = 0; i < 20; i++) {
      const count = (i % 4) + 1;
      const user = userId(`fanout-${i}`);
      await registerTestUser(SELF, user);
      await insertDevices(user, count);
      const response = await lookup(user, `203.0.113.${80 + i}`);
      expect(response.status).toBe(200);
      const text = await response.text();
      lengths.add(bytes(text));
      counts.add((JSON.parse(text) as DeviceLookupBody).devices.length);
    }

    expect([...counts].sort()).toEqual([1, 2, 3, 4]);
    expect([...lengths]).toEqual([DEVICE_LOOKUP_RESPONSE_BYTES]);
  });

  it("rate-limits by caller address before touching the roster", async () => {
    let calls = 0;
    const limit = vi.fn(async ({ key }: { key: string }) => {
      expect(key).toBe("devices-lookup-ip:198.51.100.61");
      calls += 1;
      return { success: calls <= 60 };
    });
    const all = vi.fn(async () => ({ results: [] }));
    const bind = vi.fn(() => ({ all }));
    const prepare = vi.fn(() => ({ bind }));
    const testEnv = {
      DB: { prepare },
      RATE_LIMIT_120: { limit } as unknown as RateLimit,
    } as unknown as Env;

    let last: Response | null = null;
    for (let i = 0; i < 61; i++) {
      last = await handleDevicesLookup(
        new Request("http://test/v1/devices/lookup", {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "cf-connecting-ip": "198.51.100.61",
          },
          body: JSON.stringify({ user_id: "rate-limited-user" }),
        }),
        testEnv,
      );
    }

    expect(last?.status).toBe(429);
    await expect(last?.json()).resolves.toEqual({ error: "rate_limited" });
    expect(limit).toHaveBeenCalledTimes(61);
    expect(prepare).toHaveBeenCalledTimes(60);
  });
});
