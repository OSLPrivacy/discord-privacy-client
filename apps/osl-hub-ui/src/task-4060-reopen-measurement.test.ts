import { describe, expect, it } from "vitest";
import type { OslChatHistoryRow } from "./adapters";
import {
  createOslChatDeliveryRuntime,
  type OslChatDeliveryContext,
  type OslChatDeliveryHost,
  type OslChatDeliveryPerson,
} from "./osl-chat-runtime";
import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";

const PERSON_ID = "task-4060-peer";
const PEER_OSL_USER_ID = "OSLUSER-task-4060-peer";
const DEADLINE_SECONDS = 12;

interface PlannedMessage {
  name: string;
  body: string;
  availableAtSeconds: number;
  appearedAtSeconds: number | null;
  delivered: boolean;
}

interface Measurement {
  label: string;
  count: number;
  seconds: string;
  never: string;
}

function batch(messages: readonly PlannedMessage[], nowSeconds: number): NativeDiscordOverlayOpenedBatch {
  return {
    messages: messages.map((message, index) => ({
      messageId: `task-4060-${message.name}-${index.toString().padStart(2, "0")}`,
      plaintext: message.body,
      contextVerified: true,
      personToPersonE2ee: true,
      viewOnceConsumed: false,
      createdAt: 1_700_000_000 + nowSeconds,
      expiresAt: 4_000_000_000,
    })),
    pendingViewOnce: [],
    acknowledgments: [],
    fetched: messages.length,
    decryptDisplayEnabled: true,
    deferredRows: 0,
    unrecognizedWireRows: 0,
    contentGoneRows: 0,
  };
}

class Task4060Host implements OslChatDeliveryHost {
  readonly people: OslChatDeliveryPerson[] = [{
    personId: PERSON_ID,
    safetyNumberVerified: true,
    pendingKeyChange: false,
  }];
  readonly context: OslChatDeliveryContext = {
    personId: PERSON_ID,
    peerOslUserId: PEER_OSL_USER_ID,
    scopeApproved: true,
  };
  readonly messages: PlannedMessage[];
  nowSeconds = 0;
  open = true;
  readonly stubReceivingJob: boolean;

  constructor(label: string, availableAtSeconds: readonly number[], stubReceivingJob: boolean) {
    this.stubReceivingJob = stubReceivingJob;
    this.messages = availableAtSeconds.map((seconds, index) => ({
      name: `${label}-message-${index + 1}`,
      body: `TASK4060 ${label} private message ${index + 1}`,
      availableAtSeconds: seconds,
      appearedAtSeconds: null,
      delivered: false,
    }));
  }

  identityLoaded(): boolean {
    return true;
  }

  foreignContextActive(): boolean {
    return false;
  }

  openConversationId(): string | null {
    return this.open ? PERSON_ID : null;
  }

  conversationBusy(): boolean {
    return false;
  }

  friends(): readonly OslChatDeliveryPerson[] {
    return this.people;
  }

  async requestCaptureProtection(): Promise<boolean> {
    return true;
  }

  async activateContext(personId: string): Promise<OslChatDeliveryContext | null> {
    return personId === PERSON_ID ? this.context : null;
  }

  async closeContext(): Promise<boolean> {
    return true;
  }

  async drainInbox(): Promise<NativeDiscordOverlayOpenedBatch> {
    if (this.stubReceivingJob) return batch([], this.nowSeconds);
    const ready = this.messages.filter((message) => (
      !message.delivered && message.availableAtSeconds <= this.nowSeconds
    ));
    for (const message of ready) message.delivered = true;
    return batch(ready, this.nowSeconds);
  }

  async loadHistory(): Promise<OslChatHistoryRow[]> {
    return [];
  }

  commitBatch(_personId: string, opened: NativeDiscordOverlayOpenedBatch): void {
    for (const openedMessage of opened.messages) {
      const planned = this.messages.find((message) => message.body === openedMessage.plaintext);
      if (planned && planned.appearedAtSeconds === null) {
        planned.appearedAtSeconds = this.nowSeconds;
      }
    }
  }

  commitHistory(): void {
  }
}

async function measureAlreadyOpen(
  label: string,
  availableAtSeconds: readonly number[],
  stubReceivingJob: boolean,
): Promise<Measurement> {
  const host = new Task4060Host(label, availableAtSeconds, stubReceivingJob);
  host.open = true;
  const runtime = createOslChatDeliveryRuntime(host);
  for (let second = 0; second <= DEADLINE_SECONDS; second += 1) {
    host.nowSeconds = second;
    await runtime.sync();
  }
  return measurement(label, host.messages);
}

async function measureClosedThenReopened(
  label: string,
  availableAtSeconds: readonly number[],
  reopenAtSeconds: number,
  stubReceivingJob: boolean,
): Promise<Measurement> {
  const host = new Task4060Host(label, availableAtSeconds, stubReceivingJob);
  host.open = false;
  const runtime = createOslChatDeliveryRuntime(host);
  for (let second = 0; second < reopenAtSeconds; second += 1) {
    host.nowSeconds = second;
  }
  host.open = true;
  for (let second = reopenAtSeconds; second <= DEADLINE_SECONDS; second += 1) {
    host.nowSeconds = second;
    await runtime.sync();
  }
  return measurement(label, host.messages);
}

function measurement(label: string, messages: readonly PlannedMessage[]): Measurement {
  return {
    label,
    count: messages.filter((message) => message.appearedAtSeconds !== null).length,
    seconds: messages
      .map((message) => `${message.name}:${message.appearedAtSeconds ?? "never"}`)
      .join(","),
    never: messages
      .filter((message) => message.appearedAtSeconds === null)
      .map((message) => message.name)
      .join(",") || "none",
  };
}

function printMeasurement(prefix: string, run: Measurement): void {
  console.log(`${prefix}_count=${run.count}`);
  console.log(`${prefix}_seconds=${run.seconds}`);
  console.log(`${prefix}_never=${run.never}`);
}

describe("TASK 4060 closed and reopened OSL Chat measurement", () => {
  it("prints the control and three reopen measurements", async () => {
    const stubReceivingJob = process.env.TASK4060_STUB_RECEIVING_JOB === "1";
    if (stubReceivingJob) console.log("TASK4060_STUBBED_RECEIVING_JOB=do_nothing");

    const control = await measureAlreadyOpen("control-open", [1, 2, 3], stubReceivingJob);
    printMeasurement("TASK4060_CONTROL_ALREADY_OPEN", control);
    expect(control.count, `control run expected all 3 but got ${control.count}; never=${control.never}`).toBe(3);

    const runs = [
      await measureClosedThenReopened("reopen-run-1", [1, 2, 3], 4, stubReceivingJob),
      await measureClosedThenReopened("reopen-run-2", [2, 4, 6], 7, stubReceivingJob),
      await measureClosedThenReopened("reopen-run-3", [1, 5, 8], 3, stubReceivingJob),
    ];

    runs.forEach((run, index) => {
      printMeasurement(`TASK4060_REOPEN_RUN_${index + 1}`, run);
      expect(run.count, `${run.label} expected all 3 but got ${run.count}; never=${run.never}`).toBe(3);
    });
  });
});
