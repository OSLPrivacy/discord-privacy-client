import { describe, expect, it } from "vitest";

import { handleControlInboxGet } from "../src/endpoints/control-inbox.js";
import { handleDevices } from "../src/endpoints/devices.js";
import { canonicalControlInboxGetBytes } from "../src/lib/canonical.js";
import {
  generateEd25519Pair,
  signEd25519,
} from "../test/integration/helpers.js";

type UserRow = { user_id: string; ik_ed25519_pub: string };
type DeviceRow = { user_id: string; device_id: string; prekey_bundle: string };
type InboxRow = {
  id: Uint8Array;
  recipient_id: string;
  sender_id: string;
  scope_id: string;
  bundle: Uint8Array;
  created_at: number;
  kind: string;
  delivery_status: "live";
  expires_at: number;
};

class FakeStatement {
  private binds: unknown[] = [];

  constructor(
    private readonly db: FakeD1,
    private readonly sql: string,
  ) {}

  bind(...args: unknown[]): FakeStatement {
    this.binds = args;
    return this;
  }

  async first<T>(): Promise<T | null> {
    if (this.sql.includes("FROM worker_schema_capabilities")) {
      return { version: 1 } as T;
    }
    if (this.sql.includes("FROM users")) {
      const user = this.db.users.get(String(this.binds[0]));
      return (user ?? null) as T | null;
    }
    if (this.sql.includes("FROM device_roster") && this.sql.includes("SELECT 1 AS present")) {
      const [userId, deviceId] = this.binds.map(String);
      const present = this.db.devices.some(
        (device) => device.user_id === userId && device.device_id === deviceId,
      );
      return (present ? { present: 1 } : null) as T | null;
    }
    throw new Error(`unexpected first() query: ${this.sql}`);
  }

  async all<T>(): Promise<{ results: T[] }> {
    if (this.sql.includes("FROM device_roster")) {
      const userId = String(this.binds[0]);
      const results = this.db.devices
        .filter((device) => device.user_id === userId)
        .sort((a, b) => a.device_id.localeCompare(b.device_id))
        .map((device) => ({
          device_id: device.device_id,
          prekey_bundle: device.prekey_bundle,
        }));
      return { results: results as T[] };
    }
    if (
      this.sql.includes("FROM control_inbox") &&
      this.sql.includes("recipient_id = ?") &&
      this.sql.includes("ORDER BY created_at ASC")
    ) {
      const [recipientId, expiresAt] = this.binds;
      const results = this.db.inbox
        .filter(
          (row) =>
            row.recipient_id === recipientId &&
            row.delivery_status === "live" &&
            row.expires_at >= Number(expiresAt),
        )
        .sort((a, b) => a.created_at - b.created_at)
        .slice(0, Number(this.binds[2]))
        .map((row) => ({
          id: row.id,
          sender_id: row.sender_id,
          scope_id: row.scope_id,
          bundle: row.bundle,
          created_at: row.created_at,
          kind: row.kind,
        }));
      return { results: results as T[] };
    }
    if (
      this.sql.includes("FROM control_inbox") ||
      this.sql.includes("FROM control_inbox_requests")
    ) {
      return { results: [] };
    }
    throw new Error(`unexpected all() query: ${this.sql}`);
  }
}

class FakeD1 {
  readonly users = new Map<string, UserRow>();
  readonly devices: DeviceRow[] = [];
  readonly inbox: InboxRow[] = [];

  prepare(sql: string): FakeStatement {
    return new FakeStatement(this, sql);
  }
}

async function signedDeviceDrain(
  db: FakeD1,
  userId: string,
  deviceId: string,
  signingKey: CryptoKey,
): Promise<{ items: unknown[] }> {
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
  const response = await handleControlInboxGet(
    new Request(
      `http://test/v1/control-inbox/${encodeURIComponent(userId)}?ts=${ts}&sig=${encodeURIComponent(sig)}&device_id=${encodeURIComponent(deviceId)}`,
    ),
    { DB: db } as never,
    userId,
  );
  expect(response.status).toBe(200);
  return await response.json();
}

describe("TASK4809 keyserver device cutoff", () => {
  it("returns two active devices and zero new mail rows to the removed device", async () => {
    const db = new FakeD1();
    const userId = "task-4809-account";
    const senderId = "task-4809-sender";
    const account = await generateEd25519Pair();
    const sender = await generateEd25519Pair();
    db.users.set(userId, { user_id: userId, ik_ed25519_pub: account.publicKeyB64 });
    db.users.set(senderId, { user_id: senderId, ik_ed25519_pub: sender.publicKeyB64 });
    db.devices.push(
      { user_id: userId, device_id: "device-1", prekey_bundle: "bundle-device-1" },
      { user_id: userId, device_id: "device-2", prekey_bundle: "bundle-device-2" },
      { user_id: userId, device_id: "device-3", prekey_bundle: "bundle-device-3" },
    );

    if (process.env.OSL_TASK4809_KEEP_REMOVED_DEVICE !== "1") {
      db.devices.splice(
        db.devices.findIndex((device) => device.device_id === "device-2"),
        1,
      );
    }

    const now = Math.floor(Date.now() / 1000);
    for (let i = 0; i < 10; i += 1) {
      db.inbox.push({
        id: Uint8Array.of(i + 1, ...new Uint8Array(15)),
        recipient_id: userId,
        sender_id: senderId,
        scope_id: "HERON-4809",
        bundle: new TextEncoder().encode("HERON-4809 new sync"),
        created_at: now + i,
        expires_at: now + 3600,
        delivery_status: "live",
        kind: "",
      });
    }

    const devicesResponse = await handleDevices({ DB: db } as never, userId);
    expect(devicesResponse.status).toBe(200);
    const published = await devicesResponse.json() as DeviceRow[];
    const publishedIds = published.map((device) => device.device_id);
    const removedDeviceSlots = publishedIds.filter((id) => id === "device-2").length;

    const fetchCounts: number[] = [];
    for (let i = 0; i < 10; i += 1) {
      const drained = await signedDeviceDrain(db, userId, "device-2", account.signingKey);
      fetchCounts.push(drained.items.length);
      console.log(
        `TASK4809_KEYSERVER fetch_try=${i + 1} removed_device_new_mail_rows=${drained.items.length}`,
      );
    }

    for (let i = 0; i < 10; i += 1) {
      const slots = published.length;
      console.log(
        `TASK4809_KEYSERVER message=HERON-4809-${String(i + 1).padStart(2, "0")} recipient_slots=${slots} removed_device_slots=${removedDeviceSlots}`,
      );
      expect(
        slots,
        `TASK4809_BREAK message=HERON-4809-${String(i + 1).padStart(2, "0")} slots=${slots} keyserver_devices=${published.length}`,
      ).toBe(2);
      expect(removedDeviceSlots).toBe(0);
    }

    const totalFetched = fetchCounts.reduce((sum, count) => sum + count, 0);
    console.log(
      `TASK4809_KEYSERVER devices_returned=${published.length} device_ids=${publishedIds.join(",")} removed_device_new_mail_fetch_rows_total=${totalFetched} fetch_tries=${fetchCounts.length} fetch_counts=${fetchCounts.join(",")}`,
    );
    expect(publishedIds).toEqual(["device-1", "device-3"]);
    expect(published).toHaveLength(2);
    expect(fetchCounts).toHaveLength(10);
    expect(fetchCounts.every((count) => count === 0)).toBe(true);
    expect(totalFetched).toBe(0);
  });
});
