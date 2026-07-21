import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";
import type { AutoScrubCapability, AutoScrubProviderBridge, AutoScrubProviderId } from "./autoscrub-flow";
import type { DeleteFinding, DeleteInspection, DeleteRequestResult, DeleteVerification, ScrubDeleteAdapter, StepUpProof } from "./scrub-delete-engine";

export interface ScrubImapConfiguration {
  accountId: string;
  host: string;
  port?: number;
  username: string;
  auth: { kind: "appPassword" | "oauthBearer"; secret: string };
  defaultMailbox?: string;
}

export interface ScrubImapConfigureResult { configured: boolean; liveConfirmed: boolean; authEpoch: string | null; detail: string }
export interface ScrubImapLocator { accountId: string; mailbox: string; messageId: string; sinceDate?: number }
interface ImapCapabilityResult { configured: boolean; liveConfirmed: boolean; authEpoch?: string | null; detail?: string }
interface ImapReauthResult { liveConfirmed: boolean; authEpoch: string | null; detail: string }
interface ImapEnumeration { findings: Array<{ uid: number; mailbox: string; messageId: string; authoredBySelf: boolean; contentFingerprint: string }>; authEpoch: string }

function desktopOnly(): void {
  if (!isTauriRuntime()) throw new Error("IMAP AutoScrub requires the OSL desktop app");
}

export async function configureScrubImapAccount(configuration: ScrubImapConfiguration): Promise<ScrubImapConfigureResult> {
  desktopOnly();
  return invoke<ScrubImapConfigureResult>("configure_scrub_imap_account", { request: { ...configuration } });
}

export async function getScrubImapCapability(accountId: string): Promise<ImapCapabilityResult> {
  if (!isTauriRuntime()) return { configured: false, liveConfirmed: false };
  return invoke<ImapCapabilityResult>("get_scrub_imap_capability", { request: { accountId } });
}

/** Resolve reviewed locators to transport-issued findings. Never derives a fingerprint from UI preview text. */
export async function prepareScrubImapFindings(locators: readonly ScrubImapLocator[], stepUp: StepUpProof): Promise<readonly DeleteFinding[]> {
  desktopOnly();
  if (stepUp.providerId !== "imap" || locators.some((locator) => locator.accountId !== stepUp.accountId)) throw new Error("IMAP prepare step-up does not match every locator");
  const batches = await Promise.all(locators.map(async (locator) => {
    const raw = await invoke<ImapEnumeration>("scrub_imap_enumerate", { request: { accountId: locator.accountId, expectedAuthEpoch: stepUp.authEpoch, mailbox: locator.mailbox, messageId: locator.messageId, sinceDateUnixMs: locator.sinceDate, expectedContentFingerprint: null } });
    if (raw.authEpoch !== stepUp.authEpoch) throw new Error("IMAP prepare authentication changed");
    return raw.findings.map((finding): DeleteFinding => ({ providerId: "imap", accountId: locator.accountId, channelId: finding.mailbox, correspondentId: finding.mailbox, itemId: finding.messageId, authoredBySelf: finding.authoredBySelf, createdAtUnixMs: locator.sinceDate ?? 0, contentFingerprint: finding.contentFingerprint }));
  }));
  const findings = batches.flat();
  if (findings.length !== locators.length) throw new Error("IMAP prepare did not uniquely resolve every reviewed message");
  for (const finding of findings) {
    const locator = locators.find((candidate) => candidate.accountId === finding.accountId && candidate.mailbox === finding.channelId && candidate.messageId === finding.itemId);
    if (!locator || finding.providerId !== "imap" || !finding.authoredBySelf || !finding.contentFingerprint) throw new Error("IMAP prepare returned an unsafe or mismatched finding");
  }
  return findings;
}

function operationArgs(finding: DeleteFinding, expectedAuthEpoch: string) {
  return {
    request: {
      accountId: finding.accountId,
      mailbox: finding.channelId,
      messageId: finding.itemId,
      sinceDateUnixMs: finding.createdAtUnixMs,
      expectedAuthEpoch,
      expectedContentFingerprint: finding.contentFingerprint,
    },
  };
}

class IpcImapDeleteAdapter implements ScrubDeleteAdapter {
  readonly #accountId: string;
  readonly #findings: readonly DeleteFinding[];
  readonly #authEpoch: string;
  constructor(accountId: string, findings: readonly DeleteFinding[], authEpoch: string) { this.#accountId = accountId; this.#findings = findings; this.#authEpoch = authEpoch; }
  async enumerate(scope: { accountId: string; channelIds: readonly string[]; beforeUnixMs: number }): Promise<readonly DeleteFinding[]> {
    if (scope.accountId !== this.#accountId) return [];
    const channels = new Set(scope.channelIds);
    return this.#findings.filter((finding) => channels.has(finding.channelId) && finding.createdAtUnixMs < scope.beforeUnixMs);
  }
  async inspect(finding: DeleteFinding): Promise<DeleteInspection> {
    return invoke<DeleteInspection>("scrub_imap_inspect", operationArgs(finding, this.#authEpoch));
  }
  async delete(finding: DeleteFinding): Promise<DeleteRequestResult> {
    return invoke<DeleteRequestResult>("scrub_imap_delete", operationArgs(finding, this.#authEpoch));
  }
  async verify(finding: DeleteFinding): Promise<DeleteVerification> {
    return invoke<DeleteVerification>("scrub_imap_verify", operationArgs(finding, this.#authEpoch));
  }
}

export function createDesktopAutoScrubBridge(accountIds: readonly string[]): AutoScrubProviderBridge {
  const uniqueAccountIds = [...new Set(accountIds)];
  return {
    async capabilities(): Promise<readonly AutoScrubCapability[]> {
      const imapStates = await Promise.all(uniqueAccountIds.map(async (accountId) => {
        const state = await getScrubImapCapability(accountId).catch(() => ({ configured: false, liveConfirmed: false }));
        return { accountId, ...state };
      }));
      // Capability is intentionally account-specific: never let account A activate account B.
      const liveConfirmed = imapStates.length === 1 && imapStates[0].configured && imapStates[0].liveConfirmed;
      return [
        { providerId: "imap", label: "Email (IMAP)", liveConfirmed, coverage: liveConfirmed ? "Message-ID deletion with provider readback" : "No live transport confirmed", unavailableReason: liveConfirmed ? undefined : "Connect and verify an IMAP account." },
        { providerId: "telegram", label: "Telegram", liveConfirmed: false, coverage: "Manual only", unavailableReason: "TDLib session and readback are not available in this build." },
        { providerId: "discord", label: "Discord", liveConfirmed: false, coverage: "Manual only", unavailableReason: "Hosted deletion is disabled and not live-verified." },
      ];
    },
    async adapter(providerId: AutoScrubProviderId, accountId: string, findings: readonly DeleteFinding[], stepUp: StepUpProof): Promise<ScrubDeleteAdapter> {
      if (providerId !== "imap") throw new Error("This provider has no live AutoScrub transport");
      desktopOnly();
      if (stepUp.providerId !== providerId || stepUp.accountId !== accountId || !stepUp.authEpoch) throw new Error("IMAP step-up is missing or mismatched");
      return new IpcImapDeleteAdapter(accountId, findings, stepUp.authEpoch);
    },
    async stepUp(providerId: AutoScrubProviderId, accountId: string): Promise<StepUpProof> {
      if (providerId !== "imap") throw new Error("This provider has no live AutoScrub transport");
      desktopOnly();
      const result = await invoke<ImapReauthResult>("reauth_scrub_imap_account", { request: { accountId } });
      if (!result.liveConfirmed || !result.authEpoch) throw new Error(result.detail || "IMAP re-authentication was not confirmed");
      const authenticatedAt = Date.now();
      return { providerId, accountId, authEpoch: result.authEpoch, authenticatedAt, expiresAt: authenticatedAt + 300_000 };
    },
  };
}
