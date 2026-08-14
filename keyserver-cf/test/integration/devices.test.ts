import { SELF, env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import {
  canonicalControlInboxGetBytes,
  canonicalUnregisterBytes,
} from "../../src/lib/canonical.js";
import {
  DEVICE_LOOKUP_RESPONSE_BYTES,
  handleDevicesLookup,
} from "../../src/endpoints/devices.js";
import type { Env } from "../../src/env.js";
import {
  base64Encode,
  registerTestUser,
  signEd25519,
} from "./helpers.js";

async function deviceDrain(
  userId: string,
  deviceId: string,
  signingKey: CryptoKey,
): Promise<{ items: unknown[]; filtered_device_id: string; device_delivery: string }> {
  const ts = Date.now();
  const sig = await signEd25519(
    signingKey,
    canonicalControlInboxGetBytes({
      user_id: userId,
      timestamp_ms: ts,
      sender_id: null,
      device_id: deviceId,
    }),
  );
  const response = await SELF.fetch(
    `http://test/v1/control-inbox/${encodeURIComponent(userId)}?ts=${ts}&sig=${encodeURIComponent(sig)}&device_id=${encodeURIComponent(deviceId)}`,
  );
  expect(response.status).toBe(200);
  return await response.json();
}

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

  it("TASK4809 removing one of three devices cuts it off from published delivery and new sync", async () => {
    const user = `devices-4809-${crypto.randomUUID()}`;
    const account = await registerTestUser(SELF, user);
    const sender = `sender-4809-${crypto.randomUUID()}`;
    await registerTestUser(SELF, sender);
    const removedDevice = "device-2";
    const db = (env as unknown as { DB: D1Database }).DB;
    const nowIso = new Date().toISOString();

    await db.batch([
      db.prepare("INSERT INTO device_roster (user_id, device_id, prekey_bundle, registered_at) VALUES (?, ?, ?, ?)").bind(user, "device-1", "bundle-device-1", nowIso),
      db.prepare("INSERT INTO device_roster (user_id, device_id, prekey_bundle, registered_at) VALUES (?, ?, ?, ?)").bind(user, removedDevice, "bundle-device-2", nowIso),
      db.prepare("INSERT INTO device_roster (user_id, device_id, prekey_bundle, registered_at) VALUES (?, ?, ?, ?)").bind(user, "device-3", "bundle-device-3", nowIso),
    ]);

    if (process.env.OSL_TASK4809_KEEP_REMOVED_DEVICE !== "1") {
      await db.prepare("DELETE FROM device_roster WHERE user_id = ? AND device_id = ?")
        .bind(user, removedDevice)
        .run();
    }

    const publishedResponse = await lookup(user, "203.0.113.69");
    expect(publishedResponse.status).toBe(200);
    const published = (await publishedResponse.json() as DeviceLookupBody).devices;
    const postRemovalSlots = published.map((device) => device.device_id);
    const removedDeviceSlots = postRemovalSlots.filter((device) => device === removedDevice).length;

    await db.prepare(
      `WITH RECURSIVE cnt(x) AS (
         VALUES(1) UNION ALL SELECT x + 1 FROM cnt WHERE x < 10
       )
       INSERT INTO control_inbox
         (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
       SELECT randomblob(16), ?, ?, 'HERON-4809', ?, ?, ? + x FROM cnt`,
    )
      .bind(user, sender, base64Encode(new TextEncoder().encode("HERON-4809 new sync")), Math.floor(Date.now() / 1000) + 3600, Math.floor(Date.now() / 1000))
      .run();

    const fetchCounts: number[] = [];
    for (let i = 0; i < 10; i += 1) {
      const drained = await deviceDrain(user, removedDevice, account.signingKey);
      fetchCounts.push(drained.items.length);
    }
    const totalFetchedByRemovedDevice = fetchCounts.reduce((sum, count) => sum + count, 0);

    for (let i = 0; i < 10; i += 1) {
      const slots = postRemovalSlots.length;
      console.log(
        `TASK4809_KEYSERVER message=HERON-4809-${String(i + 1).padStart(2, "0")} recipient_slots=${slots} removed_device_slots=${removedDeviceSlots}`,
      );
      expect(
        slots,
        `TASK4809_BREAK message=HERON-4809-${String(i + 1).padStart(2, "0")} slots=${slots} keyserver_devices=${published.length}`,
      ).toBe(2);
      expect(removedDeviceSlots).toBe(0);
    }
    console.log(
      `TASK4809_KEYSERVER devices_returned=${published.length} removed_device=${removedDevice} removed_device_new_mail_fetch_rows_total=${totalFetchedByRemovedDevice} fetch_tries=${fetchCounts.length} fetch_counts=${fetchCounts.join(",")}`,
    );

    expect(published.map((device) => device.device_id)).toEqual(["device-1", "device-3"]);
    expect(published).toHaveLength(2);
    expect(fetchCounts).toHaveLength(10);
    expect(fetchCounts.every((count) => count === 0)).toBe(true);
    expect(totalFetchedByRemovedDevice).toBe(0);
  });
});
