import { describe, expect, it } from "vitest";
import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";
import {
  createOslChatDeliveryRuntime,
  OSL_CHAT_OPEN_REFUSAL_SENTENCE,
  oslChatOpenRefusalMessage,
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
  };
}

function openedBatch(messageId: string, plaintext: string): NativeDiscordOverlayOpenedBatch {
  return {
    messages: [{
      messageId,
      plaintext,
      contextVerified: true,
      personToPersonE2ee: true,
      viewOnceConsumed: false,
      createdAt: 1_700_000_003,
      expiresAt: 4_000_000_000,
    }],
    pendingViewOnce: [],
    acknowledgments: [],
    fetched: 1,
    decryptDisplayEnabled: true,
    deferredRows: 0,
    unrecognizedWireRows: 0,
  };
}

describe("OSL Chat delivery runtime receive failures", () => {
  it("TASK4012 says the fixed refusal on screen when a private message was burned before opening", async () => {
    const controlWords = "control private words open exactly";
    const burnedWords = "embervault siltglass hushfield";
    const committedMessages: string[] = [];
    const openedCounts: number[] = [];
    let drainNumber = 0;

    const host: OslChatDeliveryHost = {
      identityLoaded: () => true,
      foreignContextActive: () => false,
      openConversationId: () => "p1",
      conversationBusy: () => false,
      friends: () => [{ personId: "p1", safetyNumberVerified: true, pendingKeyChange: false }],
      requestCaptureProtection: async () => true,
      activateContext: async (personId) => ({ personId, peerOslUserId: "OSLUSER-p1", scopeApproved: true }),
      closeContext: async () => true,
      drainInbox: async () => {
        drainNumber += 1;
        if (drainNumber === 1) return openedBatch("control-message", controlWords);
        return null;
      },
      drainRefusal: () => drainNumber === 2 ? OSL_CHAT_OPEN_REFUSAL_SENTENCE : null,
      loadHistory: async () => null,
      commitBatch: (_personId, batch) => {
        openedCounts.push(batch.messages.length);
        committedMessages.push(...batch.messages.map((message) => message.plaintext));
      },
      commitNotice: (_personId, message) => {
        committedMessages.push(message.body);
      },
      commitHistory: () => {},
    };

    const runtime = createOslChatDeliveryRuntime(host);
    await runtime.sync();
    await runtime.sync();

    const receivingScreenText = committedMessages.join("\n");
    const burnedPrivateWordsFound = burnedWords
      .split(" ")
      .filter((word) => receivingScreenText.includes(word))
      .length;
    const burnedOpenedPrivateMessages = openedCounts[1] ?? 0;
    const refusalOnScreen = receivingScreenText.includes(OSL_CHAT_OPEN_REFUSAL_SENTENCE) ? 1 : 0;

    expect(committedMessages[0]).toBe(controlWords);
    expect(openedCounts[0]).toBe(1);
    expect(burnedOpenedPrivateMessages).toBe(0);
    expect(refusalOnScreen).toBe(1);
    expect(burnedPrivateWordsFound).toBe(0);
    expect(receivingScreenText).not.toContain(burnedWords);
    expect(oslChatOpenRefusalMessage("manual-check", OSL_CHAT_OPEN_REFUSAL_SENTENCE, "Now")?.body)
      .toBe(OSL_CHAT_OPEN_REFUSAL_SENTENCE);

    console.log(`TASK4012_CONTROL_OPENED_WORDS="${controlWords}"`);
    console.log(`TASK4012_CONTROL_OPENED_PRIVATE_MESSAGES=${openedCounts[0]}`);
    console.log(`TASK4012_BURNED_OPENED_PRIVATE_MESSAGES=${burnedOpenedPrivateMessages}`);
    console.log(`TASK4012_BURNED_REFUSAL_SENTENCE="${OSL_CHAT_OPEN_REFUSAL_SENTENCE}"`);
    console.log(`TASK4012_BURNED_REFUSAL_ON_SCREEN=${refusalOnScreen}`);
    console.log(`TASK4012_BURNED_PRIVATE_WORDS_FOUND_ON_RECEIVER=${burnedPrivateWordsFound}`);
  });

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
