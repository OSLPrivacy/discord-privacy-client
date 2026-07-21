import {
  executeDeletion,
  findingsFingerprint,
  planFingerprint,
  scopeOnlyAllows,
  type DeleteFinding,
  type ProviderDeletionReceipt,
  type ScopePolicy,
  type ScrubDeleteAdapter,
  type StepUpProof,
} from "./scrub-delete-engine";

export type AutoScrubProviderId = "imap" | "telegram" | "discord";

export interface AutoScrubCapability {
  providerId: AutoScrubProviderId;
  label: string;
  liveConfirmed: boolean;
  coverage: string;
  unavailableReason?: string;
}

export interface AutoScrubBatch {
  providerId: AutoScrubProviderId;
  accountId: string;
  findings: readonly DeleteFinding[];
  approved: ScopePolicy;
  requested: ScopePolicy;
}

export interface AutoScrubProviderBridge {
  capabilities(): Promise<readonly AutoScrubCapability[]>;
  adapter(providerId: AutoScrubProviderId, accountId: string, findings: readonly DeleteFinding[], stepUp: StepUpProof): Promise<ScrubDeleteAdapter>;
  stepUp(providerId: AutoScrubProviderId, accountId: string): Promise<StepUpProof>;
}

export interface AutoScrubRunOptions {
  target: { providerId: AutoScrubProviderId; accountId: string };
  prepare: (stepUp: StepUpProof) => Promise<AutoScrubBatch>;
  capability: AutoScrubCapability;
  bridge: AutoScrubProviderBridge;
  finalConfirmation: boolean;
  onDryRun: (receipt: ProviderDeletionReceipt) => void | Promise<void>;
  now?: () => number;
}

export interface AutoScrubRunResult {
  dryRun: ProviderDeletionReceipt;
  execution: ProviderDeletionReceipt;
}

function consentId(now: number): string {
  return `autoscrub-${now.toString(36)}`;
}

function assertPreviewSafe(receipt: ProviderDeletionReceipt): void {
  if (!receipt.dryRun || receipt.stoppedFailClosed || receipt.items.some((item) => item.deletionCalled || item.verifiedByReadback || item.outcome !== "UNKNOWN")) {
    throw new Error("AutoScrub dry-run receipt was not preview-safe");
  }
}

/** Runs one reviewed batch. The bridge is deliberately delete-only and provider-specific. */
export async function runAutoScrubBatch(options: AutoScrubRunOptions): Promise<AutoScrubRunResult> {
  const { target, capability, bridge } = options;
  if (!capability.liveConfirmed || capability.providerId !== target.providerId) throw new Error("AutoScrub transport is not live-confirmed");
  if (!options.finalConfirmation) throw new Error("AutoScrub requires explicit final confirmation");
  const stepUp = await bridge.stepUp(target.providerId, target.accountId);
  const now = options.now?.() ?? Date.now();
  if (stepUp.providerId !== target.providerId || stepUp.accountId !== target.accountId || stepUp.authenticatedAt > now || stepUp.expiresAt < now || now - stepUp.authenticatedAt > 300_000) {
    throw new Error("AutoScrub re-authentication is not fresh");
  }
  const batch = await options.prepare(stepUp);
  if (batch.providerId !== target.providerId || batch.accountId !== target.accountId || batch.approved.providerId !== target.providerId || batch.approved.accountId !== target.accountId || !scopeOnlyAllows(batch.approved, batch.requested)) throw new Error("AutoScrub scope may only shrink after review");
  const adapter = await bridge.adapter(batch.providerId, batch.accountId, batch.findings, stepUp);
  const consent = {
    id: consentId(now),
    planFingerprint: planFingerprint(batch.approved),
    findingsFingerprint: findingsFingerprint(batch.findings),
    issuedAt: now,
    expiresAt: Math.min(now + 300_000, stepUp.expiresAt),
  };
  const common = {
    adapter,
    findings: batch.findings,
    approved: batch.approved,
    requested: batch.requested,
    consent,
    stepUp,
    finalConfirmation: true,
    now,
  };
  const dryRun = await executeDeletion({ ...common, dryRun: true });
  assertPreviewSafe(dryRun);
  await options.onDryRun(dryRun);
  const execution = await executeDeletion({ ...common, dryRun: false });
  return { dryRun, execution };
}

export interface AutoScrubReceiptSummary {
  heading: string;
  detail: string;
  verifiedDeleted: number;
  confirmedPresent: number;
  unknown: number;
}

export function summarizeAutoScrubReceipt(receipt: ProviderDeletionReceipt): AutoScrubReceiptSummary {
  const verifiedDeleted = receipt.items.filter((item) => item.outcome === "confirmed-deleted" && item.verifiedByReadback).length;
  const confirmedPresent = receipt.items.filter((item) => item.outcome === "confirmed-not-deleted" && item.verifiedByReadback).length;
  const unknown = receipt.items.length - verifiedDeleted - confirmedPresent;
  const heading = receipt.dryRun ? "Dry-run preview" : verifiedDeleted > 0 ? "Verified within stated coverage" : "No deletion was verified";
  const detail = receipt.dryRun
    ? `${receipt.items.length} selected; no deletion was called.`
    : `${verifiedDeleted} verified absent by provider readback · ${confirmedPresent} verified still present · ${unknown} unknown`;
  return { heading, detail, verifiedDeleted, confirmedPresent, unknown };
}

export const unavailableAutoScrubCapabilities: readonly AutoScrubCapability[] = [
  { providerId: "imap", label: "Email (IMAP)", liveConfirmed: false, coverage: "No live transport confirmed", unavailableReason: "Connect and verify an IMAP account in the desktop app." },
  { providerId: "telegram", label: "Telegram", liveConfirmed: false, coverage: "Manual only", unavailableReason: "TDLib session and readback are not available in this build." },
  { providerId: "discord", label: "Discord", liveConfirmed: false, coverage: "Manual only", unavailableReason: "Hosted deletion is not enabled or live-verified." },
] as const;
