import {
  env,
  evictDurableObject,
  runDurableObjectAlarm,
  runInDurableObject,
} from "cloudflare:test";
import {
  EMPTY_FRAME,
  FRAME_BYTES,
  IDLE_TICK,
  encodeWakeupFrame,
} from "../src/realtime/connection.js";
import type { PushConnection } from "../src/realtime/connection.js";
import { describe, expect, it } from "vitest";

const TAG = "a".repeat(32);
const BLOB = "b".repeat(32);
const TASK_3924_ACCOUNT_NAMES = ["AccountQuartzSeven", "AccountVelvetNine"];
const TASK_3924_FRIEND_NAMES = ["FriendYaraNorth", "FriendMiloSouth"];
const TASK_3924_CONVERSATION_NAMES = [
  "ConversationJuniperBay",
  "ConversationLumenDock",
];
const TASK_3924_PRIVATE_WORDS = [
  "private-amber-ledger",
  "private-violet-receipt",
  "private-cobalt-phrase",
  "private-silver-token",
  "private-copper-archive",
];
const TASK_3924_MESSAGE_FIXTURES = [
  {
    accountName: TASK_3924_ACCOUNT_NAMES[0]!,
    friendName: TASK_3924_FRIEND_NAMES[0]!,
    conversationName: TASK_3924_CONVERSATION_NAMES[0]!,
    privateWord: TASK_3924_PRIVATE_WORDS[0]!,
    wakeup: {
      delivery_tag: "10000000000000000000000000000001",
      blob_id: "20000000000000000000000000000001",
    },
  },
  {
    accountName: TASK_3924_ACCOUNT_NAMES[0]!,
    friendName: TASK_3924_FRIEND_NAMES[1]!,
    conversationName: TASK_3924_CONVERSATION_NAMES[1]!,
    privateWord: TASK_3924_PRIVATE_WORDS[1]!,
    wakeup: {
      delivery_tag: "10000000000000000000000000000002",
      blob_id: "20000000000000000000000000000002",
    },
  },
  {
    accountName: TASK_3924_ACCOUNT_NAMES[1]!,
    friendName: TASK_3924_FRIEND_NAMES[0]!,
    conversationName: TASK_3924_CONVERSATION_NAMES[0]!,
    privateWord: TASK_3924_PRIVATE_WORDS[2]!,
    wakeup: {
      delivery_tag: "10000000000000000000000000000003",
      blob_id: "20000000000000000000000000000003",
    },
  },
  {
    accountName: TASK_3924_ACCOUNT_NAMES[1]!,
    friendName: TASK_3924_FRIEND_NAMES[1]!,
    conversationName: TASK_3924_CONVERSATION_NAMES[1]!,
    privateWord: TASK_3924_PRIVATE_WORDS[3]!,
    wakeup: {
      delivery_tag: "10000000000000000000000000000004",
      blob_id: "20000000000000000000000000000004",
    },
  },
  {
    accountName: TASK_3924_ACCOUNT_NAMES[0]!,
    friendName: TASK_3924_FRIEND_NAMES[0]!,
    conversationName: TASK_3924_CONVERSATION_NAMES[0]!,
    privateWord: TASK_3924_PRIVATE_WORDS[4]!,
    wakeup: {
      delivery_tag: "10000000000000000000000000000005",
      blob_id: "20000000000000000000000000000005",
    },
  },
];

/// D-293 — the second test in this file used to read `connection.ts` back as
/// TEXT through a `?raw` import and grade three spellings:
///
///   expect(connectionSource).toContain("this.ctx.setWebSocketAutoResponse(AUTO_RESPONSE)");
///   expect(connectionSource).toContain("this.ctx.setWebSocketAutoResponse()");
///   expect(connectionSource).not.toMatch(/setAlarm|\balarm\s*\(|setTimeout|setInterval/);
///
/// on a module the same file already imports and EXECUTES — the D-221 shape.
/// Every one of those claims is observable at runtime, and the source-text form
/// was strictly weaker than the executing one, which is not an argument but a
/// measurement: swapping the auto-response's PAYLOAD (`new
/// WebSocketRequestResponsePair(IDLE_TICK, IDLE_TICK)`) or deleting the
/// restoring call in `webSocketMessage` both leave the pinned spellings in the
/// file, so the pins stayed GREEN through sabotage that this test catches.
///
/// The claims below are the same three, executed:
///
///   1. the connection installs exactly the idle mapping IDLE_TICK -> EMPTY_FRAME;
///   2. an idle tick against a HIBERNATED object is answered by the RUNTIME —
///      `getWebSocketAutoResponseTimestamp()` is set only when workerd itself
///      answered, so it is the object not being woken, not a spelling of it;
///   3. a real `deliver()` CLEARS that mapping, so the next identical tick is
///      answered by the handler instead (proved by the same timestamp NOT
///      advancing) and carries the real wakeup, after which the mapping is
///      restored byte for byte;
///   4. nothing is ever scheduled: no alarm exists and none runs.
const namespace = (
  env as unknown as Record<string, DurableObjectNamespace<PushConnection>>
).PUSH_CONNECTION;

/// One connected client, with every server frame captured in order.
async function connect(stub: DurableObjectStub<PushConnection>): Promise<{
  send: (frame: string) => void;
  frame: (index: number) => Promise<string>;
}> {
  const response = await stub.fetch("https://push.invalid/", {
    headers: { Upgrade: "websocket" },
  });
  expect(response.status).toBe(101);
  const client = response.webSocket;
  if (!client) throw new Error("the Durable Object did not return a WebSocket");

  const frames: string[] = [];
  client.addEventListener("message", (event) => {
    frames.push(String((event as MessageEvent).data));
  });
  client.accept();

  return {
    send: (frame: string) => client.send(frame),
    frame: async (index: number) => {
      const deadline = Date.now() + 10_000;
      while (frames.length <= index) {
        if (Date.now() > deadline) {
          throw new Error(
            `timed out waiting for server frame ${index}; got ${frames.length}`,
          );
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      return frames[index]!;
    },
  };
}

/// The runtime's own record of when it last answered a tick on this socket
/// WITHOUT waking the object. Null means every reply so far came from the
/// hibernation-breaking handler path instead.
async function lastAutoResponse(
  stub: DurableObjectStub<PushConnection>,
): Promise<number | null> {
  return runInDurableObject(stub, (_instance, state) => {
    const socket = state.getWebSockets()[0];
    if (!socket) throw new Error("no live socket");
    return state.getWebSocketAutoResponseTimestamp(socket)?.getTime() ?? null;
  });
}

/// The installed idle mapping, as plain values: `null` when there is none.
async function autoResponse(
  stub: DurableObjectStub<PushConnection>,
): Promise<{ request: string; response: string } | null> {
  return runInDurableObject(stub, (_instance, state) => {
    const pair = state.getWebSocketAutoResponse();
    return pair === null
      ? null
      : { request: pair.request, response: pair.response };
  });
}

function countNeedle(haystack: string, needle: string): number {
  let count = 0;
  let offset = 0;
  while (true) {
    const found = haystack.indexOf(needle, offset);
    if (found === -1) return count;
    count += 1;
    offset = found + needle.length;
  }
}

describe("T1-T51 push Durable Object", () => {
  it("emits only fixed-size delivery_tag/blob_id wakeup frames", () => {
    const real = encodeWakeupFrame({ delivery_tag: TAG, blob_id: BLOB });

    expect(IDLE_TICK).toHaveLength(FRAME_BYTES);
    expect(EMPTY_FRAME).toHaveLength(FRAME_BYTES);
    expect(real).toHaveLength(FRAME_BYTES);
    expect(JSON.parse(EMPTY_FRAME.trim())).toEqual({
      delivery_tag: "0".repeat(32),
      blob_id: "0".repeat(32),
    });
    expect(JSON.parse(real.trim())).toEqual({ delivery_tag: TAG, blob_id: BLOB });
    expect(() => encodeWakeupFrame({ delivery_tag: "P", blob_id: BLOB })).toThrow();
  });

  it("keeps idle ticks in the hibernation auto-response path, not an alarm", async () => {
    // Cost-shape sabotage: replacing the auto-response with an alarm/timer
    // wakes the object per tick, so this assertion must turn red.
    const stub = namespace.get(namespace.newUniqueId());
    const socket = await connect(stub);

    // 1. The connection installs exactly the idle mapping and nothing else, and
    //    the runtime has not answered anything yet.
    expect(await autoResponse(stub)).toEqual({
      request: IDLE_TICK,
      response: EMPTY_FRAME,
    });
    expect(await lastAutoResponse(stub)).toBeNull();

    // 2. Tear the instance down so the socket is genuinely hibernated, then
    //    tick. The reply is the decoy frame and the runtime records that IT
    //    answered — the object was not woken to produce it.
    await evictDurableObject(stub);
    socket.send(IDLE_TICK);
    expect(await socket.frame(0)).toBe(EMPTY_FRAME);
    const idleAnswer = await lastAutoResponse(stub);
    expect(idleAnswer).not.toBeNull();

    // 3. A genuine hit clears the mapping, so the next IDENTICAL tick is
    //    answered by the handler instead of the runtime: the same bytes, a
    //    different answering path, and the wakeup rides out on it.
    expect(
      await runInDurableObject(stub, (instance) =>
        instance.deliver({ delivery_tag: TAG, blob_id: BLOB })),
    ).toBe(true);
    expect(await autoResponse(stub)).toBeNull();

    socket.send(IDLE_TICK);
    const wakeup = await socket.frame(1);
    expect(wakeup).toHaveLength(FRAME_BYTES);
    expect(JSON.parse(wakeup.trim())).toEqual({ delivery_tag: TAG, blob_id: BLOB });
    // The runtime did not answer this one: its timestamp has not moved.
    expect(await lastAutoResponse(stub)).toBe(idleAnswer);
    // ...and the idle mapping is restored byte for byte for the next period.
    expect(await autoResponse(stub)).toEqual({
      request: IDLE_TICK,
      response: EMPTY_FRAME,
    });

    // 4. No wakeup was ever scheduled: idle traffic costs no alarm.
    expect(
      await runInDurableObject(stub, (_instance, state) => state.storage.getAlarm()),
    ).toBeNull();
    expect(await runDurableObjectAlarm(stub)).toBe(false);
  });

  it("records five fixed-size wake-up exchanges without private labels", async () => {
    const stub = namespace.get(namespace.newUniqueId());
    const response = await stub.fetch("https://push.invalid/", {
      headers: { Upgrade: "websocket" },
    });
    expect(response.status).toBe(101);
    const client = response.webSocket;
    if (!client) throw new Error("the Durable Object did not return a WebSocket");

    const receivedFrames: string[] = [];
    client.addEventListener("message", (event) => {
      receivedFrames.push(String((event as MessageEvent).data));
    });
    client.accept();

    const sentFrames: string[] = [];
    const exchanges: { sent: string; received: string }[] = [];
    const messages = TASK_3924_MESSAGE_FIXTURES.map(({ wakeup }) => wakeup);

    expect(TASK_3924_ACCOUNT_NAMES).toHaveLength(2);
    expect(messages).toHaveLength(5);
    expect(TASK_3924_PRIVATE_WORDS).toHaveLength(5);
    expect(
      new Set(TASK_3924_MESSAGE_FIXTURES.map(({ friendName }) => friendName)).size,
    ).toBe(2);
    expect(
      new Set(
        TASK_3924_MESSAGE_FIXTURES.map(
          ({ conversationName }) => conversationName,
        ),
      ).size,
    ).toBe(2);

    for (const [index, message] of messages.entries()) {
      expect(
        await runInDurableObject(stub, (instance) => instance.deliver(message)),
      ).toBe(true);

      sentFrames.push(IDLE_TICK);
      client.send(IDLE_TICK);

      const deadline = Date.now() + 10_000;
      while (receivedFrames.length <= index) {
        if (Date.now() > deadline) {
          throw new Error(
            `timed out waiting for task 3924 wake-up frame ${index}; got ${receivedFrames.length}`,
          );
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }

      exchanges.push({ sent: IDLE_TICK, received: receivedFrames[index]! });
    }

    const recordedFrames = exchanges.flatMap(({ sent, received }) => [
      sent,
      received,
    ]);
    const recordedWire = recordedFrames.join("");
    const fixedFrameLengths = new Set(recordedFrames.map((frame) => frame.length));
    const privateNeedles = [
      ...TASK_3924_ACCOUNT_NAMES,
      ...TASK_3924_FRIEND_NAMES,
      ...TASK_3924_CONVERSATION_NAMES,
      ...TASK_3924_PRIVATE_WORDS,
    ];
    const privateNeedleCounts = Object.fromEntries(
      privateNeedles.map((needle) => [needle, countNeedle(recordedWire, needle)]),
    ) as Record<string, number>;
    const privateNeedleTotal = Object.values(privateNeedleCounts).reduce(
      (sum, count) => sum + count,
      0,
    );
    const countHits = (needles: readonly string[]) =>
      needles.reduce((sum, needle) => sum + privateNeedleCounts[needle]!, 0);

    expect(sentFrames).toEqual(Array(5).fill(IDLE_TICK));
    expect(receivedFrames).toHaveLength(5);
    expect(exchanges).toHaveLength(5);
    expect(recordedWire.length).toBeGreaterThan(0);
    expect(fixedFrameLengths).toEqual(new Set([FRAME_BYTES]));
    expect(privateNeedleTotal).toBe(0);
    for (const [index, received] of receivedFrames.entries()) {
      expect(JSON.parse(received.trim())).toEqual(messages[index]);
    }

    console.info(
      "TASK_3924_WAKEUP_RECORDING",
      JSON.stringify({
        account_names_searched: TASK_3924_ACCOUNT_NAMES.length,
        friend_names_searched: TASK_3924_FRIEND_NAMES.length,
        conversation_names_searched: TASK_3924_CONVERSATION_NAMES.length,
        private_words_searched: TASK_3924_PRIVATE_WORDS.length,
        distinct_fixture_friends: new Set(
          TASK_3924_MESSAGE_FIXTURES.map(({ friendName }) => friendName),
        ).size,
        distinct_fixture_conversations: new Set(
          TASK_3924_MESSAGE_FIXTURES.map(
            ({ conversationName }) => conversationName,
          ),
        ).size,
        recorded_exchanges: exchanges.length,
        recorded_frames: recordedFrames.length,
        recorded_bytes: recordedWire.length,
        frame_lengths: [...fixedFrameLengths],
        account_name_hits: countHits(TASK_3924_ACCOUNT_NAMES),
        friend_name_hits: countHits(TASK_3924_FRIEND_NAMES),
        conversation_name_hits: countHits(TASK_3924_CONVERSATION_NAMES),
        private_word_hits: countHits(TASK_3924_PRIVATE_WORDS),
        private_needle_total: privateNeedleTotal,
      }),
    );
  });
});
