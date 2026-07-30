import { describe, expect, it } from "vitest";
import {
  decideCloudAutoScrubProvisioning,
  type CloudAutoScrubProvisioningRequest,
} from "./cloud-autoscrub-provisioning";

const NOW = 1_785_300_000_000;
const RUN_ID = "run_0123456789abcdef";
const A = "a".repeat(64);
const B = "b".repeat(64);
const C = "c".repeat(64);
const D = "d".repeat(64);
const E = "e".repeat(64);
const F = "f".repeat(64);

function validRequest(): CloudAutoScrubProvisioningRequest {
  return {
    runId: RUN_ID,
    nowUnixMs: NOW,
    consent: {
      tier: "reviewed_delete",
      consentReceiptSha256: A,
      consentedAtUnixMs: NOW - 1_000,
      cloudContextVisible: true,
      notEndToEndPrivate: true,
      temporaryCredentialUse: true,
      perRunDisposableEnvironment: true,
    },
    binding: {
      runId: RUN_ID,
      bindingSha256: B,
      accountBindingSha256: C,
      reviewedPlanSha256: D,
      expiresAtUnixMs: NOW + 60_000,
    },
    nativeAuthority: {
      runId: RUN_ID,
      receiptSha256: E,
      attendedReviewComplete: true,
      reviewedItemsOnly: true,
      mintedByCloud: false,
      expiresAtUnixMs: NOW + 120_000,
    },
    providerAuthority: {
      runId: RUN_ID,
      grantReceiptSha256: F,
      documentedApiAuthority: true,
      userGrantedScope: true,
      publishedRateLimitBound: true,
      expiresAtUnixMs: NOW + 180_000,
    },
    environment: {
      runId: RUN_ID,
      launchBindingSha256: "1".repeat(64),
      isolation: "per_run_disposable",
      maxTtlSeconds: 3_600,
      wipeOnCompletion: true,
      wipeOnFailure: true,
      persistentStorage: false,
      retainCredentials: false,
      reusableWorker: false,
      plaintextCredentialIncluded: false,
    },
  };
}

describe("cloud AutoScrub isolated environment provisioning", () => {
  it("refuses when consent, binding, or authority is absent", () => {
    expect(decideCloudAutoScrubProvisioning({ ...validRequest(), consent: null })).toEqual({
      status: "refused",
      reason: "missing_consent",
    });
    expect(decideCloudAutoScrubProvisioning({ ...validRequest(), binding: null })).toEqual({
      status: "refused",
      reason: "missing_binding",
    });
    expect(decideCloudAutoScrubProvisioning({ ...validRequest(), nativeAuthority: null })).toEqual({
      status: "refused",
      reason: "missing_native_authority",
    });
    expect(decideCloudAutoScrubProvisioning({ ...validRequest(), providerAuthority: null })).toEqual({
      status: "refused",
      reason: "missing_provider_authority",
    });
  });

  it("requires honest high-sensitivity cloud consent", () => {
    const missingTruth = validRequest();
    missingTruth.consent = {
      ...missingTruth.consent!,
      notEndToEndPrivate: false as true,
    };
    expect(decideCloudAutoScrubProvisioning(missingTruth)).toEqual({
      status: "refused",
      reason: "insufficient_consent",
    });

    const noConsentTier = validRequest();
    noConsentTier.consent = {
      ...noConsentTier.consent!,
      tier: "none",
    };
    expect(decideCloudAutoScrubProvisioning(noConsentTier)).toEqual({
      status: "refused",
      reason: "insufficient_consent",
    });
  });

  it("refuses reusable workers, retained credentials, durable state, and stale bindings", () => {
    const reusable = validRequest();
    reusable.environment = { ...reusable.environment!, reusableWorker: true as false };
    expect(decideCloudAutoScrubProvisioning(reusable)).toEqual({
      status: "refused",
      reason: "invalid_environment",
    });

    const retainedCredential = validRequest();
    retainedCredential.environment = { ...retainedCredential.environment!, retainCredentials: true as false };
    expect(decideCloudAutoScrubProvisioning(retainedCredential)).toEqual({
      status: "refused",
      reason: "invalid_environment",
    });

    const persistent = validRequest();
    persistent.environment = { ...persistent.environment!, persistentStorage: true as false };
    expect(decideCloudAutoScrubProvisioning(persistent)).toEqual({
      status: "refused",
      reason: "invalid_environment",
    });

    const stale = validRequest();
    stale.binding = { ...stale.binding!, expiresAtUnixMs: NOW };
    expect(decideCloudAutoScrubProvisioning(stale)).toEqual({
      status: "refused",
      reason: "invalid_binding",
    });
  });

  it("returns a sanitized one-run provisioning plan with no automatic retry", () => {
    const first = decideCloudAutoScrubProvisioning(validRequest());
    const second = decideCloudAutoScrubProvisioning(validRequest());
    expect(first).toEqual(second);
    expect(first.status).toBe("provisionable");
    if (first.status !== "provisionable") throw new Error("expected plan");
    expect(first.plan).toMatchObject({
      kind: "cloud-autoscrub-isolated-execution-plan",
      status: "provisionable",
      runId: RUN_ID,
      consentTier: "reviewed_delete",
      expiresAtUnixMs: NOW + 60_000,
      retryPolicy: "never_auto_retry",
      environment: {
        isolation: "per_run_disposable",
        maxTtlSeconds: 3_600,
        wipeOnCompletion: true,
        wipeOnFailure: true,
        persistentStorage: false,
        retainCredentials: false,
        reusableWorker: false,
      },
    });
    expect(first.plan.launchPinSha256).toMatch(/^[0-9a-f]{64}$/u);
    expect(JSON.stringify(first.plan)).not.toContain("account-");
    expect(JSON.stringify(first.plan)).not.toContain("credential");
  });
});
