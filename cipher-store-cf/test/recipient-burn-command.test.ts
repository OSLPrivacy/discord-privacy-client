import { describe, expect, it } from "vitest";
import { handleDelete, handleFetch, handleUpload } from "../src/endpoints/blob.js";
import { createStoredProtectedMessageGrants } from "../src/lib/delete-grant.js";
import { sha256Hex } from "../src/lib/digest.js";
import { d1Count, workerEnv } from "./helpers/workerd.js";

const scope = "discord:9000000000000407:direct_message:stored-protected-message";
const message = "message:0407";
const senderOwner = "identity:sender-0407";
const recipientOwner = "identity:recipient-0407";
const senderBlobId = "04070000000000000000000000000001";
const recipientBlobId = "04070000000000000000000000000002";
const senderFetchCap = "1".repeat(32);
const recipientFetchCap = "2".repeat(32);
const ackCap = "3".repeat(32);
const senderManageCap = "4".repeat(32);
const recipientManageCap = "5".repeat(32);

function body(): Uint8Array {
  return new Uint8Array([7]);
}

async function uploadCopy(input: {
  blobId: string;
  fetchCap: string;
  manageCap: string;
  owner: string;
  deleteMessage?: string;
  burnScope?: string;
}): Promise<Response> {
  return handleUpload(new Request("https://cipher.test/v1/blob", {
    method: "PUT",
    headers: {
      "x-osl-ttl-seconds": "3600",
      "x-osl-expiry-mode": "absolute",
      "x-osl-blob-id": input.blobId,
      "x-osl-fetch-digest": await sha256Hex(input.fetchCap),
      "x-osl-ack-digest": await sha256Hex(ackCap),
      "x-osl-manage-digest": await sha256Hex(input.manageCap),
      "x-osl-object-class": "single-ack",
      "x-osl-delivery-tag": "6".repeat(32),
      "x-osl-delete-message": input.deleteMessage ?? message,
      "x-osl-delete-owner": input.owner,
      "x-osl-burn-scope": input.burnScope ?? scope,
    },
    body: body(),
  }), workerEnv());
}

function burnCopy(blobId: string, manageCap: string, grant: unknown): Promise<Response> {
  return handleDelete(new Request(`https://cipher.test/v1/blob/${blobId}`, {
    method: "DELETE",
    headers: {
      "x-osl-manage-cap": manageCap,
      "x-osl-delete-grant": JSON.stringify(grant),
    },
  }), workerEnv(), blobId);
}

function burnCopyWithoutGrant(blobId: string, manageCap: string): Promise<Response> {
  return handleDelete(new Request(`https://cipher.test/v1/blob/${blobId}`, {
    method: "DELETE",
    headers: { "x-osl-manage-cap": manageCap },
  }), workerEnv(), blobId);
}

async function canRead(blobId: string, fetchCap: string): Promise<boolean> {
  const response = await handleFetch(new Request(`https://cipher.test/v1/blob/${blobId}`, {
    headers: { "x-osl-fetch-cap": fetchCap },
  }), workerEnv(), blobId);
  return response.status === 200;
}

describe("TASK 0407 recipient burn command", () => {
  it("uses a recipient-owned grant and removes only the recipient's permitted copy", async () => {
    const created = createStoredProtectedMessageGrants({
      message,
      readKey: recipientFetchCap,
      sender: senderOwner,
      recipient: recipientOwner,
      scope,
    });
    if (!created.ok) throw new Error(created.code);
    const senderGrant = created.grants.senderDeleteGrants[0]!;
    const recipientGrant = created.grants.recipientDeleteGrants[0]!;
    expect(senderGrant.owner).toBe(senderOwner);
    expect(recipientGrant.owner).toBe(recipientOwner);
    expect(senderGrant).not.toEqual(recipientGrant);

    expect((await uploadCopy({
      blobId: senderBlobId,
      fetchCap: senderFetchCap,
      manageCap: senderManageCap,
      owner: senderOwner,
    })).status).toBe(201);
    expect((await uploadCopy({
      blobId: recipientBlobId,
      fetchCap: recipientFetchCap,
      manageCap: recipientManageCap,
      owner: recipientOwner,
    })).status).toBe(201);

    const before = await d1Count(
      "SELECT COUNT(*) AS c FROM blob_capability_index WHERE blob_id IN (?, ?)",
      senderBlobId,
      recipientBlobId,
    );
    expect(before).toBe(2);

    const substituted = await burnCopy(recipientBlobId, recipientManageCap, senderGrant);
    const substitutedBody = await substituted.json() as { error: string };
    expect(substituted.status).toBe(403);
    expect(substitutedBody.error).toBe("delete_grant_owner_mismatch");
    expect(await canRead(recipientBlobId, recipientFetchCap)).toBe(true);

    const recipientBurn = await burnCopy(recipientBlobId, recipientManageCap, recipientGrant);
    expect(recipientBurn.status).toBe(204);

    const senderReadable = await canRead(senderBlobId, senderFetchCap);
    const recipientReadable = await canRead(recipientBlobId, recipientFetchCap);
    const remainingSenderCopies = await d1Count(
      "SELECT COUNT(*) AS c FROM blob_capability_index WHERE blob_id = ? AND delete_owner = ?",
      senderBlobId,
      senderOwner,
    );
    const remainingRecipientCopies = await d1Count(
      "SELECT COUNT(*) AS c FROM blob_capability_index WHERE blob_id = ? AND delete_owner = ?",
      recipientBlobId,
      recipientOwner,
    );

    console.log(
      `TASK0407 recipient_burn sender_grant_substitute_status=${substituted.status}`
      + ` substitute_refused=${substitutedBody.error}`
      + ` recipient_burn_status=${recipientBurn.status}`
      + ` before_copies=${before}`
      + ` sender_copy_remaining=${remainingSenderCopies}`
      + ` recipient_copy_remaining=${remainingRecipientCopies}`
      + ` sender_copy_readable=${senderReadable ? "yes" : "no"}`
      + ` recipient_copy_readable=${recipientReadable ? "yes" : "no"}`
      + ` recipient_grant_owner=${recipientGrant.owner}`
      + ` sender_grant_owner=${senderGrant.owner}`
      + ` scope=${recipientGrant.scope}`,
    );

    expect(remainingSenderCopies).toBe(1);
    expect(remainingRecipientCopies).toBe(0);
    expect(senderReadable).toBe(true);
    expect(recipientReadable).toBe(false);
  });
});

describe("TASK 0414 recipient own-copy burn", () => {
  it("directly burns the recipient copy while the sender copy remains readable", async () => {
    const taskScope = "discord:9000000000000414:direct_message:stored-protected-message";
    const taskMessage = "message:0414";
    const taskSenderOwner = "identity:sender-0414";
    const taskRecipientOwner = "identity:recipient-0414";
    const taskSenderBlobId = "04140000000000000000000000000001";
    const taskRecipientBlobId = "04140000000000000000000000000002";
    const taskSenderFetchCap = "a".repeat(32);
    const taskRecipientFetchCap = "b".repeat(32);
    const taskSenderManageCap = "c".repeat(32);
    const taskRecipientManageCap = "d".repeat(32);

    const created = createStoredProtectedMessageGrants({
      message: taskMessage,
      readKey: taskRecipientFetchCap,
      sender: taskSenderOwner,
      recipient: taskRecipientOwner,
      scope: taskScope,
    });
    if (!created.ok) throw new Error(created.code);
    const recipientGrant = created.grants.recipientDeleteGrants[0]!;

    expect((await uploadCopy({
      blobId: taskSenderBlobId,
      fetchCap: taskSenderFetchCap,
      manageCap: taskSenderManageCap,
      owner: taskSenderOwner,
      deleteMessage: taskMessage,
      burnScope: taskScope,
    })).status).toBe(201);
    expect((await uploadCopy({
      blobId: taskRecipientBlobId,
      fetchCap: taskRecipientFetchCap,
      manageCap: taskRecipientManageCap,
      owner: taskRecipientOwner,
      deleteMessage: taskMessage,
      burnScope: taskScope,
    })).status).toBe(201);

    const before = await d1Count(
      "SELECT COUNT(*) AS c FROM blob_capability_index WHERE blob_id IN (?, ?)",
      taskSenderBlobId,
      taskRecipientBlobId,
    );
    expect(before).toBe(2);

    const missingGrantBurn = await burnCopyWithoutGrant(
      taskRecipientBlobId,
      taskRecipientManageCap,
    );
    expect(missingGrantBurn.status).toBe(403);
    expect(await canRead(taskRecipientBlobId, taskRecipientFetchCap)).toBe(true);

    const recipientBurn = await burnCopy(
      taskRecipientBlobId,
      taskRecipientManageCap,
      recipientGrant,
    );
    expect(recipientBurn.status).toBe(204);

    const senderReadable = await canRead(taskSenderBlobId, taskSenderFetchCap);
    const recipientReadable = await canRead(taskRecipientBlobId, taskRecipientFetchCap);
    const remainingSenderCopies = await d1Count(
      "SELECT COUNT(*) AS c FROM blob_capability_index WHERE blob_id = ? AND delete_owner = ?",
      taskSenderBlobId,
      taskSenderOwner,
    );
    const remainingRecipientCopies = await d1Count(
      "SELECT COUNT(*) AS c FROM blob_capability_index WHERE blob_id = ? AND delete_owner = ?",
      taskRecipientBlobId,
      taskRecipientOwner,
    );

    console.log(
      `TASK0414 recipient_own_copy_burn direct_call=handleDelete`
      + ` before_copies=${before}`
      + ` missing_grant_status=${missingGrantBurn.status}`
      + ` recipient_burn_status=${recipientBurn.status}`
      + ` recipient_copy_remaining=${remainingRecipientCopies}`
      + ` recipient_copy_readable=${recipientReadable ? "yes" : "no"}`
      + ` sender_copy_remaining=${remainingSenderCopies}`
      + ` sender_copy_readable=${senderReadable ? "yes" : "no"}`
      + ` recipient_grant_owner=${recipientGrant.owner}`
      + ` sender_owner=${taskSenderOwner}`
      + ` scope=${recipientGrant.scope}`,
    );

    expect(remainingRecipientCopies).toBe(0);
    expect(recipientReadable).toBe(false);
    expect(remainingSenderCopies).toBe(1);
    expect(senderReadable).toBe(true);
  });
});
