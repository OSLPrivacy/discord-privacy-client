import { describe, expect, it } from "vitest";
import { env, runInDurableObject } from "cloudflare:test";
import { base64Encode } from "../../src/mail/protocol.js";
import type { Mailbox } from "../../src/mail/mailbox.js";

const OWNER = "owner-task4326";
const UNOPENED = "mail_task4326_unopened";

describe("task 4326 OSL Mail I-have-taken-it command", () => {
  it("acknowledges only opened messages and keeps the unacknowledged-drop count at zero", async () => {
    const box = env.MAILBOX.getByName(`task4326-${crypto.randomUUID()}`);
    const now = Date.now();
    const opened = [
      "mail_task4326_opened_1",
      "mail_task4326_opened_2",
      "mail_task4326_opened_3",
    ];

    for (const messageId of [...opened, UNOPENED]) {
      await box.store({
        ownerUserId: OWNER,
        messageId,
        requestId: `store-${messageId}`,
        kind: "osl_e2ee",
        senderUserId: "sender-task4326",
        opaqueThreadToken: `thread-${messageId}`,
        ciphertextB64: base64Encode(new TextEncoder().encode(`ciphertext-${messageId}`)),
        envelopeJson: JSON.stringify({ version: 1, nonce_b64: "bm9uY2U=" }),
        recipientKeyFingerprint: "fingerprint-task4326",
        receivedAt: now,
        expiresAt: now + 60_000,
      });
    }

    for (const messageId of opened) {
      expect(await box.fetchMessage(OWNER, messageId)).toMatchObject({ message_id: messageId });
    }

    let acknowledged = 0;
    for (const messageId of opened) {
      const result = await box.ack(OWNER, `ack-${messageId}`, messageId, now);
      if (result.deleted) acknowledged += 1;
    }

    const openedHeldAfterAck = (
      await Promise.all(opened.map((messageId) => box.fetchMessage(OWNER, messageId)))
    ).filter(Boolean).length;

    let unopenedRefusal = "";
    await runInDurableObject(box, async (instance) => {
      try {
        (instance as unknown as Mailbox).ack(OWNER, "ack-unopened", UNOPENED, now);
      } catch (error) {
        unopenedRefusal = error instanceof Error ? error.message : String(error);
      }
    });
    if (unopenedRefusal === "") {
      throw new Error(`message dropped without being saved: ${UNOPENED}`);
    }
    expect(unopenedRefusal).toBe(`message was never opened: ${UNOPENED}`);

    const unopenedStillHeld = await box.fetchMessage(OWNER, UNOPENED);
    if (!unopenedStillHeld) {
      throw new Error(`message dropped without being saved: ${UNOPENED}`);
    }

    let dropped = -1;
    await runInDurableObject(box, async (instance) => {
      dropped = (instance as unknown as Mailbox).droppedWithoutAcknowledgement();
    });

    console.log(`task_4326_opened_messages_acknowledged=${acknowledged}`);
    console.log(`task_4326_opened_messages_held_after_ack=${openedHeldAfterAck}`);
    console.log(`task_4326_unopened_refusal="${unopenedRefusal}"`);
    console.log(`task_4326_dropped_without_acknowledgement=${dropped}`);

    expect(acknowledged).toBe(3);
    expect(openedHeldAfterAck).toBe(0);
    expect(dropped).toBe(0);
  });
});
