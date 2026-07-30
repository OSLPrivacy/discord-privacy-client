import { beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: () => true }));

import {
  OSL_MAIL_STATUS_CONTRACT,
  acknowledgeOslMailRetrieval,
  burnOslMailbox,
  listOslMailThreads,
  loadOslMailStatus,
  parseOslMailDeleteReceipt,
  parseOslMailRetrievedThread,
  parseOslMailSendReceipt,
  parseOslMailStatus,
  registerOslMailStatusAdapterTests,
  sendOslMail,
  type OslMailAddress,
  type OslMailProvisionedStatus,
  type OslMailStatus,
  type OslMailUnprovisionedStatus,
} from "./osl-mail-adapter";

const id = "abcdefghijkl";
const hash = "a".repeat(64);

registerOslMailStatusAdapterTests({ describe, expect, expectTypeOf, it });

describe("OSL Mail strict adapter", () => {
  beforeEach(() => invoke.mockReset());

  it("publishes the OSL Mail status DTO contract", () => {
    expect(OSL_MAIL_STATUS_CONTRACT).toEqual({
      maximumUnreadCount: 100_000,
      minimumRetentionSeconds: 60,
      maximumRetentionSeconds: 604_800,
    });
    expectTypeOf<OslMailStatus>().toEqualTypeOf<OslMailProvisionedStatus | OslMailUnprovisionedStatus>();
    expectTypeOf<OslMailProvisionedStatus["address"]>().toEqualTypeOf<OslMailAddress>();
    expectTypeOf<OslMailUnprovisionedStatus["address"]>().toEqualTypeOf<null>();
    expectTypeOf<OslMailUnprovisionedStatus["unreadCount"]>().toEqualTypeOf<0>();
  });

  it("accepts only an exact provisioned status", () => {
    expect(parseOslMailStatus({ available: true, provisioned: true, address: "liam@oslprivacy.com", unreadCount: 2, retentionSeconds: 3600 })?.address).toBe("liam@oslprivacy.com");
    expect(parseOslMailStatus({ available: true, provisioned: true, address: null, unreadCount: 0, retentionSeconds: 3600 })).toBeNull();
    expect(parseOslMailStatus({ available: true, provisioned: false, address: null, unreadCount: 0, retentionSeconds: 3600, extra: true })).toBeNull();
    expect(parseOslMailStatus({ available: true, provisioned: true, address: "liam@oslprivacy.com", unreadCount: OSL_MAIL_STATUS_CONTRACT.maximumUnreadCount, retentionSeconds: OSL_MAIL_STATUS_CONTRACT.maximumRetentionSeconds })).not.toBeNull();
    expect(parseOslMailStatus({ available: true, provisioned: true, address: "liam@oslprivacy.com", unreadCount: OSL_MAIL_STATUS_CONTRACT.maximumUnreadCount + 1, retentionSeconds: 3600 })).toBeNull();
    expect(parseOslMailStatus({ available: true, provisioned: true, address: "liam@oslprivacy.com", unreadCount: 0, retentionSeconds: OSL_MAIL_STATUS_CONTRACT.minimumRetentionSeconds - 1 })).toBeNull();
  });

  it("models unprovisioned status as no mailbox and no unread mail", async () => {
    const unprovisioned = parseOslMailStatus({ available: true, provisioned: false, address: null, unreadCount: 0, retentionSeconds: 3600 });
    expect(unprovisioned).toMatchObject({ provisioned: false, address: null, unreadCount: 0 });
    expect(parseOslMailStatus({ available: true, provisioned: false, address: "liam@oslprivacy.com", unreadCount: 0, retentionSeconds: 3600 })).toBeNull();
    expect(parseOslMailStatus({ available: true, provisioned: false, address: null, unreadCount: 1, retentionSeconds: 3600 })).toBeNull();
    invoke.mockResolvedValueOnce({ available: true, provisioned: false, address: null, unreadCount: 1, retentionSeconds: 3600 });
    await expect(loadOslMailStatus()).resolves.toBeNull();
  });

  it("strictly parses retrieved thread and message responses", () => {
    const valid = { threadId: id, retrievalId: `${id}x`, expiresAt: 100, messages: [{ messageId: `${id}m`, from: "a@example.com", to: ["liam@oslprivacy.com"], subject: "Hello", body: "Body", receivedAt: 90, transit: "externalSmtp" }] };
    expect(parseOslMailRetrievedThread(valid)?.messages).toHaveLength(1);
    expect(parseOslMailRetrievedThread({ ...valid, messages: [] })).toBeNull();
    expect(parseOslMailRetrievedThread({ ...valid, messages: [{ ...valid.messages[0], body: "bad\0body" }] })).toBeNull();
    expect(parseOslMailRetrievedThread({ ...valid, messages: [{ ...valid.messages[0], to: [] }] })).toBeNull();
    expect(parseOslMailRetrievedThread({ ...valid, messages: [{ ...valid.messages[0], transit: "smtp" }] })).toBeNull();
    expect(parseOslMailRetrievedThread({ ...valid, messages: [{ ...valid.messages[0], unexpected: true }] })).toBeNull();
    expect(parseOslMailRetrievedThread({ ...valid, unexpected: true })).toBeNull();
  });

  it("requires an explicit positive server deletion receipt", () => {
    expect(parseOslMailDeleteReceipt({ retrievalId: id, deletedMessageIds: [`${id}m`], deletedAt: 100, receiptSha256: hash, serverDeleteConfirmed: true })).not.toBeNull();
    expect(parseOslMailDeleteReceipt({ retrievalId: id, deletedMessageIds: [`${id}m`], deletedAt: 100, receiptSha256: hash, serverDeleteConfirmed: false })).toBeNull();
  });

  it("accepts only exact OSL E2EE send receipts", () => {
    const valid = { clientMessageId: id, acceptedAt: 100, recipient: "friend@oslprivacy.com", transit: "oslE2ee", receiptSha256: hash };
    expect(parseOslMailSendReceipt(valid)).toEqual(valid);
    expect(parseOslMailSendReceipt({ ...valid, recipient: "outside@example.com" })).toBeNull();
    expect(parseOslMailSendReceipt({ ...valid, transit: "externalSmtp" })).toBeNull();
    expect(parseOslMailSendReceipt({ ...valid, receiptSha256: hash.toUpperCase() })).toBeNull();
    expect(parseOslMailSendReceipt({ ...valid, authority: "external-outbound" })).toBeNull();
  });

  it("never invokes external outbound and accepts only OSL E2EE receipts", async () => {
    await expect(sendOslMail("outside@example.com", "Hi", "Body")).resolves.toBeNull();
    expect(invoke).not.toHaveBeenCalled();
    invoke.mockResolvedValueOnce({ clientMessageId: id, acceptedAt: 100, recipient: "friend@oslprivacy.com", transit: "externalSmtp", receiptSha256: hash });
    await expect(sendOslMail("friend@oslprivacy.com", "Hi", "Body")).resolves.toBeNull();
    invoke.mockResolvedValueOnce({ clientMessageId: id, acceptedAt: 100, recipient: "friend@oslprivacy.com", transit: "oslE2ee", receiptSha256: hash });
    await expect(sendOslMail("friend@oslprivacy.com", "Hi", "Body")).resolves.toMatchObject({ transit: "oslE2ee" });
  });

  it("fails closed on invalid lists, acknowledgments, and burn confirmations", async () => {
    invoke.mockResolvedValueOnce([{ threadId: id, subject: "Hi", correspondent: "a@example.com", latestAt: 100, unread: false, transit: "externalSmtp", extra: true }]);
    await expect(listOslMailThreads()).resolves.toBeNull();
    await expect(acknowledgeOslMailRetrieval(id, [])).resolves.toBeNull();
    await expect(burnOslMailbox("liam@oslprivacy.com", "wrong@oslprivacy.com")).resolves.toBeNull();
  });
});
