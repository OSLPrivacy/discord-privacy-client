/**
 * T15-E5 — the decision the source device must obtain after a destination has
 * confirmed import. This is deliberately a small, side-effect-free state
 * machine: the transfer UI owns rendering and the backend owns any eventual
 * destructive operation, while neither gets to infer an owner's choice.
 */

export type OldDeviceCopyAction = "keep" | "destroy";

export interface OldDeviceCopyDecisionState {
  /** This screen cannot be reached merely because an export was made. */
  importConfirmed: boolean;
  /** `null` means the owner has not yet made the required choice. */
  choice: OldDeviceCopyAction | null;
}

export interface OldDeviceCopyDecisionOption {
  id: OldDeviceCopyAction;
  label: string;
  description: string;
}

export interface OldDeviceCopyDecisionView {
  mode: "choose" | "unavailable";
  question: string | null;
  options: OldDeviceCopyDecisionOption[];
}

export interface OldDeviceCopyDecisionTransition {
  state: OldDeviceCopyDecisionState;
  outcome: "none" | "rejected";
}

export type OldDeviceCopyDecisionCompletion =
  | { state: OldDeviceCopyDecisionState; outcome: "rejected" }
  | { state: OldDeviceCopyDecisionState; outcome: "complete"; choice: OldDeviceCopyAction };

const OPTIONS: readonly OldDeviceCopyDecisionOption[] = [
  {
    id: "keep",
    label: "Keep this device's copy",
    description: "Keep the transferred account data on this device as well as the new one.",
  },
  {
    id: "destroy",
    label: "Destroy this device's copy",
    description: "Remove this device's transferred account copy after the destination import is confirmed.",
  },
];

export function initialOldDeviceCopyDecision(
  input: Pick<OldDeviceCopyDecisionState, "importConfirmed">,
): OldDeviceCopyDecisionState {
  return { importConfirmed: input.importConfirmed, choice: null };
}

export function oldDeviceCopyDecisionView(
  state: OldDeviceCopyDecisionState,
): OldDeviceCopyDecisionView {
  if (!state.importConfirmed) return { mode: "unavailable", question: null, options: [] };
  return {
    mode: "choose",
    question: "The import is confirmed. What should happen to this device's copy?",
    options: [...OPTIONS],
  };
}

function isOldDeviceCopyAction(value: unknown): value is OldDeviceCopyAction {
  return value === "keep" || value === "destroy";
}

/**
 * Selecting either visible action is explicit. Unknown values (including a
 * stale UI value) leave the pending state unchanged rather than creating a
 * third path through the transfer.
 */
export function selectOldDeviceCopyAction(
  state: OldDeviceCopyDecisionState,
  action: unknown,
): OldDeviceCopyDecisionTransition {
  if (!state.importConfirmed || !isOldDeviceCopyAction(action)) return { state, outcome: "rejected" };
  return { state: { ...state, choice: action }, outcome: "none" };
}

/** The source flow may advance only after an owner selected one of the two actions. */
export function completeOldDeviceCopyDecision(
  state: OldDeviceCopyDecisionState,
): OldDeviceCopyDecisionCompletion {
  if (!state.importConfirmed || state.choice === null) return { state, outcome: "rejected" };
  return { state, outcome: "complete", choice: state.choice };
}
