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
