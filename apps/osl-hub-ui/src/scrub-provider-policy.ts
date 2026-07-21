export type ScrubReadMethod = "user_export" | "osl_owned_history" | "documented_provider_api";
export type ScrubDeleteMethod = "documented_provider_api" | "manual_jump";

export interface ScrubProviderCapability {
  serviceId: string;
  readMethod: ScrubReadMethod;
  deleteMethod: ScrubDeleteMethod;
  providerDocumentationUrl: string | null;
  userGrantedReadScope: boolean;
  userGrantedDeleteScope: boolean;
  publishedRateLimitEnforced: boolean;
}

export interface ScrubProviderDecision {
  scanAllowed: boolean;
  autoDeleteAllowed: boolean;
  manualJumpAllowed: boolean;
  reason: string;
}

/**
 * Scrub never drives provider UI, calls private endpoints, or disguises
 * automation. If documented authority is absent, it can only prepare a local
 * review list and a user-invoked jump target.
 */
export function decideScrubProviderCapability(capability: ScrubProviderCapability): ScrubProviderDecision {
  if (!validServiceId(capability.serviceId)) return blocked("Invalid provider identity");
  if (capability.readMethod === "documented_provider_api"
    && (!validHttpsUrl(capability.providerDocumentationUrl) || !capability.userGrantedReadScope)) {
    return blocked("Documented read authority is unavailable");
  }
  const scanAllowed = capability.readMethod === "user_export"
    || capability.readMethod === "osl_owned_history"
    || (capability.readMethod === "documented_provider_api" && capability.userGrantedReadScope);
  if (!scanAllowed) return blocked("No approved history source is available");

  const autoDeleteAllowed = capability.deleteMethod === "documented_provider_api"
    && validHttpsUrl(capability.providerDocumentationUrl)
    && capability.userGrantedDeleteScope
    && capability.publishedRateLimitEnforced;
  return {
    scanAllowed: true,
    autoDeleteAllowed,
    manualJumpAllowed: capability.deleteMethod === "manual_jump" || !autoDeleteAllowed,
    reason: autoDeleteAllowed
      ? "Documented provider deletion authority is available"
      : "AutoScrub is unavailable; use the local review list and manual jump",
  };
}

function blocked(reason: string): ScrubProviderDecision {
  return { scanAllowed: false, autoDeleteAllowed: false, manualJumpAllowed: false, reason };
}

function validServiceId(value: string): boolean {
  return /^[a-z0-9_-]{1,32}$/u.test(value);
}

function validHttpsUrl(value: string | null): boolean {
  if (value === null || value.length > 2_048) return false;
  try {
    return new URL(value).protocol === "https:";
  } catch {
    return false;
  }
}
