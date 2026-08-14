import { honestStateTone, type HonestState, type HonestStateTone } from "./honest-state";
import { statusTone, type StatusTone } from "./status-tone";

export type HomeProtectionState = {
  evidence: HonestState;
  label: string;
  honestTone: HonestStateTone;
  statusTone: StatusTone;
};

/**
 * Produces the state for a protection summary from the result of a capability
 * check. A missing check is deliberately distinct from a completed check that
 * found no capability.
 */
export function homeProtectionState(
  checked: boolean,
  enabled: boolean,
  labels: { enabled: string; unavailable: string },
): HomeProtectionState {
  if (!checked) {
    return {
      evidence: "unknown",
      label: "Not checked",
      honestTone: honestStateTone("unknown"),
      statusTone: statusTone(["unknown"]),
    };
  }

  if (enabled) {
    return {
      evidence: "confirmed",
      label: labels.enabled,
      honestTone: honestStateTone("confirmed"),
      statusTone: statusTone(["L1", "L2", "L3"]),
    };
  }

  return {
    evidence: "not-confirmed",
    label: labels.unavailable,
    honestTone: honestStateTone("not-confirmed"),
    statusTone: statusTone(["L1", "L2"]),
  };
}

export type HomeOverallStatusInput = {
  /** Core protection finished bootstrapping and is usable. */
  coreReady: boolean;
  /** Owner-facing sentence describing the core state when it is not ready. */
  coreDetail: string;
  /** Identity storage is confirmed protected on this device. */
  storageProtected: boolean;
  /** Owner-facing sentence for confirmed device protection. */
  storageDetail: string;
  /** Evidence state of the connected-apps check. */
  connectedApps: HomeProtectionState;
  pendingFriendReviews: number;
  verifiedFriends: number;
};

export type HomeOverallStatusState = "protected" | "not-checked" | "needs-attention";

export type HomeOverallStatus = {
  state: HomeOverallStatusState;
  headline: string;
  detail: string;
};

/**
 * The Home headline, derived from every check the screen itself reports.
 *
 * The first cross-cutting design rule is "never imply protection that isn't
 * there": "Protected" is a claim over everything Home lists, so it is only
 * allowed when every one of those checks has run and passed. A check that has
 * not run is not a passing check -- unknown is not protected -- and a failing
 * or unreviewed item names itself in the detail line instead of being averaged
 * away behind a reassuring default.
 */
export function homeOverallStatus(input: HomeOverallStatusInput): HomeOverallStatus {
  const problems: string[] = [];
  const unchecked: string[] = [];

  if (!input.coreReady) problems.push(input.coreDetail);
  else if (!input.storageProtected) problems.push("Device storage is not protected");

  if (input.connectedApps.evidence === "unknown") unchecked.push("Connected apps have not been checked");
  else if (input.connectedApps.evidence !== "confirmed") problems.push("No connected app is ready");

  if (input.pendingFriendReviews > 0) {
    problems.push(input.pendingFriendReviews === 1
      ? "1 person needs your review"
      : `${input.pendingFriendReviews.toLocaleString("en-US")} people need your review`);
  } else if (input.verifiedFriends === 0) {
    problems.push("No one is verified yet");
  }

  const gaps = [...problems, ...unchecked];
  if (gaps.length === 0) {
    return { state: "protected", headline: "Protected", detail: input.storageDetail };
  }
  if (problems.length === 0) {
    return { state: "not-checked", headline: "Not checked yet", detail: gaps.join(" · ") };
  }
  return {
    state: "needs-attention",
    headline: gaps.length === 1 ? "Needs attention" : `${gaps.length.toLocaleString("en-US")} things need your attention`,
    detail: gaps.join(" · "),
  };
}
