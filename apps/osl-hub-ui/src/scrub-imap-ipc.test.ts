import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: () => true }));

import { configureScrubImapAccount, createDesktopAutoScrubBridge, prepareScrubImapFindings } from "./scrub-imap-ipc";
import type { DeleteFinding } from "./scrub-delete-engine";

const proof = { providerId: "imap", accountId: "mail", authEpoch: "epoch-2", authenticatedAt: 10, expiresAt: 1000 } as const;
const finding: DeleteFinding = { providerId: "imap", accountId: "mail", channelId: "Sent", correspondentId: "Sent", itemId: "m@example.test", authoredBySelf: true, createdAtUnixMs: 12, contentFingerprint: "sha256:abc" };

describe("IMAP AutoScrub IPC contract", () => {
  beforeEach(() => invoke.mockReset());

  it("nests configuration and never persists or logs the credential in the UI bridge", async () => {
    invoke.mockResolvedValue({ configured: true, liveConfirmed: true, authEpoch: "epoch-1", detail: "ok" });
    await configureScrubImapAccount({ accountId: "mail", host: "imap.example.test", username: "me", auth: { kind: "appPassword", secret: "secret" }, defaultMailbox: "Sent" });
    expect(invoke).toHaveBeenCalledWith("configure_scrub_imap_account", { request: { accountId: "mail", host: "imap.example.test", username: "me", auth: { kind: "appPassword", secret: "secret" }, defaultMailbox: "Sent" } });
  });

  it("binds enumerate and every operation to the same fresh epoch", async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === "scrub_imap_enumerate") return { findings: [{ uid: 7, mailbox: "Sent", messageId: "m@example.test", authoredBySelf: true, contentFingerprint: "sha256:abc" }], authEpoch: "epoch-2" };
      if (command === "scrub_imap_inspect") return { state: "present", authoredBySelf: true, contentFingerprint: "sha256:abc", authEpoch: "epoch-2", schemaVersion: "imap-v1", retractable: true, detail: "present" };
      return { outcome: "confirmed-deleted", authEpoch: "epoch-2", detail: "absent" };
    });
    const prepared = await prepareScrubImapFindings([{ accountId: "mail", mailbox: "Sent", messageId: "m@example.test", sinceDate: 12 }], proof);
    const adapter = await createDesktopAutoScrubBridge(["mail"]).adapter("imap", "mail", prepared, proof);
    await adapter.inspect(prepared[0]);
    await expect(adapter.delete(prepared[0])).rejects.toThrow("one-shot reviewed consent");
    await adapter.verify(prepared[0]);
    expect(prepared).toEqual([finding]);
    for (const call of invoke.mock.calls) {
      expect(call[1].request.expectedAuthEpoch).toBe("epoch-2");
    }
    expect(invoke.mock.calls.some(([command]) => command === "scrub_imap_delete")).toBe(false);
    expect(invoke.mock.calls[0][1]).toEqual({ request: { accountId: "mail", expectedAuthEpoch: "epoch-2", mailbox: "Sent", messageId: "m@example.test", sinceDateUnixMs: 12, expectedContentFingerprint: null } });
  });

  it("never activates one selected account from another account's capability", async () => {
    invoke.mockResolvedValue({ configured: true, liveConfirmed: true, authEpoch: null, detail: "ok" });
    const single = await createDesktopAutoScrubBridge(["mail-a"]).capabilities();
    const ambiguous = await createDesktopAutoScrubBridge(["mail-a", "mail-b"]).capabilities();
    expect(single.find((capability) => capability.providerId === "imap")?.liveConfirmed).toBe(false);
    expect(single.find((capability) => capability.providerId === "imap")?.coverage).toContain("Read-only");
    expect(ambiguous.find((capability) => capability.providerId === "imap")?.liveConfirmed).toBe(false);
    expect(single[0]).toMatchObject({ providerId: "gmail-web", primary: true, liveConfirmed: false });
    expect(invoke).toHaveBeenCalledWith("get_scrub_imap_capability", { request: { accountId: "mail-a" } });
  });
});
