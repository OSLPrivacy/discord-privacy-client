import { describe, expect, it, vi } from "vitest";
import {
  scrubImapDelete,
  scrubImapPrepareDelete,
  type NativeConfigureImapRequest,
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

const prepared: ScrubImapPreparedDelete = {
  ...request,
  preparedUid: 10,
  fingerprint: Array(32).fill(7),
  batchDigest: Array(32).fill(9),
};

function port(result: unknown): ScrubImapIpcPort {
  return { invoke: vi.fn().mockResolvedValue(result) };
}

describe("scrub IMAP IPC", () => {
  it("Define scrub-imap-ipc.ts wrapper types mirroring the native Imap struc", async () => {
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

    const adapter = port(prepared);
    await expect(scrubImapPrepareDelete(request, adapter)).resolves.toEqual(prepared);
    expect(adapter.invoke).toHaveBeenCalledWith("scrub_imap_prepare_delete", { request });
  });

  it("scrubImapPrepareDelete/scrubImapDelete TS wrappers", async () => {
    const preparePort = port(prepared);
    await expect(scrubImapPrepareDelete(request, preparePort)).resolves.toEqual(prepared);
    expect(preparePort.invoke).toHaveBeenCalledTimes(1);

    const deletePort = port({
      accountId: "acct-local-1",
      mailbox: "INBOX",
      messageId: "<msg-1@local.test>",
      deletedUid: 10,
    });
    await expect(scrubImapDelete(prepared, deletePort)).resolves.toEqual({
      accountId: "acct-local-1",
      mailbox: "INBOX",
      messageId: "<msg-1@local.test>",
      deletedUid: 10,
    });
    expect(deletePort.invoke).toHaveBeenCalledWith("scrub_imap_delete", { prepared });

    await expect(scrubImapPrepareDelete({ ...request, ownerOslUserId: "" }, preparePort))
      .rejects.toThrow("invalid scrub IMAP prepare delete request");
    expect(preparePort.invoke).toHaveBeenCalledTimes(1);

    await expect(scrubImapPrepareDelete(request, port({ ...prepared, fingerprint: Array(31).fill(7) })))
      .rejects.toThrow("invalid scrub IMAP prepared delete response");
    await expect(scrubImapDelete(prepared, port({ ...prepared, deletedUid: 11 })))
      .rejects.toThrow("invalid scrub IMAP delete receipt");
  });
});
