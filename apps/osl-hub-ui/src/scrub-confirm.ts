import { findingsFingerprint, planFingerprint, type ExecutionConsent, type ScopePolicy, type StepUpProof } from "./scrub-delete-engine";

const STEP_UP_MAX_AGE_MS = 300_000;

/** The exact native ConsentLedger binding presented when spending a grant. */
export interface ScrubConsentBinding {
  providerId: string;
  accountId: string;
  scope: string;
}

/**
 * Thin IPC-shaped boundary over the native `ConsentLedger`; this module keeps
 * no client-side ledger and cannot mint or replay a consent grant.
 */
export interface NativeConsentLedger {
  consume(consentId: string, binding: ScrubConsentBinding, now: number): Promise<boolean>;
}

export interface IrreversibleScrubRequest {
  approved: ScopePolicy;
  findings: readonly { providerId: string; accountId: string; channelId: string; correspondentId: string; itemId: string; authoredBySelf: boolean; createdAtUnixMs: number; contentFingerprint: string }[];
  confirmation: string;
  consent: ExecutionConsent;
  stepUp: StepUpProof;
  now: number;
}

export type IrreversibleScrubAuthorization =
  | { authorized: true; consent: ExecutionConsent }
  | { authorized: false; reason: "typed-confirmation" | "step-up" | "consent" };

export function scrubConfirmationPhrase(approved: ScopePolicy, findings: IrreversibleScrubRequest["findings"]): string {
  return `DELETE ${approved.providerId}/${approved.accountId} ${findingsFingerprint(findings)}`;
}

function consentBinding(approved: ScopePolicy, findings: IrreversibleScrubRequest["findings"]): ScrubConsentBinding {
  return { providerId: approved.providerId, accountId: approved.accountId, scope: `${planFingerprint(approved)}:${findingsFingerprint(findings)}` };
}

function hasFreshBoundStepUp(request: IrreversibleScrubRequest): boolean {
  const { approved, stepUp, now } = request;
  return stepUp.providerId === approved.providerId && stepUp.accountId === approved.accountId
    && stepUp.authenticatedAt <= now && stepUp.expiresAt >= now
    && now - stepUp.authenticatedAt <= STEP_UP_MAX_AGE_MS;
}

/**
 * Authorizes one irreversible batch. Validation precedes native spend, so a
 * mistyped phrase or stale step-up cannot consume a usable grant.
 */
export async function authorizeIrreversibleScrub(request: IrreversibleScrubRequest, ledger: NativeConsentLedger): Promise<IrreversibleScrubAuthorization> {
  if (request.confirmation !== scrubConfirmationPhrase(request.approved, request.findings)) return { authorized: false, reason: "typed-confirmation" };
  if (!hasFreshBoundStepUp(request)) return { authorized: false, reason: "step-up" };
  if (request.consent.planFingerprint !== planFingerprint(request.approved) || request.consent.findingsFingerprint !== findingsFingerprint(request.findings)
    || request.consent.issuedAt > request.now || request.consent.expiresAt < request.now
    || !await ledger.consume(request.consent.id, consentBinding(request.approved, request.findings), request.now)) return { authorized: false, reason: "consent" };
  return { authorized: true, consent: request.consent };
}
