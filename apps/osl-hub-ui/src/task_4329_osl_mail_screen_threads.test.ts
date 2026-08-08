import { beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { listOslMailThreads, retrieveOslMailThread } from "./osl-mail-adapter";
import { oslMailViewMarkup } from "./osl-mail-view";

const THREADS = ["thread4329aa", "thread4329ab", "thread4329ac", "thread4329ad"].map((threadId, index) => ({
  threadId,
  subject: `TASK4329 subject ${index + 1}`,
  correspondent: `sender${index + 1}@example.com`,
  latestAt: 1_970_000_000 + index,
  unread: false,
  transit: "oslE2ee" as const,
}));

const EXACT_WORDS = "TASK4329 exact words from the selected real thread.";
const MAIN_SOURCE = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionBody(name: string, nextName: string): string {
  const start = MAIN_SOURCE.indexOf(`async function ${name}`);
  const end = MAIN_SOURCE.indexOf(`async function ${nextName}`, start);
  expect(start, `${name} should remain a real screen handoff`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should delimit ${name}`).toBeGreaterThan(start);
  return MAIN_SOURCE.slice(start, end);
}

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
  mocks.invoke.mockImplementation(async (command: string, args?: { threadId?: string }) => {
    if (command === "osl_mail_list_threads") return THREADS;
    if (command === "osl_mail_retrieve_thread") {
      expect(args).toEqual({ threadId: THREADS[2].threadId });
      return {
        threadId: THREADS[2].threadId,
        retrievalId: "retrieve4329a",
        expiresAt: 1_970_000_600,
        messages: [{
          messageId: "message4329aa",
          from: THREADS[2].correspondent,
          to: ["member@oslprivacy.com"],
          subject: THREADS[2].subject,
          body: EXACT_WORDS,
          receivedAt: 1_970_000_002,
          transit: "oslE2ee",
        }],
      };
    }
    throw new Error(`unexpected command: ${command}`);
  });
});

describe("TASK 4329 OSL Mail reading screen", () => {
  it("removes all three deliberate empty screen handoffs", () => {
    const refresh = functionBody("refreshOslMail", "provisionOslMailFromProfile");
    const provision = functionBody("provisionOslMailFromProfile", "openOslMailThread");
    const open = functionBody("openOslMailThread", "sendOslMailForm");
    const emptyScreenHandoffs = [
      !refresh.includes("const threads = await listOslMailThreads();"),
      !provision.includes("await refreshOslMail();"),
      !open.includes("await retrieveOslMailThread(threadId)"),
    ].filter(Boolean).length;

    console.log(`TASK4329 deliberate_empty_screen_handoffs=${emptyScreenHandoffs} unread_count=0 waiting_on=4503`);
    expect(emptyScreenHandoffs).toBe(0);
  });

  it("renders all four server threads and the selected thread's exact words", async () => {
    const threads = await listOslMailThreads();
    const selected = await retrieveOslMailThread(THREADS[2].threadId);
    expect(selected?.messages.map((message) => message.body)).toEqual([EXACT_WORDS]);

    const markup = oslMailViewMarkup({
      loading: false,
      available: true,
      signedUsername: "member",
      status: { available: true, provisioned: true, address: "member@oslprivacy.com", unreadCount: 0, retentionSeconds: 604_800 },
      threads: threads ?? [],
      activeThread: selected,
      pane: "inbox",
      notifications: true,
      deleteReceipt: null,
      sendReceipt: null,
      burnReceipt: null,
      error: null,
      threadSyncUnavailable: false,
    });

    const renderedRows = markup.match(/data-mail-thread=/gu)?.length ?? 0;
    console.log(`TASK4329 mailbox_threads=${THREADS.length} rendered_rows=${renderedRows} selected_words=${selected?.messages[0]?.body} unread_count=0`);
    expect(renderedRows).toBe(4);
    expect(THREADS.every((thread) => markup.includes(thread.subject) && markup.includes(thread.correspondent))).toBe(true);
    expect(markup).toContain(EXACT_WORDS);
    expect(mocks.invoke.mock.calls).toEqual([
      ["osl_mail_list_threads"],
      ["osl_mail_retrieve_thread", { threadId: THREADS[2].threadId }],
    ]);
  });
});
