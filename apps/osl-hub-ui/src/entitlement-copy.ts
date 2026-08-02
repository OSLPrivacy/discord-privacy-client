import type { EntitlementView } from "./entitlement-view";

/** User-facing text selected from the entitlement view-model by T7. */
export interface EntitlementCopy {
  title: string;
  detail: string;
}

const FREE_FLOOR = "Free OSL keeps working: encrypted messages, sending, receiving, and the word-bank carrier.";

function activeCopy(daysLeft: number | null): EntitlementCopy {
  if (daysLeft === null) {
    return { title: "Pro access is active", detail: "Pro features are available on this device." };
  }
  if (daysLeft === 0) {
    return { title: "Pro access ends today", detail: "When Pro access ends, Free OSL keeps working." };
  }
  const dayLabel = daysLeft === 1 ? "day" : "days";
  return {
    title: `Pro access ends in ${daysLeft} ${dayLabel}`,
    detail: "When Pro access ends, Free OSL keeps working.",
  };
}

/**
 * Supplies complete, prepaid-entitlement copy without inferring payment or
 * storage behaviour. The lapsed state explicitly preserves the Free floor.
 */
export function entitlementCopy(view: EntitlementView): EntitlementCopy {
  switch (view.banner) {
    case "active":
      return activeCopy(view.daysLeft);
    case "lapsed":
      return { title: "Pro access has ended", detail: FREE_FLOOR };
    case "offlineGrace":
      return { title: "Pro access is available offline", detail: "Pro features remain available while this device reconnects." };
    case "free":
      return { title: "Free OSL", detail: FREE_FLOOR };
    case "activationRequired":
      return { title: "Activate Pro", detail: "Enter an activation code to use Pro features." };
    case "activationPending":
      return { title: "Checking activation", detail: "Pro access will appear here when the code is ready." };
    case "accessUnavailable":
      return { title: "Pro access is unavailable", detail: FREE_FLOOR };
    case "accessUnknown":
      return { title: "Pro access could not be checked", detail: FREE_FLOOR };
  }
}
