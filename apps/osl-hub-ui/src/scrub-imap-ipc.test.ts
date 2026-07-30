import { describe, expect, it, vi } from "vitest";
import {
  scrubImapDelete,
  scrubImapPrepareDelete,
  type NativeImapDeleteReceipt,
  type NativeImapMessageSnapshot,
  type NativePreparedImapDelete,
  type ScrubImapDeletePort,
  type ScrubImapPrepareDeleteRequest,
} from "./scrub-imap-ipc";

const request: ScrubImapPrepareDeleteRequest = {
  ownerOslUserId: "owner-local-1",
  accountId: "account-a",
  mailbox: "INBOX",
  messageId: "message-0001",
};

const fingerprint = Array.from({ length: 32 }, (_, index) => index);
const batchDigest = Array.from({ length: 32 }, (_, index) => 255 - index);

const prepared: NativePreparedImapDelete = {
  ...request,
  preparedUid: 42,
  fingerprint,
  batchDigest,
};

const receipt: NativeImapDeleteReceipt = {
  accountId: "account-a",
  mailbox: "INBOX",
  messageId: "message-0001",
  deletedUid: 42,
};

function port(overrides: Partial<ScrubImapDeletePort> = {}): ScrubImapDeletePort {
  return {
    prepareDelete: vi.fn().mockResolvedValue(prepared),
    delete: vi.fn().mockResolvedValue(receipt),
    ...overrides,
  };
}

describe("scrub IMAP IPC", () => {
  it("Define scrub-imap-ipc.ts wrapper types mirroring the native Imap struc", async () => {
    const nativeMessage: NativeImapMessageSnapshot = {
      ...request,
      uid: 42,
      fingerprint,
      authoredBySelf: true,
    };
    expect(nativeMessage).toEqual({
      ownerOslUserId: "owner-local-1",
      accountId: "account-a",
      mailbox: "INBOX",
      messageId: "message-0001",
      uid: 42,
      fingerprint,
      authoredBySelf: true,
    });

    await expect(scrubImapPrepareDelete(request, port())).resolves.toEqual(prepared);
    await expect(scrubImapPrepareDelete(request, port({
      prepareDelete: vi.fn().mockResolvedValue({ ...prepared, fingerprint: fingerprint.slice(1) }),
    }))).rejects.toThrow("invalid scrub IMAP prepared delete");
  });

  it("scrubImapPrepareDelete/scrubImapDelete TS wrappers", async () => {
    const adapter = port();

    await expect(scrubImapPrepareDelete(request, adapter)).resolves.toEqual(prepared);
    expect(adapter.prepareDelete).toHaveBeenCalledWith(request);

    await expect(scrubImapDelete(prepared, adapter)).resolves.toEqual(receipt);
    expect(adapter.delete).toHaveBeenCalledWith(prepared);

    await expect(scrubImapPrepareDelete({ ...request, ownerOslUserId: "" }, adapter))
      .rejects.toThrow("invalid scrub IMAP prepare-delete request");
    expect(adapter.prepareDelete).toHaveBeenCalledTimes(1);

    await expect(scrubImapDelete(prepared, port({
      delete: vi.fn().mockResolvedValue({ ...receipt, deletedUid: 43 }),
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
      await expect(scrubImapDelete(prepared, port({
        delete: vi.fn().mockResolvedValue(malformed),
      }))).rejects.toThrow("invalid scrub IMAP delete receipt");
    }
  });
});
