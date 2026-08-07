import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { DELETE_GRANT_RECORD, encodeDeleteGrant } from "../src/lib/delete-grant.js";
import { sha256Hex } from "../src/lib/digest.js";
import { DELETE_GRANT_RECORD } from "../src/lib/delete-grant.js";
import { sha256Hex } from "../src/lib/digest.js";
import { senderOwnCopyBurnCommand } from "../src/lib/sender-own-copy-burn-command.js";
import { d1All, d1Count, d1First, d1Run } from "./helpers/workerd.js";

const ORIGIN = "https://cipher.test";
const ID = "04050000000000000000000000000000";
const FETCH_CAP = "f4050000000000000000000000000000";
const ACK_CAP = "a4050000000000000000000000000000";
const MANAGE_CAP = "d4050000000000000000000000000000";
const MESSAGE = "message:0405";
const OWNER = "identity:sender-0405";
const BURN_SCOPE = "discord:9000000000000405:direct_message:stored-copy";

async function seedProtectedStoredCopy() {
  const now = Math.floor(Date.now() / 1000);
  const fetchDigest = await sha256Hex(FETCH_CAP);
interface ProtectedStoredCopyFixture {
  id: string;
  fetchCap: string;
  ackCap: string;
  manageCap: string;
  message: string;
  owner: string;
  burnScope: string;
  deliveryTag?: string;
  payload?: Uint8Array;
}

async function seedProtectedStoredCopyFixture(fixture: ProtectedStoredCopyFixture) {
  const now = Math.floor(Date.now() / 1000);
  const fetchDigest = await sha256Hex(fixture.fetchCap);
  await d1Run(
    `INSERT INTO blob_capability_index (
       blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
       manage_digest_sha256_hex, object_class, pool, delivery_tag,
       size_bytes, expires_at, created_at,
       delete_message, delete_owner, burn_scope
     ) VALUES (?, ?, ?, ?, 'single-ack', 'undelivered', ?, ?, ?, ?, ?, ?, ?)`,
    ID,
    fetchDigest,
    await sha256Hex(ACK_CAP),
    await sha256Hex(MANAGE_CAP),
    "5".repeat(32),
    1,
    now + 3600,
    now,
    MESSAGE,
    OWNER,
    BURN_SCOPE,
  );
  await env.PAYLOADS.put(fetchDigest, new Uint8Array([7]));
  return fetchDigest;
}

function senderDeleteGrant(): string {
  return encodeDeleteGrant({
    type: DELETE_GRANT_RECORD,
    message: MESSAGE,
    owner: OWNER,
    scope: BURN_SCOPE,
    grant: MANAGE_CAP,
       delete_grant_message, delete_grant_owner, burn_scope
     ) VALUES (?, ?, ?, ?, 'single-ack', 'undelivered', ?, ?, ?, ?, ?, ?, ?)`,
    fixture.id,
    fetchDigest,
    await sha256Hex(fixture.ackCap),
    await sha256Hex(fixture.manageCap),
    fixture.deliveryTag ?? "5".repeat(32),
    1,
    now + 3600,
    now,
    fixture.message,
    fixture.owner,
    fixture.burnScope,
  );
  await env.PAYLOADS.put(fetchDigest, fixture.payload ?? new Uint8Array([7]));
  return fetchDigest;
}

async function seedProtectedStoredCopy() {
  return seedProtectedStoredCopyFixture({
    id: ID,
    fetchCap: FETCH_CAP,
    ackCap: ACK_CAP,
    manageCap: MANAGE_CAP,
    message: MESSAGE,
    owner: OWNER,
    burnScope: BURN_SCOPE,
  });
}

function senderDeleteGrant(): string {
  return JSON.stringify({
    record: DELETE_GRANT_RECORD,
    message: MESSAGE,
    owner: OWNER,
    scope: BURN_SCOPE,
  });
}

async function storedCopySnapshot() {
  return d1First<Record<string, unknown>>(
    `SELECT blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
            manage_digest_sha256_hex, object_class, pool, delivery_tag,
            size_bytes, expires_at, created_at,
            delete_message, delete_owner, burn_scope
            delete_grant_message, delete_grant_owner, burn_scope
       FROM blob_capability_index
      WHERE blob_id = ?`,
    ID,
  );
}

describe("TASK 0405 server delete-grant validation", () => {
  it("refuses a delete request without a grant before any record changes", async () => {
    const fetchDigest = await seedProtectedStoredCopy();
    const storedCopyBefore = await storedCopySnapshot();
    const storedCopyCountBefore = await d1Count("SELECT COUNT(*) FROM blob_capability_index");
    const rateCountersBefore = await d1All<Record<string, unknown>>(
      "SELECT bucket_key, window_start, used FROM rate_counters ORDER BY bucket_key",
    );

    const response = await SELF.fetch(`${ORIGIN}/v1/blob/${ID}`, {
      method: "DELETE",
      headers: {
        "cf-connecting-ip": "198.51.100.45",
        "x-osl-manage-cap": MANAGE_CAP,
      },
    });
    const body = await response.json() as { error: string; message: string };

    expect(response.status).toBe(403);
    expect(body.error).toBe("delete_grant_required");
    expect(await storedCopySnapshot()).toEqual(storedCopyBefore);
    expect(await d1Count("SELECT COUNT(*) FROM blob_capability_index")).toBe(storedCopyCountBefore);
    await expect(env.PAYLOADS.head(fetchDigest)).resolves.toMatchObject({ size: 1 });
    await expect(
      d1All<Record<string, unknown>>("SELECT bucket_key, window_start, used FROM rate_counters ORDER BY bucket_key"),
    ).resolves.toEqual(rateCountersBefore);

    const rateCounterChanges = (
      await d1All<Record<string, unknown>>("SELECT bucket_key, window_start, used FROM rate_counters")
    ).length - rateCountersBefore.length;
    console.log(
      `TASK0405 delete_without_grant status=${response.status} refused=${body.error} blob_rows_before=${storedCopyCountBefore} blob_rows_after=${await d1Count("SELECT COUNT(*) FROM blob_capability_index")} rate_counter_changes=${rateCounterChanges} stored_copy_unchanged=yes payload_still_present=yes`,
    );
  });

  it("deletes the stored copy only when the scoped delete grant is present", async () => {
    const fetchDigest = await seedProtectedStoredCopy();
    const response = await SELF.fetch(`${ORIGIN}/v1/blob/${ID}`, {
      method: "DELETE",
      headers: {
        "cf-connecting-ip": "198.51.100.46",
        "x-osl-manage-cap": MANAGE_CAP,
        "x-osl-delete-grant": senderDeleteGrant(),
      },
    });

    expect(response.status).toBe(204);
    expect(await d1Count("SELECT COUNT(*) FROM blob_capability_index WHERE blob_id = ?", ID)).toBe(0);
    await expect(env.PAYLOADS.head(fetchDigest)).resolves.toBeNull();
  });
});

describe("TASK 0406 sender own-copy burn command", () => {
  it("deletes its permitted copy and reports the exact affected scope", async () => {
    const fixture = {
      id: "04060000000000000000000000000000",
      fetchCap: "f4060000000000000000000000000000",
      ackCap: "a4060000000000000000000000000000",
      manageCap: "d4060000000000000000000000000000",
      message: "message:0406",
      owner: "identity:sender-0406",
      burnScope: "discord:9000000000000406:direct_message:stored-copy",
    };
    const fetchDigest = await seedProtectedStoredCopyFixture(fixture);
    const grant = JSON.stringify({
      record: DELETE_GRANT_RECORD,
      message: fixture.message,
      owner: fixture.owner,
      scope: fixture.burnScope,
    });

    const report = await senderOwnCopyBurnCommand({
      fetch: SELF.fetch.bind(SELF),
      origin: ORIGIN,
      blobId: fixture.id,
      manageCap: fixture.manageCap,
      senderDeleteGrant: grant,
    });

    const remainingRows = await d1Count("SELECT COUNT(*) FROM blob_capability_index WHERE blob_id = ?", fixture.id);
    expect(report).toEqual({
      deleted: true,
      status: 204,
      affected_scope: fixture.burnScope,
    });
    expect(remainingRows).toBe(0);
    await expect(env.PAYLOADS.head(fetchDigest)).resolves.toBeNull();

    console.log(
      `TASK0406 sender_command=sender_own_copy_burn deleted=${report.deleted ? 1 : 0} status=${report.status} affected_scope=${report.affected_scope} remaining_rows=${remainingRows} payload_deleted=yes`,
    );
  });
});

describe("TASK 0413 sender own-copy burn on a two-copy fixture", () => {
  it("burns the sender copy while leaving the recipient copy readable", async () => {
    const message = "message:0413";
    const burnScope = "discord:9000000000000413:direct_message:stored-protected-message";
    const sender = {
      id: "04130000000000000000000000000001",
      fetchCap: "f4130000000000000000000000000001",
      ackCap: "a4130000000000000000000000000001",
      manageCap: "d4130000000000000000000000000001",
      message,
      owner: "identity:sender-0413",
      burnScope,
      deliveryTag: "1".repeat(32),
      payload: new TextEncoder().encode("sender-copy-0413"),
    };
    const recipient = {
      id: "04130000000000000000000000000002",
      fetchCap: "f4130000000000000000000000000002",
      ackCap: "a4130000000000000000000000000002",
      manageCap: "d4130000000000000000000000000002",
      message,
      owner: "identity:recipient-0413",
      burnScope,
      deliveryTag: "2".repeat(32),
      payload: new TextEncoder().encode("recipient-copy-0413"),
    };
    const senderFetchDigest = await seedProtectedStoredCopyFixture(sender);
    const recipientFetchDigest = await seedProtectedStoredCopyFixture(recipient);
    const grant = JSON.stringify({
      record: DELETE_GRANT_RECORD,
      message: sender.message,
      owner: sender.owner,
      scope: sender.burnScope,
    });

    const report = await senderOwnCopyBurnCommand({
      fetch: SELF.fetch.bind(SELF),
      origin: ORIGIN,
      blobId: sender.id,
      manageCap: sender.manageCap,
      senderDeleteGrant: grant,
    });

    const senderRemaining = await d1Count("SELECT COUNT(*) FROM blob_capability_index WHERE blob_id = ?", sender.id);
    const recipientRemaining = await d1Count("SELECT COUNT(*) FROM blob_capability_index WHERE blob_id = ?", recipient.id);
    const senderReadAfter = await SELF.fetch(`${ORIGIN}/v1/blob/${sender.id}`, {
      headers: { "x-osl-fetch-cap": sender.fetchCap },
    });
    const recipientReadAfter = await SELF.fetch(`${ORIGIN}/v1/blob/${recipient.id}`, {
      headers: { "x-osl-fetch-cap": recipient.fetchCap },
    });
    const recipientBody = new TextDecoder().decode(await recipientReadAfter.arrayBuffer());

    expect(report).toEqual({
      deleted: true,
      status: 204,
      affected_scope: burnScope,
    });
    expect(senderRemaining).toBe(0);
    expect(recipientRemaining).toBe(1);
    await expect(env.PAYLOADS.head(senderFetchDigest)).resolves.toBeNull();
    await expect(env.PAYLOADS.head(recipientFetchDigest)).resolves.toMatchObject({ size: recipient.payload.byteLength });
    expect(senderReadAfter.status).toBe(404);
    expect(recipientReadAfter.status).toBe(200);
    expect(recipientBody).toBe("recipient-copy-0413");

    console.log(
      `TASK0413 sender_command=sender_own_copy_burn fixture_copies=2 deleted=${report.deleted ? 1 : 0} sender_status=${report.status} sender_remaining=${senderRemaining} sender_read_status=${senderReadAfter.status} recipient_remaining=${recipientRemaining} recipient_read_status=${recipientReadAfter.status} recipient_body=${recipientBody} affected_scope=${report.affected_scope}`,
    );
  });
});
