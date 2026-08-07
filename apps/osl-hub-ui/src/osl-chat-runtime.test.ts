import { describe, expect, it } from "vitest";
import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";
import {
  createOslChatDeliveryRuntime,
  oslChatHistoryMessages,
  oslChatUnrecognizedWireRowsNotice,
  receivedOslChatBatchMessage,
  type OslChatDeliveryHost,
} from "./osl-chat-runtime";

function batchWithUnrecognized(): NativeDiscordOverlayOpenedBatch {
  return {
    messages: [{
      messageId: "peer-11111111111111111111111111111111",
      plaintext: "first good row",
      contextVerified: true,
      personToPersonE2ee: true,
      viewOnceConsumed: false,
      createdAt: 1_700_000_001,
      expiresAt: 4_000_000_000,
    }, {
      messageId: "peer-22222222222222222222222222222222",
      plaintext: "second good row",
      contextVerified: true,
      personToPersonE2ee: true,
      viewOnceConsumed: false,
      createdAt: 1_700_000_002,
      expiresAt: 4_000_000_000,
    }],
    pendingViewOnce: [],
    acknowledgments: [],
    fetched: 2,
    decryptDisplayEnabled: true,
    deferredRows: 0,
    unrecognizedWireRows: 1,
    contentGoneRows: 0,
  };
}

describe("OSL Chat delivery runtime receive failures", () => {
  it("commits good backfilled rows and then surfaces unrecognized wire rows after history refresh", async () => {
    const events: string[] = [];
    const host: OslChatDeliveryHost = {
      identityLoaded: () => true,
      foreignContextActive: () => false,
      openConversationId: () => null,
      conversationBusy: () => false,
      friends: () => [{ personId: "p1", safetyNumberVerified: true, pendingKeyChange: false }],
      requestCaptureProtection: async () => true,
      activateContext: async (personId) => ({ personId, peerOslUserId: "OSLUSER-p1", scopeApproved: true }),
      closeContext: async () => true,
      drainInbox: async () => batchWithUnrecognized(),
      loadHistory: async () => [{
        messageId: "peer-11111111111111111111111111111111",
        senderOslUserId: "OSLUSER-p1",
        plaintext: "first good row",
        createdAt: 1_700_000_001,
        decryptedAt: 1_700_000_001,
      }, {
        messageId: "peer-22222222222222222222222222222222",
        senderOslUserId: "OSLUSER-p1",
        plaintext: "second good row",
        createdAt: 1_700_000_002,
        decryptedAt: 1_700_000_002,
      }],
      commitBatch: (_personId, batch) => {
        events.push(`batch:${batch.messages.map((message) => message.plaintext).join("|")}`);
      },
      commitHistory: (_personId, rows) => {
        events.push(`history:${rows.map((row) => row.plaintext).join("|")}`);
      },
    };

    await createOslChatDeliveryRuntime(host).sync();

    expect(events).toEqual([
      "batch:first good row|second good row",
      "history:first good row|second good row",
      `batch:${oslChatUnrecognizedWireRowsNotice(1)}`,
    ]);
  });

  it("labels newly opened offline messages with the sender-created timestamp, not arrival time", () => {
    const message = batchWithUnrecognized().messages[0]!;
    const label = receivedOslChatBatchMessage(
      "received-local",
      { ...message, createdAt: 1_700_000_001 },
      (epochSeconds) => epochSeconds === 1_700_000_001 ? "11:33 AM" : "12:21 PM",
    );

    expect(label.timestampLabel).toBe("11:33 AM");
  });

  it("labels reopened history with the sender-created timestamp, not the decrypt time", () => {
    const [message] = oslChatHistoryMessages([{
      messageId: "peer-history",
      senderOslUserId: "OSLUSER-p1",
      plaintext: "offline hello",
      createdAt: 1_700_000_001,
      decryptedAt: 1_700_002_881,
      reactions: [{ emoji: "👍", count: 1, mine: true }],
    }], {
      personId: "p1",
      peerOslUserId: "OSLUSER-p1",
      scopeApproved: true,
    }, (epochSeconds) => epochSeconds === 1_700_000_001 ? "11:33 AM" : "12:21 PM");

    expect(message?.timestampLabel).toBe("11:33 AM");
    expect(message?.reactions).toEqual([{ emoji: "👍", count: 1, mine: true }]);
  });
});
