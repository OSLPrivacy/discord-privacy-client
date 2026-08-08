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
    contentGoneRows: 0,
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
    contentGoneRows: 0,
  };
}

function emptyOpenedBatch(overrides: Partial<NativeDiscordOverlayOpenedBatch> = {}): NativeDiscordOverlayOpenedBatch {
  const { contentGoneRows = 0, ...otherOverrides } = overrides;
  return {
    messages: [],
    pendingViewOnce: [],
    acknowledgments: [],
    fetched: 0,
    decryptDisplayEnabled: true,
    deferredRows: 0,
    unrecognizedWireRows: 0,
    ...otherOverrides,
    contentGoneRows,
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

  it("TASK4021 keeps a good open visible when every gated receive failure kind is mixed into the same delivery batch", async () => {
    const goodWords = "TASK-4021 good message exact plaintext";
    const failingKinds = [
      "store-unreachable",
      "content-gone",
      "burned-message",
      "expired-timer",
    ] as const;
    type FailingKind = typeof failingKinds[number];

    async function runMixedBatch(label: string, kinds: readonly FailingKind[]): Promise<number> {
      const people = [...kinds.map((kind, index) => `${kind}-${index}`), "good"];
      const committedBodies: string[] = [];
      let activePerson: string | null = null;

      const host: OslChatDeliveryHost = {
        identityLoaded: () => true,
        foreignContextActive: () => false,
        openConversationId: () => null,
        conversationBusy: () => false,
        friends: () => people.map((personId) => ({
          personId,
          safetyNumberVerified: true,
          pendingKeyChange: false,
        })),
        requestCaptureProtection: async () => true,
        activateContext: async (personId) => {
          activePerson = personId;
          return { personId, peerOslUserId: `OSLUSER-${personId}`, scopeApproved: true };
        },
        closeContext: async () => {
          activePerson = null;
          return true;
        },
        drainInbox: async () => {
          if (activePerson === "good") return openedBatch("task-4021-good-message", goodWords);
          const kind = kinds.find((candidate) => activePerson?.startsWith(candidate)) ?? null;
          switch (kind) {
            case "store-unreachable":
              return emptyOpenedBatch({ deferredRows: 1 });
            case "content-gone":
            case "burned-message":
            case "expired-timer":
              return emptyOpenedBatch();
            default:
              throw new Error(`unexpected task 4021 active person: ${activePerson}`);
          }
        },
        loadHistory: async () => null,
        commitBatch: (_personId, batch) => {
          committedBodies.push(...batch.messages.map((message) => message.plaintext));
        },
        commitHistory: () => {},
      };

      await createOslChatDeliveryRuntime(host, { batchLimit: people.length }).sync();
      const goodExactMatches = committedBodies.filter((body) => body === goodWords).length;

      console.log(`TASK4021_MIXED_BATCH label=${label} failing_kinds=${kinds.length} good_exact_matches=${goodExactMatches} committed_bodies=${committedBodies.length}`);
      expect(goodExactMatches).toBe(1);
      expect(committedBodies.filter((body) => body.includes("TASK-4021")).length).toBe(1);
      return goodExactMatches;
    }

    let minimumGoodCount = Number.POSITIVE_INFINITY;
    for (const kind of failingKinds) {
      const goodCount = await runMixedBatch(kind, [kind]);
      minimumGoodCount = Math.min(minimumGoodCount, goodCount);
      console.log(`TASK4021_FAILING_KIND label=${kind} good_exact_matches=${goodCount}`);
    }
    const allKindsGoodCount = await runMixedBatch("all_failing_kinds", failingKinds);
    minimumGoodCount = Math.min(minimumGoodCount, allKindsGoodCount);

    console.log(`TASK4021_GOOD_EXACT_STRING="${goodWords}"`);
    console.log(`TASK4021_FAILING_KINDS_TRIED=${failingKinds.length}`);
    console.log(`TASK4021_MIN_GOOD_EXACT_MATCHES=${minimumGoodCount}`);
    console.log(`TASK4021_ALL_KINDS_BATCH_GOOD_EXACT_MATCHES=${allKindsGoodCount}`);
    expect(failingKinds.length).toBeGreaterThanOrEqual(4);
    expect(minimumGoodCount).toBeGreaterThanOrEqual(1);
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
        reactions: [],
      }, {
        messageId: "peer-22222222222222222222222222222222",
        senderOslUserId: "OSLUSER-p1",
        plaintext: "second good row",
        createdAt: 1_700_000_002,
        decryptedAt: 1_700_000_002,
        reactions: [],
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
    expect(label.dateLabel).toBe(new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" })
      .format(new Date(1_700_000_001_000)));
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
    expect(message?.dateLabel).toBe(new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" })
      .format(new Date(1_700_000_001_000)));
    expect(message?.reactions).toEqual([{ emoji: "👍", count: 1, mine: true }]);
  });
});
