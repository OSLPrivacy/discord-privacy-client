import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { DELETE_GRANT_RECORD } from "../src/lib/delete-grant.js";
import { sha256Hex } from "../src/lib/digest.js";
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
  await d1Run(
    `INSERT INTO blob_capability_index (
       blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
       manage_digest_sha256_hex, object_class, pool, delivery_tag,
       size_bytes, expires_at, created_at,
       delete_grant_message, delete_grant_owner, burn_scope
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
