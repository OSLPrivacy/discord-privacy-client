import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { handleDelete, handleFetch, handleUpload } from "../src/endpoints/blob.js";
import {
  countUsableSenderDeleteGrants,
  createStoredProtectedMessageGrants,
  recipientMessageData,
} from "../src/lib/delete-grant.js";
import { sha256Hex } from "../src/lib/digest.js";
import { workerEnv } from "./helpers/workerd.js";

const BLOB_ID = "04120000000000000000000000000000";
const ACK_CAP = "a".repeat(32);
const DELIVERY_TAG = "b".repeat(32);
const BODY = new Uint8Array([7]);

async function rowCount(blobId: string): Promise<number> {
  const row = await env.DB.prepare(
    "SELECT COUNT(*) AS c FROM blob_capability_index WHERE blob_id = ?",
  ).bind(blobId).first<{ c: number }>();
  return Number(row?.c ?? 0);
}

describe("TASK 0412 separate read and delete secrets", () => {
  it("reads as recipient but refuses delete using only recipient read data", async () => {
    const grants = await createStoredProtectedMessageGrants({
      message: "message:0412",
      owner: "identity:sender-0412",
      scope: "discord:9000000000000412:direct_message:separate-read-delete",
    });
    const recipient = recipientMessageData({
      grants,
      ciphertext: "stored-in-cipher-store",
      nonce: "nonce-0412",
    });
    const readKey = recipient.readKeys[0]!;
    const senderDeleteGrant = grants.senderDeleteGrants[0]!;
    const payloadKey = await sha256Hex(readKey.key);

    const created = await handleUpload(
      new Request("https://cipher.test/v1/blob", {
        method: "PUT",
        headers: {
          "x-osl-ttl-seconds": "3600",
          "x-osl-expiry-mode": "absolute",
          "x-osl-blob-id": BLOB_ID,
          "x-osl-fetch-digest": await sha256Hex(readKey.key),
          "x-osl-ack-digest": await sha256Hex(ACK_CAP),
          "x-osl-manage-digest": await sha256Hex(senderDeleteGrant.grant),
          "x-osl-object-class": "single-ack",
          "x-osl-delivery-tag": DELIVERY_TAG,
          "x-osl-delete-message": grants.message,
          "x-osl-delete-owner": grants.owner,
          "x-osl-burn-scope": grants.scope,
        },
        body: BODY,
      }),
      workerEnv(),
    );
    expect(created.status).toBe(201);

    const read = await handleFetch(
      new Request(`https://cipher.test/v1/blob/${BLOB_ID}`, {
        headers: { "x-osl-fetch-cap": readKey.key },
      }),
      workerEnv(),
      BLOB_ID,
    );
    expect(read.status).toBe(200);
    expect(new Uint8Array(await read.arrayBuffer())).toEqual(BODY);

    const usableSenderDeleteGrantCount = countUsableSenderDeleteGrants(recipient);
    expect(usableSenderDeleteGrantCount).toBe(0);
    const beforeRows = await rowCount(BLOB_ID);

    const refused = await handleDelete(
      new Request(`https://cipher.test/v1/blob/${BLOB_ID}`, {
        method: "DELETE",
        headers: { "x-osl-fetch-cap": readKey.key },
      }),
      workerEnv(),
      BLOB_ID,
    );
    const refusedBody = await refused.json<{ error: string }>();
    expect(refused.status).toBe(403);
    expect(refusedBody.error).toBe("delete_grant_required");

    const afterRows = await rowCount(BLOB_ID);
    expect(afterRows).toBe(beforeRows);
    expect(await env.PAYLOADS.head(payloadKey)).not.toBeNull();

    const reread = await handleFetch(
      new Request(`https://cipher.test/v1/blob/${BLOB_ID}`, {
        headers: { "x-osl-fetch-cap": readKey.key },
      }),
      workerEnv(),
      BLOB_ID,
    );
    expect(reread.status).toBe(200);
    expect(new Uint8Array(await reread.arrayBuffer())).toEqual(BODY);

    console.log(
      `TASK0412 message=${grants.message} recipient_read_status=${read.status} `
      + `recipient_fields=${Object.keys(recipient).sort().join(",")} `
      + `usable_sender_delete_grant_count=${usableSenderDeleteGrantCount} `
      + `delete_status=${refused.status} refused=${refusedBody.error} `
      + `rows_before=${beforeRows} rows_after=${afterRows} `
      + `payload_still_present=${await env.PAYLOADS.head(payloadKey) === null ? "no" : "yes"} `
      + `reread_status=${reread.status}`,
    );
  });
});
