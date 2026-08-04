import { describe, expect, it } from "vitest";
import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";
import {
  createOslChatDeliveryRuntime,
  oslChatUnrecognizedWireRowsNotice,
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
      expiresAt: 4_000_000_000,
    }, {
      messageId: "peer-22222222222222222222222222222222",
      plaintext: "second good row",
      contextVerified: true,
      personToPersonE2ee: true,
      viewOnceConsumed: false,
      expiresAt: 4_000_000_000,
    }],
    pendingViewOnce: [],
    acknowledgments: [],
    fetched: 2,
    decryptDisplayEnabled: true,
    deferredRows: 0,
    unrecognizedWireRows: 1,
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
        decryptedAt: 1_700_000_001,
      }, {
        messageId: "peer-22222222222222222222222222222222",
        senderOslUserId: "OSLUSER-p1",
        plaintext: "second good row",
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
});
