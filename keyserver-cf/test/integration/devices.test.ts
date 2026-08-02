import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { registerTestUser } from "./helpers.js";

describe("device roster", () => {
  it("T6-T17 returns only distinct registered device bundles", async () => {
    const user = `devices-${crypto.randomUUID()}`;
    await registerTestUser(SELF, user);
    const db = (env as unknown as { DB: D1Database }).DB;
    await db.batch([
      db.prepare("INSERT INTO device_roster (user_id, device_id, prekey_bundle, registered_at) VALUES (?, ?, ?, ?)").bind(user, "phone", "bundle-phone", new Date().toISOString()),
      db.prepare("INSERT INTO device_roster (user_id, device_id, prekey_bundle, registered_at) VALUES (?, ?, ?, ?)").bind(user, "desktop", "bundle-desktop", new Date().toISOString()),
    ]);
    const body = await (await SELF.fetch(`http://test/v1/devices/${user}`)).json<{ device_id: string; prekey_bundle: string }[]>();
    expect(body).toEqual([{ device_id: "desktop", prekey_bundle: "bundle-desktop" }, { device_id: "phone", prekey_bundle: "bundle-phone" }]);
    expect(new Set(body.map((row) => row.prekey_bundle)).size).toBe(2);
    expect(await (await SELF.fetch(`http://test/v1/devices/missing-${crypto.randomUUID()}`)).json()).toEqual([]);
  });
});
