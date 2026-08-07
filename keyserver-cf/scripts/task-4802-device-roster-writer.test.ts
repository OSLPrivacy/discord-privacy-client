import { DatabaseSync } from "node:sqlite";
import { afterEach, describe, expect, it } from "vitest";

import { handleDevicesPost } from "../src/endpoints/devices.js";
import type { Env } from "../src/env.js";
import { canonicalDeviceListBytes } from "../src/lib/canonical.js";

const STUB_X25519_PUB_B64 = base64Encode(new Uint8Array(32).fill(0x11));
const STUB_ED25519_PUB_B64 = base64Encode(new Uint8Array(32).fill(0x22));
const STUB_MLKEM_PUB_B64 = base64Encode(new Uint8Array(1184).fill(0x33));
const STUB_RATCHET_PUB_B64 = base64Encode(new Uint8Array(32).fill(0x44));
const STUB_SIGNATURE_B64 = base64Encode(new Uint8Array(64).fill(0x55));

interface TestDevice {
  device_id: string;
  prekey_bundle: string;
}

class FakeD1PreparedStatement {
  constructor(
    private readonly db: DatabaseSync,
    readonly sql: string,
    readonly params: unknown[] = [],
  ) {}

  bind(...params: unknown[]): FakeD1PreparedStatement {
    return new FakeD1PreparedStatement(this.db, this.sql, params);
  }

  async first<T>(): Promise<T | null> {
    return this.db.prepare(this.sql).get(...this.params) as T | null;
  }

  async all<T>(): Promise<{ results: T[] }> {
    return { results: this.db.prepare(this.sql).all(...this.params) as T[] };
  }

  async run(): Promise<{ meta: { changes: number } }> {
    const result = this.db.prepare(this.sql).run(...this.params);
    return { meta: { changes: result.changes } };
  }
}

class FakeD1Database {
  constructor(private readonly db: DatabaseSync) {}

  prepare(sql: string): FakeD1PreparedStatement {
    return new FakeD1PreparedStatement(this.db, sql);
  }

  async batch(statements: FakeD1PreparedStatement[]): Promise<Array<{ meta: { changes: number } }>> {
    this.db.exec("BEGIN");
    try {
      const results = statements.map((statement) => {
        const result = this.db.prepare(statement.sql).run(...statement.params);
        return { meta: { changes: result.changes } };
      });
      this.db.exec("COMMIT");
      return results;
    } catch (error) {
      this.db.exec("ROLLBACK");
      throw error;
    }
  }
}

function base64Encode(bytes: Uint8Array): string {
  let output = "";
  for (const byte of bytes) output += String.fromCharCode(byte);
  return btoa(output);
}

function base64Decode(value: string): Uint8Array {
  const binary = atob(value);
  const output = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    output[i] = binary.charCodeAt(i);
  }
  return output;
}

async function generateEd25519Pair(): Promise<{
  publicKeyB64: string;
  signingKey: CryptoKey;
}> {
  const pair = await crypto.subtle.generateKey(
    { name: "Ed25519" },
    true,
    ["sign", "verify"],
  ) as CryptoKeyPair;
  const rawPub = await crypto.subtle.exportKey("raw", pair.publicKey);
  return {
    publicKeyB64: base64Encode(new Uint8Array(rawPub)),
    signingKey: pair.privateKey,
  };
}

async function signEd25519(
  signingKey: CryptoKey,
  message: Uint8Array,
): Promise<string> {
  const signature = await crypto.subtle.sign(
    { name: "Ed25519" },
    signingKey,
    message,
  );
  return base64Encode(new Uint8Array(signature));
}

async function signedDeviceListBody(
  userId: string,
  rootSigningKey: CryptoKey,
  version: number,
  devices: TestDevice[],
): Promise<Record<string, unknown>> {
  return {
    user_id: userId,
    version,
    devices,
    root_signature_b64: await signEd25519(
      rootSigningKey,
      canonicalDeviceListBytes({ user_id: userId, version, devices }),
    ),
  };
}

function flipOneSignatureByte(body: Record<string, unknown>): Record<string, unknown> {
  const signature = base64Decode(body.root_signature_b64 as string);
  signature[0] ^= 0x01;
  return { ...body, root_signature_b64: base64Encode(signature) };
}

function setupDb(rootPublicKeyB64: string, userId: string): {
  db: DatabaseSync;
  env: Env;
} {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON;");
  db.exec(`
    CREATE TABLE users (
      user_id TEXT PRIMARY KEY,
      ik_x25519_pub TEXT NOT NULL,
      ik_ed25519_pub TEXT NOT NULL,
      ik_mlkem768_pub TEXT NOT NULL,
      ik_x25519_signature TEXT NOT NULL,
      registered_at TEXT NOT NULL,
      last_rotated_at TEXT,
      ik_ratchet_initial_pub TEXT,
      identity_lookup_enabled INTEGER NOT NULL DEFAULT 1,
      rn_capabilities INTEGER NOT NULL DEFAULT 0,
      identity_scheme INTEGER NOT NULL DEFAULT 1,
      ik_root_ed25519_pub TEXT,
      identity_revision INTEGER NOT NULL DEFAULT 1,
      identity_bundle_proof_sig TEXT
    );
    CREATE TABLE device_roster (
      user_id TEXT NOT NULL,
      device_id TEXT NOT NULL,
      prekey_bundle TEXT NOT NULL,
      registered_at TEXT NOT NULL,
      PRIMARY KEY (user_id, device_id),
      FOREIGN KEY (user_id) REFERENCES users (user_id)
    ) WITHOUT ROWID;
    CREATE TABLE device_roster_versions (
      user_id TEXT PRIMARY KEY,
      version INTEGER NOT NULL CHECK (version >= 1),
      updated_at TEXT NOT NULL,
      FOREIGN KEY (user_id) REFERENCES users (user_id)
    ) WITHOUT ROWID;
  `);
  db.prepare(
    `INSERT INTO users (
      user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
      ik_x25519_signature, registered_at, last_rotated_at,
      ik_ratchet_initial_pub, identity_lookup_enabled, rn_capabilities,
      identity_scheme, ik_root_ed25519_pub, identity_revision,
      identity_bundle_proof_sig
    ) VALUES (?, ?, ?, ?, ?, ?, NULL, ?, 1, 1, 1, ?, 1, ?)`,
  ).run(
    userId,
    STUB_X25519_PUB_B64,
    STUB_ED25519_PUB_B64,
    STUB_MLKEM_PUB_B64,
    STUB_SIGNATURE_B64,
    new Date().toISOString(),
    STUB_RATCHET_PUB_B64,
    rootPublicKeyB64,
    STUB_SIGNATURE_B64,
  );
  return {
    db,
    env: { DB: new FakeD1Database(db) } as unknown as Env,
  };
}

async function publish(
  env: Env,
  body: Record<string, unknown>,
): Promise<Response> {
  return await handleDevicesPost(
    new Request("http://test/v1/devices", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    }),
    env,
  );
}

function rosterCount(db: DatabaseSync, userId: string): number {
  return db.prepare(
    "SELECT COUNT(*) AS count FROM device_roster WHERE user_id = ?",
  ).get(userId)!.count as number;
}

function deviceCount(db: DatabaseSync, userId: string, deviceId: string): number {
  return db.prepare(
    "SELECT COUNT(*) AS count FROM device_roster WHERE user_id = ? AND device_id = ?",
  ).get(userId, deviceId)!.count as number;
}

function rosterVersion(db: DatabaseSync, userId: string): number | null {
  const row = db.prepare(
    "SELECT version FROM device_roster_versions WHERE user_id = ?",
  ).get(userId) as { version: number } | undefined;
  return row?.version ?? null;
}

describe("TASK 4802 device roster writer", () => {
  let db: DatabaseSync | null = null;

  afterEach(() => {
    db?.close();
    db = null;
  });

  it("stores only higher root-signed whole-list versions and replaces removed devices", async () => {
    const userId = `task4802-${crypto.randomUUID()}`;
    const root = await generateEd25519Pair();
    const setup = setupDb(root.publicKeyB64, userId);
    db = setup.db;

    const threeDevices = [
      { device_id: "phone", prekey_bundle: "bundle-phone-v5" },
      { device_id: "desktop", prekey_bundle: "bundle-desktop-v5" },
      { device_id: "tablet", prekey_bundle: "bundle-tablet-v5" },
    ];
    const version5Body = await signedDeviceListBody(
      userId,
      root.signingKey,
      5,
      threeDevices,
    );
    expect((await publish(setup.env, version5Body)).status).toBe(200);
    const version5Rows = rosterCount(db, userId);
    console.log(`TASK4802 version=5 stored_rows=${version5Rows}`);
    expect(version5Rows).toBe(3);

    const duplicate = await publish(setup.env, version5Body);
    const duplicateJson = await duplicate.json() as { error: string };
    const duplicateRows = rosterCount(db, userId);
    console.log(
      `TASK4802 duplicate_error="${duplicateJson.error}" stored_rows=${duplicateRows}`,
    );
    expect(duplicate.status).toBe(409);
    expect(duplicateJson.error).toBe("device list version must go up");
    expect(duplicateRows).toBe(3);

    const twoDevices = [
      { device_id: "phone", prekey_bundle: "bundle-phone-v6" },
      { device_id: "desktop", prekey_bundle: "bundle-desktop-v6" },
    ];
    expect(
      (await publish(
        setup.env,
        await signedDeviceListBody(userId, root.signingKey, 6, twoDevices),
      )).status,
    ).toBe(200);
    const version6Rows = rosterCount(db, userId);
    const removedRows = deviceCount(db, userId, "tablet");
    console.log(
      `TASK4802 version=6 stored_rows=${version6Rows} removed_device_rows=${removedRows}`,
    );
    expect(version6Rows).toBe(2);
    expect(removedRows).toBe(0);

    const beforeBadRows = rosterCount(db, userId);
    const beforeBadVersion = rosterVersion(db, userId);
    const badSignature = await publish(
      setup.env,
      flipOneSignatureByte(
        await signedDeviceListBody(userId, root.signingKey, 7, threeDevices),
      ),
    );
    const afterBadRows = rosterCount(db, userId);
    const afterBadVersion = rosterVersion(db, userId);
    const changedRows =
      beforeBadRows === afterBadRows && beforeBadVersion === afterBadVersion
        ? 0
        : 1;
    console.log(
      `TASK4802 bad_signature_status=${badSignature.status} changed_rows=${changedRows} stored_rows=${afterBadRows} stored_version=${afterBadVersion}`,
    );
    expect(badSignature.status).toBe(401);
    expect(changedRows).toBe(0);
  });
});
