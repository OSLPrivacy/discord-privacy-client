import type { HubLicenseState } from "./core";

/** Semantic tier for the renderer; copy and controls remain T7's concern. */
export type EntitlementTier = HubLicenseState["access"];

/** A complete, non-copy-bearing entitlement state for T7 to render. */
export interface EntitlementView {
  tier: EntitlementTier;
  daysLeft: number | null;
  banner: "free" | "activationRequired" | "activationPending" | "active" | "offlineGrace" | "lapsed" | "accessUnavailable" | "accessUnknown";
  cta: "activate" | "wait" | "none" | "retry";
}

type ViewTemplate = Omit<EntitlementView, "tier" | "daysLeft">;

const VIEW_BY_STATUS: Readonly<Record<HubLicenseState["status"], ViewTemplate>> = {
  UNCONFIGURED: { banner: "free", cta: "activate" },
  UNREDEEMED: { banner: "activationRequired", cta: "activate" },
  PENDING: { banner: "activationPending", cta: "wait" },
  ACTIVE: { banner: "active", cta: "none" },
  CANCELLED: { banner: "active", cta: "none" },
  GRACE: { banner: "offlineGrace", cta: "none" },
  EXPIRED: { banner: "lapsed", cta: "activate" },
  REVOKED: { banner: "accessUnavailable", cta: "activate" },
  UNKNOWN: { banner: "accessUnknown", cta: "retry" },
};

function remainingDays(currentPeriodEnd: number | null, nowUnixSeconds: number, status: HubLicenseState["status"]): number | null {
  if (status === "EXPIRED") return 0;
  if (currentPeriodEnd === null) return null;
  return Math.max(0, Math.ceil((currentPeriodEnd - nowUnixSeconds) / 86_400));
}

/**
 * Projects the native entitlement DTO into one renderable state without DOM,
 * copy, or wall-clock reads. The caller supplies the clock so projections are
 * deterministic and testable.
 */
export function entitlementView(state: HubLicenseState, nowUnixSeconds: number): EntitlementView {
  return {
    tier: state.access,
    daysLeft: remainingDays(state.currentPeriodEnd, nowUnixSeconds, state.status),
    ...VIEW_BY_STATUS[state.status],
  };
}
