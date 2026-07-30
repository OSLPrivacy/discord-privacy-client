import { describe, expect, it, vi } from "vitest";
import {
  configureScrubImapAccount,
  getScrubImapCapability,
  reauthScrubImapAccount,
  type ScrubImapAccountConfig,
  type ScrubImapCapability,
  type ScrubImapPort,
} from "./scrub-imap";

const scopeFingerprint = "a".repeat(64);
const capability: ScrubImapCapability = {
  accountId: "mail-main",
  capabilityId: "b".repeat(32),
  scopeFingerprint,
  authEpoch: 3,
  canScan: true,
  canDelete: false,
  providerAccess: "imap_read_only",
  liveAuthEpochRequired: true,
};

const config: ScrubImapAccountConfig = {
  accountId: "mail-main",
  emailAddress: "person@example.com",
  serverHost: "imap.example.com",
  serverPort: 993,
  tlsMode: "tls",
  credentialRef: "secret_store_slot",
  scopeFingerprint,
  liveAuthEpoch: 3,
};

function port(overrides: Partial<ScrubImapPort> = {}): ScrubImapPort {
  return {
    configure: vi.fn().mockResolvedValue(capability),
    capability: vi.fn().mockResolvedValue(capability),
    reauth: vi.fn().mockResolvedValue({ accountId: "mail-main", authEpoch: 4 }),
    ...overrides,
  };
}

describe("scrub IMAP", () => {
  it("configureScrubImapAccount/getScrubImapCapability/reauthScrubImapAccount", async () => {
    const adapter = port();

    await expect(configureScrubImapAccount(config, adapter)).resolves.toEqual(capability);
    await expect(getScrubImapCapability("mail-main", scopeFingerprint, 3, adapter)).resolves.toEqual(capability);
    await expect(reauthScrubImapAccount({
      accountId: "mail-main",
      previousAuthEpoch: 3,
      proofNonce: "c".repeat(64),
    }, adapter)).resolves.toBe(4);

    expect(adapter.configure).toHaveBeenCalledWith(config);
    expect(adapter.capability).toHaveBeenCalledWith("mail-main", scopeFingerprint, 3);
    expect(adapter.reauth).toHaveBeenCalledWith({
      accountId: "mail-main",
      previousAuthEpoch: 3,
      proofNonce: "c".repeat(64),
    });

    await expect(configureScrubImapAccount({ ...config, liveAuthEpoch: 0 }, adapter))
      .rejects.toThrow("invalid scrub IMAP account configuration");
    expect(adapter.configure).toHaveBeenCalledTimes(1);

    await expect(getScrubImapCapability("mail-main", scopeFingerprint, 4, adapter))
      .rejects.toThrow("invalid scrub IMAP capability");

    const staleReauth = port({ reauth: vi.fn().mockResolvedValue({ accountId: "mail-main", authEpoch: 3 }) });
    await expect(reauthScrubImapAccount({
      accountId: "mail-main",
      previousAuthEpoch: 3,
      proofNonce: "d".repeat(64),
    }, staleReauth)).rejects.toThrow("invalid scrub IMAP reauthorization response");
  });
});
