export type CloudAutoScrubConsentTier = "none" | "scan_only" | "reviewed_delete";

export interface CloudAutoScrubConsentReceipt {
  tier: CloudAutoScrubConsentTier;
  consentReceiptSha256: string;
  consentedAtUnixMs: number;
  cloudContextVisible: true;
  notEndToEndPrivate: true;
  temporaryCredentialUse: true;
  perRunDisposableEnvironment: true;
}

export interface CloudAutoScrubRunBinding {
  runId: string;
  bindingSha256: string;
  accountBindingSha256: string;
  reviewedPlanSha256: string;
  expiresAtUnixMs: number;
}

export interface CloudAutoScrubNativeAuthority {
  runId: string;
  receiptSha256: string;
  attendedReviewComplete: true;
  reviewedItemsOnly: true;
  mintedByCloud: false;
  expiresAtUnixMs: number;
}

export interface CloudAutoScrubProviderAuthority {
  runId: string;
  grantReceiptSha256: string;
  documentedApiAuthority: true;
  userGrantedScope: true;
  publishedRateLimitBound: true;
  expiresAtUnixMs: number;
}

export interface CloudAutoScrubDisposableEnvironment {
  runId: string;
  launchBindingSha256: string;
  isolation: "per_run_disposable";
  maxTtlSeconds: number;
  wipeOnCompletion: true;
  wipeOnFailure: true;
  persistentStorage: false;
  retainCredentials: false;
  reusableWorker: false;
  plaintextCredentialIncluded: false;
}

export interface CloudAutoScrubProvisioningRequest {
  runId: string;
  nowUnixMs: number;
  consent: CloudAutoScrubConsentReceipt | null;
  binding: CloudAutoScrubRunBinding | null;
  nativeAuthority: CloudAutoScrubNativeAuthority | null;
  providerAuthority: CloudAutoScrubProviderAuthority | null;
  environment: CloudAutoScrubDisposableEnvironment | null;
}

export interface CloudAutoScrubProvisioningPlan {
  kind: "cloud-autoscrub-isolated-execution-plan";
  status: "provisionable";
  runId: string;
  consentTier: Exclude<CloudAutoScrubConsentTier, "none">;
  launchPinSha256: string;
  expiresAtUnixMs: number;
  retryPolicy: "never_auto_retry";
  receipts: {
    consentReceiptSha256: string;
    bindingSha256: string;
    nativeAuthorityReceiptSha256: string;
    providerAuthorityGrantSha256: string;
    launchBindingSha256: string;
  };
  environment: {
    isolation: "per_run_disposable";
    maxTtlSeconds: number;
    wipeOnCompletion: true;
    wipeOnFailure: true;
    persistentStorage: false;
    retainCredentials: false;
    reusableWorker: false;
  };
}

export type CloudAutoScrubProvisioningDecision =
  | {
    status: "refused";
    reason:
      | "invalid_run"
      | "missing_consent"
      | "insufficient_consent"
      | "missing_binding"
      | "invalid_binding"
      | "missing_native_authority"
      | "invalid_native_authority"
      | "missing_provider_authority"
      | "invalid_provider_authority"
      | "missing_environment"
      | "invalid_environment";
  }
  | { status: "provisionable"; plan: CloudAutoScrubProvisioningPlan };

const SHA256_RE = /^[0-9a-f]{64}$/u;
const RUN_ID_RE = /^run_[a-z0-9]{16,64}$/u;
const MAX_TTL_SECONDS = 2 * 60 * 60;

export function decideCloudAutoScrubProvisioning(
  request: CloudAutoScrubProvisioningRequest,
): CloudAutoScrubProvisioningDecision {
  if (!validRunId(request.runId) || !validUnixMs(request.nowUnixMs)) {
    return refused("invalid_run");
  }
  if (request.consent === null) return refused("missing_consent");
  if (!validConsent(request.consent, request.nowUnixMs)) {
    return refused("insufficient_consent");
  }
  if (request.binding === null) return refused("missing_binding");
  if (!validBinding(request.binding, request.runId, request.nowUnixMs)) {
    return refused("invalid_binding");
  }
  if (request.nativeAuthority === null) {
    return refused("missing_native_authority");
  }
  if (!validNativeAuthority(request.nativeAuthority, request.runId, request.nowUnixMs)) {
    return refused("invalid_native_authority");
  }
  if (request.providerAuthority === null) {
    return refused("missing_provider_authority");
  }
  if (!validProviderAuthority(request.providerAuthority, request.runId, request.nowUnixMs)) {
    return refused("invalid_provider_authority");
  }
  if (request.environment === null) return refused("missing_environment");
  if (!validEnvironment(request.environment, request.runId)) {
    return refused("invalid_environment");
  }

  const expiresAtUnixMs = Math.min(
    request.binding.expiresAtUnixMs,
    request.nativeAuthority.expiresAtUnixMs,
    request.providerAuthority.expiresAtUnixMs,
    request.nowUnixMs + request.environment.maxTtlSeconds * 1_000,
  );
  return {
    status: "provisionable",
    plan: {
      kind: "cloud-autoscrub-isolated-execution-plan",
      status: "provisionable",
      runId: request.runId,
      consentTier: request.consent.tier,
      launchPinSha256: request.environment.launchBindingSha256,
      expiresAtUnixMs,
      retryPolicy: "never_auto_retry",
      receipts: {
        consentReceiptSha256: request.consent.consentReceiptSha256,
        bindingSha256: request.binding.bindingSha256,
        nativeAuthorityReceiptSha256: request.nativeAuthority.receiptSha256,
        providerAuthorityGrantSha256: request.providerAuthority.grantReceiptSha256,
        launchBindingSha256: request.environment.launchBindingSha256,
      },
      environment: {
        isolation: "per_run_disposable",
        maxTtlSeconds: request.environment.maxTtlSeconds,
        wipeOnCompletion: true,
        wipeOnFailure: true,
        persistentStorage: false,
        retainCredentials: false,
        reusableWorker: false,
      },
    },
  };
}

function refused(reason: Extract<CloudAutoScrubProvisioningDecision, { status: "refused" }>["reason"]): CloudAutoScrubProvisioningDecision {
  return { status: "refused", reason };
}

function validConsent(consent: CloudAutoScrubConsentReceipt, nowUnixMs: number): consent is CloudAutoScrubConsentReceipt & { tier: Exclude<CloudAutoScrubConsentTier, "none"> } {
  return (consent.tier === "scan_only" || consent.tier === "reviewed_delete")
    && validSha256(consent.consentReceiptSha256)
    && validUnixMs(consent.consentedAtUnixMs)
    && consent.consentedAtUnixMs <= nowUnixMs
    && consent.cloudContextVisible === true
    && consent.notEndToEndPrivate === true
    && consent.temporaryCredentialUse === true
    && consent.perRunDisposableEnvironment === true;
}

function validBinding(binding: CloudAutoScrubRunBinding, runId: string, nowUnixMs: number): boolean {
  return binding.runId === runId
    && validSha256(binding.bindingSha256)
    && validSha256(binding.accountBindingSha256)
    && validSha256(binding.reviewedPlanSha256)
    && futureUnixMs(binding.expiresAtUnixMs, nowUnixMs);
}

function validNativeAuthority(authority: CloudAutoScrubNativeAuthority, runId: string, nowUnixMs: number): boolean {
  return authority.runId === runId
    && validSha256(authority.receiptSha256)
    && authority.attendedReviewComplete === true
    && authority.reviewedItemsOnly === true
    && authority.mintedByCloud === false
    && futureUnixMs(authority.expiresAtUnixMs, nowUnixMs);
}

function validProviderAuthority(authority: CloudAutoScrubProviderAuthority, runId: string, nowUnixMs: number): boolean {
  return authority.runId === runId
    && validSha256(authority.grantReceiptSha256)
    && authority.documentedApiAuthority === true
    && authority.userGrantedScope === true
    && authority.publishedRateLimitBound === true
    && futureUnixMs(authority.expiresAtUnixMs, nowUnixMs);
}

function validEnvironment(environment: CloudAutoScrubDisposableEnvironment, runId: string): boolean {
  return environment.runId === runId
    && validSha256(environment.launchBindingSha256)
    && environment.isolation === "per_run_disposable"
    && Number.isSafeInteger(environment.maxTtlSeconds)
    && environment.maxTtlSeconds > 0
    && environment.maxTtlSeconds <= MAX_TTL_SECONDS
    && environment.wipeOnCompletion === true
    && environment.wipeOnFailure === true
    && environment.persistentStorage === false
    && environment.retainCredentials === false
    && environment.reusableWorker === false
    && environment.plaintextCredentialIncluded === false;
}

function validRunId(value: string): boolean {
  return RUN_ID_RE.test(value);
}

function validSha256(value: string): boolean {
  return SHA256_RE.test(value);
}

function validUnixMs(value: number): boolean {
  return Number.isSafeInteger(value) && value > 0;
}

function futureUnixMs(value: number, nowUnixMs: number): boolean {
  return validUnixMs(value) && value > nowUnixMs;
}
