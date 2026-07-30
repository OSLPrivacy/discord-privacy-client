import { describe, expect, it, vi } from "vitest";
import {
  scrubImapDelete,
  scrubImapPrepareDelete,
  type NativeConfigureImapRequest,
  type NativeImapDeleteReceipt,
  type NativeImapMessageSnapshot,
  type NativePreparedImapDelete,
  type ScrubImapDeletePort,
  type ScrubImapIpcPort,
  type ScrubImapPrepareDeleteRequest,
  type ScrubImapPreparedDelete,
} from "./scrub-imap-ipc";

const request: ScrubImapPrepareDeleteRequest = {
  ownerOslUserId: "owner-local-1",
  accountId: "acct-local-1",
  mailbox: "INBOX",
  messageId: "<msg-1@local.test>",
};

const fingerprint = Array.from({ length: 32 }, (_, index) => index);
const batchDigest = Array.from({ length: 32 }, (_, index) => 255 - index);

const prepared: NativePreparedImapDelete = {
  ...request,
  preparedUid: 10,
  fingerprint,
  batchDigest,
};

const receipt: NativeImapDeleteReceipt = {
  accountId: "acct-local-1",
  mailbox: "INBOX",
  messageId: "<msg-1@local.test>",
  deletedUid: 10,
};

function methodPort(overrides: Partial<ScrubImapDeletePort> = {}): ScrubImapDeletePort {
  return {
    prepareDelete: vi.fn().mockResolvedValue(prepared),
    delete: vi.fn().mockResolvedValue(receipt),
    ...overrides,
  };
}

function invokePort(result: unknown): ScrubImapIpcPort {
  return { invoke: vi.fn().mockResolvedValue(result) };
}

describe("scrub IMAP IPC", () => {
  it("defines native scrub IMAP structures and configure auth variants", async () => {
    const nativeMessage: NativeImapMessageSnapshot = {
      ...request,
      uid: 10,
      fingerprint,
      authoredBySelf: true,
    };
    expect(nativeMessage).toEqual({
      ownerOslUserId: "owner-local-1",
      accountId: "acct-local-1",
      mailbox: "INBOX",
      messageId: "<msg-1@local.test>",
      uid: 10,
      fingerprint,
      authoredBySelf: true,
    });

    const passwordConfig = {
      ownerOslUserId: "owner-local-1",
      accountId: "acct-local-1",
      host: "imap.local.test",
      port: 993,
      tlsRequired: true,
      auth: { password: { username: "user@local.test", password: "local-test-password" } },
    } satisfies NativeConfigureImapRequest;
    const oauthConfig = {
      ownerOslUserId: "owner-local-1",
      accountId: "acct-local-1",
      host: "imap.local.test",
      port: 993,
      tlsRequired: true,
      auth: { oAuthBearer: { username: "user@local.test", bearerToken: "local-test-token" } },
    } satisfies NativeConfigureImapRequest;
    expect(passwordConfig).toMatchObject({
      ownerOslUserId: "owner-local-1",
      tlsRequired: true,
      auth: { password: { username: "user@local.test" } },
    });
    expect(oauthConfig.auth).toHaveProperty("oAuthBearer");

    await expect(scrubImapPrepareDelete(request, methodPort())).resolves.toEqual(prepared);
    await expect(scrubImapPrepareDelete(request, methodPort({
      prepareDelete: vi.fn().mockResolvedValue({ ...prepared, fingerprint: fingerprint.slice(1) }),
    }))).rejects.toThrow("invalid scrub IMAP prepared delete response");
  });

  it("calls Tauri-style invoke ports for prepare and delete", async () => {
    const preparePort = invokePort(prepared);
    await expect(scrubImapPrepareDelete(request, preparePort)).resolves.toEqual(prepared);
    expect(preparePort.invoke).toHaveBeenCalledWith("scrub_imap_prepare_delete", { request });

    const deletePort = invokePort(receipt);
    await expect(scrubImapDelete(prepared, deletePort)).resolves.toEqual(receipt);
    expect(deletePort.invoke).toHaveBeenCalledWith("scrub_imap_delete", { prepared });
  });

  it("validates request, prepared delete and receipt bindings", async () => {
    const adapter = methodPort();

    await expect(scrubImapPrepareDelete({ ...request, ownerOslUserId: "" }, adapter))
      .rejects.toThrow("invalid scrub IMAP prepare delete request");
    expect(adapter.prepareDelete).not.toHaveBeenCalled();

    await expect(scrubImapPrepareDelete(request, invokePort({
      ...prepared,
      fingerprint: Array(31).fill(7),
    }))).rejects.toThrow("invalid scrub IMAP prepared delete response");

    await expect(scrubImapDelete({
      ...prepared,
      batchDigest: Array(31).fill(9),
    } satisfies ScrubImapPreparedDelete, adapter)).rejects.toThrow("invalid scrub IMAP prepared delete");
    expect(adapter.delete).not.toHaveBeenCalled();

    await expect(scrubImapDelete(prepared, methodPort({
      delete: vi.fn().mockResolvedValue({ ...receipt, deletedUid: 11 }),
    }))).rejects.toThrow("invalid scrub IMAP delete receipt");
  });

  it("rejects malformed native delete receipts before trusting IPC data", async () => {
    const malformedReceipts: unknown[] = [
      { mailbox: "INBOX", messageId: "message-0001", deletedUid: 42 },
      { accountId: "account-a", messageId: "message-0001", deletedUid: 42 },
      { accountId: "account-a", mailbox: "INBOX", deletedUid: 42 },
      { accountId: "account-a", mailbox: "INBOX", messageId: "message-0001" },
      { accountId: "", mailbox: "INBOX", messageId: "message-0001", deletedUid: 42 },
      { accountId: "account-a", mailbox: "INBOX", messageId: "message-0001", deletedUid: "42" },
      { accountId: "account-a", mailbox: "INBOX", messageId: "message-0001", deletedUid: 42, extra: true },
    ];

    for (const malformed of malformedReceipts) {
      await expect(scrubImapDelete(prepared, methodPort({
        delete: vi.fn().mockResolvedValue(malformed),
      }))).rejects.toThrow("invalid scrub IMAP delete receipt");
    }
  });
});
