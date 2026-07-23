import { describe, expect, it } from "vitest";
import { decideScrubProviderCapability, type ScrubProviderCapability } from "./scrub-provider-policy";

describe("Scrub provider policy", () => {
  const exportOnly: ScrubProviderCapability = {
    serviceId: "discord",
    readMethod: "user_export",
    deleteMethod: "manual_jump",
    providerDocumentationUrl: null,
    userGrantedReadScope: false,
    userGrantedDeleteScope: false,
    publishedRateLimitEnforced: false,
  };

  it("allows local export scanning but never converts it into automated deletion", () => {
    expect(decideScrubProviderCapability(exportOnly)).toEqual({
      scanAllowed: true,
      autoDeleteAllowed: false,
      manualJumpAllowed: true,
      reason: "AutoScrub is unavailable; use the local review list and manual jump",
    });
  });

  it("requires documented API authority, explicit delete scope, and provider rate limits", () => {
    const official: ScrubProviderCapability = {
      ...exportOnly,
      readMethod: "documented_provider_api",
      deleteMethod: "documented_provider_api",
      providerDocumentationUrl: "https://provider.example/developer/messages",
      userGrantedReadScope: true,
      userGrantedDeleteScope: true,
      publishedRateLimitEnforced: true,
    };
    expect(decideScrubProviderCapability(official).autoDeleteAllowed).toBe(true);
    expect(decideScrubProviderCapability({ ...official, userGrantedDeleteScope: false }).autoDeleteAllowed).toBe(false);
    expect(decideScrubProviderCapability({ ...official, publishedRateLimitEnforced: false }).autoDeleteAllowed).toBe(false);
    expect(decideScrubProviderCapability({ ...official, providerDocumentationUrl: null }).scanAllowed).toBe(false);
  });
});
