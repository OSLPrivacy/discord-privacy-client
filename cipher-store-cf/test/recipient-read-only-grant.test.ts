import { describe, expect, it } from "vitest";
import {
  DELETE_GRANT_RECORD,
  MESSAGE_READ_KEY_RECORD,
  createRecipientProtectedMessageData,
  createStoredProtectedMessageGrants,
  decryptRecipientProtectedMessageData,
  usableSenderDeleteGrantCount,
} from "../src/lib/delete-grant.js";

const message = "message:0403";
const sender = "identity:sender-0403";
const scope = "discord:9000000000000403:direct_message:recipient-read-only";
const readKey = "c".repeat(32);
const plaintext = "TASK0403 recipient read-only grant text";

describe("TASK 0403 recipient read-only grant", () => {
  it("decrypts recipient message data without carrying a usable sender delete grant", async () => {
    const senderSide = createStoredProtectedMessageGrants({
      message,
      readKey,
      sender,
      scope,
    });
    expect(senderSide.ok).toBe(true);
    if (!senderSide.ok) throw new Error(`sender grants failed: ${senderSide.code}`);
    expect(senderSide.grants.senderDeleteGrants).toHaveLength(1);
    expect(senderSide.grants.senderDeleteGrants[0]).toMatchObject({
      record: DELETE_GRANT_RECORD,
      owner: sender,
      scope,
    });

    const created = await createRecipientProtectedMessageData({
      message,
      readKey,
      plaintext,
    });
    expect(created.ok).toBe(true);
    if (!created.ok) throw new Error(`recipient data failed: ${created.code}`);

    expect(created.data.readKeys).toHaveLength(1);
    expect(created.data.readKeys[0]).toEqual({
      record: MESSAGE_READ_KEY_RECORD,
      message,
      readKey,
    });
    expect(Object.hasOwn(created.data, "senderDeleteGrants")).toBe(false);
    expect(Object.hasOwn(created.data, "senderDeleteGrant")).toBe(false);
    expect(usableSenderDeleteGrantCount(created.data, sender, scope)).toBe(0);

    const opened = await decryptRecipientProtectedMessageData(created.data);
    expect(opened).toEqual({ ok: true, plaintext });
    if (!opened.ok) throw new Error(`decrypt failed: ${opened.code}`);

    const recipientFields = Object.keys(created.data).sort().join(",");
    const usableDeleteGrants = usableSenderDeleteGrantCount(created.data, sender, scope);
    console.log(
      `TASK0403 decrypted_text=${opened.plaintext} recipient_fields=${recipientFields} usable_sender_delete_grant_count=${usableDeleteGrants} usable_sender_delete_grant=${usableDeleteGrants > 0 ? "yes" : "no"}`,
    );
  });

  it("detects recipient message data polluted with the sender delete grant", async () => {
    const senderSide = createStoredProtectedMessageGrants({
      message,
      readKey,
      sender,
      scope,
    });
    expect(senderSide.ok).toBe(true);
    if (!senderSide.ok) throw new Error(`sender grants failed: ${senderSide.code}`);

    const created = await createRecipientProtectedMessageData({
      message,
      readKey,
      plaintext,
    });
    expect(created.ok).toBe(true);
    if (!created.ok) throw new Error(`recipient data failed: ${created.code}`);

    const polluted = {
      ...created.data,
      senderDeleteGrants: senderSide.grants.senderDeleteGrants,
    };
    const usableDeleteGrants = usableSenderDeleteGrantCount(polluted, sender, scope);
    expect(usableDeleteGrants).toBe(1);
    console.log(
      `TASK0403 break_check polluted_usable_sender_delete_grant_count=${usableDeleteGrants}`,
    );
  });
});
