import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  acknowledgeOslMailRetrieval,
  burnOslMailbox,
  listOslMailThreads,
  loadOslMailStatus,
  provisionOslMail,
  retrieveOslMailThread,
  sendOslMail,
} from "./osl-mail-adapter";

const ID = "mail_abcdefghijklmnopqrstuvwxyz";
const RECEIPT = "a".repeat(64);

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
  mocks.invoke.mockImplementation(async (command: string) => {
    switch (command) {
      case "osl_mail_get_status":
      case "osl_mail_provision":
        return { available: true, provisioned: true, address: "member@oslprivacy.com", unreadCount: 0, retentionSeconds: 604_800 };
      case "osl_mail_list_threads":
        return [];
      case "osl_mail_retrieve_thread":
        return { threadId: ID, retrievalId: ID, expiresAt: 1, messages: [{ messageId: ID, from: "sender@oslprivacy.com", to: ["member@oslprivacy.com"], subject: "", body: "ciphertext opened locally", receivedAt: 1, transit: "oslE2ee" }] };
      case "osl_mail_acknowledge_retrieval":
        return { retrievalId: ID, deletedMessageIds: [ID], deletedAt: 1, receiptSha256: RECEIPT, serverDeleteConfirmed: true };
      case "osl_mail_send":
        return { clientMessageId: ID, acceptedAt: 1, recipient: "member@oslprivacy.com", transit: "oslE2ee", receiptSha256: RECEIPT };
      case "osl_mail_burn":
        return { address: "member@oslprivacy.com", burnedAt: 1, deletedMessages: 1, receiptSha256: RECEIPT, mailboxDisabled: true };
      default:
        throw new Error(`unexpected command: ${command}`);
    }
  });
});

describe("OSL Mail renderer IPC contract", () => {
  it("accepts the bridge's unprovisioned identity status instead of treating it as an error", async () => {
    mocks.invoke.mockResolvedValueOnce({
      available: true,
      provisioned: false,
      address: null,
      unreadCount: 0,
      retentionSeconds: 604_800,
    });

    await expect(loadOslMailStatus()).resolves.toEqual({
      available: true,
      provisioned: false,
      address: null,
      unreadCount: 0,
      retentionSeconds: 604_800,
    });
  });

  it("uses the seven frozen command names and camelCase argument shapes", async () => {
    await loadOslMailStatus();
    await provisionOslMail("member");
    await listOslMailThreads();
    await retrieveOslMailThread(ID);
    await acknowledgeOslMailRetrieval(ID, [ID]);
    await sendOslMail("member@oslprivacy.com", "subject", "body");
    await burnOslMailbox("member@oslprivacy.com", "member@oslprivacy.com");

    expect(mocks.invoke.mock.calls).toEqual([
      ["osl_mail_get_status", {}],
      ["osl_mail_provision", { username: "member" }],
      ["osl_mail_list_threads"],
      ["osl_mail_retrieve_thread", { threadId: ID }],
      ["osl_mail_acknowledge_retrieval", { retrievalId: ID, messageIds: [ID] }],
      ["osl_mail_send", { recipient: "member@oslprivacy.com", subject: "subject", body: "body" }],
      ["osl_mail_burn", { address: "member@oslprivacy.com", confirmation: "member@oslprivacy.com" }],
    ]);
  });
});
