/**
 * T15-A7 — the recovery-kit screen, as a state machine.
 *
 * Two defects are fixed here and both are about honesty:
 *
 * 1. **The refusal was a dead end.** When capture resistance could not be
 *    proven, onboarding rendered one "Retry protection" button and nothing
 *    else. Refusing to show a secret the owner can never see again is a worse
 *    failure than showing it on a screen that might be captured, so the
 *    refusal now carries three exits — retry, show anyway behind a typed
 *    acknowledgement, and remind me later — and none of them destroys the
 *    phrases the user has not saved yet.
 *
 * 2. **The protection claim was false off Windows.** `SetWindowDisplayAffinity`
 *    exists only on Windows; every other platform's `apply_to_hwnd` is a no-op
 *    that returns `Ok(())`, so the gate "passed" and the copy claimed capture
 *    resistance over a completely unprotected window. The claim is now derived
 *    from whether the platform actually enforces protection, not from whether
 *    the call returned success.
 *
 * Nothing in this module writes anything anywhere. Secrets live in the state
 * object the caller holds in memory; the only thing that survives a restart is
 * the boolean "the kit was never saved" flag owned by `onboarding-resume.ts`.
 */

export interface RecoveryKitSecrets {
  userId: string;
  identityPhrase: string | null;
  passwordPhrase: string;
}

/**
 * Whether the running platform actually applies capture resistance.
 * `"unenforced"` is the truth on Linux and macOS, where the primitive is a
 * compile-time no-op.
 */
export type CaptureEnforcement = "enforced" | "unenforced";

export interface RecoveryKitState {
  /** In-memory only. Never persisted, never written to disk. */
  secrets: RecoveryKitSecrets | null;
  /** The capture-protection latch was accepted for the current generation. */
  captureProven: boolean;
  captureEnforcement: CaptureEnforcement;
  /** The owner typed the acknowledgement and asked to see the kit anyway. */
  shownWithoutProtection: boolean;
  /** The owner ticked "I saved my recovery kit". */
  savedAcknowledged: boolean;
  /** A kit exists for this account that the owner has never confirmed saving. */
  kitUnsaved: boolean;
}

export type RecoveryKitMode =
  /** Secrets are on screen. */
  | "kit"
  /** Secrets are held back, but there is a way forward. */
  | "refusal"
  /** The secrets are no longer in memory and must be re-read from the backend. */
  | "reveal-required"
  /** Nothing to show and nothing outstanding. */
  | "unavailable";

export type RecoveryKitExitId =
  | "retry-protection"
  | "show-anyway"
  | "remind-me-later"
  | "reveal-with-password";

export interface RecoveryKitExit {
  id: RecoveryKitExitId;
  label: string;
  requiresAcknowledgement: boolean;
}

export type RecoveryProtectionClaim = "capture-resistant" | "not-capture-resistant";

export interface RecoveryKitView {
  mode: RecoveryKitMode;
  secretsVisible: boolean;
  claim: RecoveryProtectionClaim;
  notice: string;
  exits: RecoveryKitExit[];
  acknowledgementPrompt: string | null;
}

export const RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT = "show anyway";

export const RECOVERY_CAPTURE_UNENFORCED_NOTICE =
  "This build cannot make any window capture-resistant. Windows display affinity is the only capture protection OSL has, and it does not exist on this platform. Treat everything on this screen as capturable.";

export const RECOVERY_CAPTURE_UNPROVEN_NOTICE =
  "OSL could not prove Windows capture resistance for this window. Your recovery secrets are being held back until it can, but you can still choose to see them.";

export const RECOVERY_CAPTURE_PROVEN_NOTICE =
  "Windows capture resistance is applied to this window while it stays focused. A camera pointed at the screen still works.";

export const RECOVERY_SHOWN_UNPROTECTED_NOTICE =
  "You chose to see these secrets without proven capture resistance. Anything that can read this screen can read them.";

export const RECOVERY_REVEAL_REQUIRED_NOTICE =
  "You never confirmed saving your recovery kit. Enter your password to read it again — OSL kept it encrypted, it was never written down for you.";

/**
 * The recovery phrases protect different things. Keep this copy with the kit
 * renderer so every place that shows both phrases makes that distinction.
 */
export function recoveryKitSecretCardsMarkup(
  secrets: RecoveryKitSecrets,
  escape: (value: string) => string,
): string {
  const identityPhrase = secrets.identityPhrase
    ? `<code>${escape(secrets.identityPhrase)}</code>`
    : "<p>Keep using the identity phrase you imported.</p>";

  return `<article class="recovery-kit-item" data-recovery-secret="identity"><span aria-hidden="true">1</span><div><strong>Identity phrase</strong><p>Your identity phrase brings back who you are.</p>${identityPhrase}</div></article><article class="recovery-kit-item" data-recovery-secret="password"><span aria-hidden="true">2</span><div><strong>Password phrase</strong><p>Your password phrase brings back your data.</p><code>${escape(secrets.passwordPhrase)}</code></div></article><p class="recovery-kit-requirement">You need both.</p>`;
}

export function initialRecoveryKitState(
  secrets: RecoveryKitSecrets | null,
  kitUnsaved: boolean,
): RecoveryKitState {
  return {
    secrets,
    captureProven: false,
    captureEnforcement: "unenforced",
    shownWithoutProtection: false,
    savedAcknowledged: false,
    kitUnsaved,
  };
}

export function acknowledgementAccepted(typed: string): boolean {
  return typed.trim().toLowerCase() === RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT;
}

function claimFor(state: RecoveryKitState): RecoveryProtectionClaim {
  return state.captureProven && state.captureEnforcement === "enforced"
    ? "capture-resistant"
    : "not-capture-resistant";
}

function noticeFor(state: RecoveryKitState, mode: RecoveryKitMode): string {
  if (mode === "reveal-required") return RECOVERY_REVEAL_REQUIRED_NOTICE;
  if (state.captureEnforcement === "unenforced") return RECOVERY_CAPTURE_UNENFORCED_NOTICE;
  if (state.captureProven) return RECOVERY_CAPTURE_PROVEN_NOTICE;
  if (mode === "kit") return RECOVERY_SHOWN_UNPROTECTED_NOTICE;
  return RECOVERY_CAPTURE_UNPROVEN_NOTICE;
}

function modeFor(state: RecoveryKitState): RecoveryKitMode {
  if (!state.secrets) return state.kitUnsaved ? "reveal-required" : "unavailable";
  if (state.captureProven || state.shownWithoutProtection) return "kit";
  return "refusal";
}

/**
 * The exits. The whole point of A7 is that this list is never one entry that
 * can fail: a user whose window cannot be proven protected must still be able
 * to reach their phrases or defer without losing them.
 *
 * `reveal-required` originally shipped as `["reveal-with-password"]` alone,
 * which re-created the dead end A7 removed one screen over. That screen is
 * reached on *every* launch after "Remind me later", and its single control
 * depends on a backend round trip; when the round trip did not come back there
 * was nothing else on the screen and the app could not be reached at all. The
 * deferral exit is therefore unconditional here, and the renderer must keep it
 * usable while a reveal is in flight — it is the escape of last resort, so it
 * may never be gated on the thing that is failing.
 */
function exitsFor(mode: RecoveryKitMode, enforcement: CaptureEnforcement): RecoveryKitExit[] {
  if (mode === "reveal-required") {
    return [
      { id: "reveal-with-password", label: "Show my recovery kit", requiresAcknowledgement: false },
      { id: "remind-me-later", label: "Remind me later", requiresAcknowledgement: false },
    ];
  }
  if (mode !== "refusal") return [];
  const exits: RecoveryKitExit[] = [];
  // Retrying is pointless where the primitive does not exist at all; offering
  // it there would be the same false claim in button form.
  if (enforcement === "enforced") {
    exits.push({ id: "retry-protection", label: "Retry protection", requiresAcknowledgement: false });
  }
  exits.push({
    id: "show-anyway",
    label: "Show anyway — I have checked my screen",
    requiresAcknowledgement: true,
  });
  exits.push({ id: "remind-me-later", label: "Remind me later", requiresAcknowledgement: false });
  return exits;
}

export function recoveryKitView(state: RecoveryKitState): RecoveryKitView {
  const mode = modeFor(state);
  return {
    mode,
    secretsVisible: mode === "kit",
    claim: claimFor(state),
    notice: noticeFor(state, mode),
    exits: exitsFor(mode, state.captureEnforcement),
    acknowledgementPrompt: mode === "refusal"
      ? `Type "${RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT}" to confirm you have checked who can see this screen.`
      : null,
  };
}

/** The phrases the caller may put on screen, or `null` while they are held back. */
export function visibleRecoverySecrets(state: RecoveryKitState): RecoveryKitSecrets | null {
  return recoveryKitView(state).secretsVisible ? state.secrets : null;
}

export type RecoveryKitAction =
  | { kind: "protection-proved"; proven: boolean; enforcement: CaptureEnforcement }
  | { kind: "show-anyway"; acknowledgement: string }
  | { kind: "remind-me-later" }
  | { kind: "set-saved-acknowledged"; acknowledged: boolean }
  | { kind: "continue" }
  | { kind: "revealed"; secrets: RecoveryKitSecrets };

export type RecoveryKitOutcome =
  /** State changed (or did not), stay on the recovery step. */
  | "none"
  /** The action was refused; nothing changed, nothing was lost. */
  | "rejected"
  /** The recovery step is finished for now; the caller may route onward. */
  | "leave-recovery";

export interface RecoveryKitTransition {
  state: RecoveryKitState;
  outcome: RecoveryKitOutcome;
}

export function recoveryKitReducer(
  state: RecoveryKitState,
  action: RecoveryKitAction,
): RecoveryKitTransition {
  switch (action.kind) {
    case "protection-proved":
      return {
        state: { ...state, captureProven: action.proven, captureEnforcement: action.enforcement },
        outcome: "none",
      };
    case "show-anyway":
      // A wrong or empty acknowledgement must leave the state — and therefore
      // the phrases — exactly as it found them.
      if (!acknowledgementAccepted(action.acknowledgement)) return { state, outcome: "rejected" };
      return { state: { ...state, shownWithoutProtection: true }, outcome: "none" };
    case "remind-me-later":
      // Deferring drops the secrets from memory but records that the kit is
      // still unsaved, which is what forces the step to be re-offered.
      return {
        state: { ...state, secrets: null, shownWithoutProtection: false, kitUnsaved: true },
        outcome: "leave-recovery",
      };
    case "set-saved-acknowledged":
      return { state: { ...state, savedAcknowledged: action.acknowledged }, outcome: "none" };
    case "continue":
      if (!state.savedAcknowledged || !recoveryKitView(state).secretsVisible) {
        return { state, outcome: "rejected" };
      }
      return {
        state: {
          ...state,
          secrets: null,
          shownWithoutProtection: false,
          savedAcknowledged: false,
          kitUnsaved: false,
        },
        outcome: "leave-recovery",
      };
    case "revealed":
      return { state: { ...state, secrets: action.secrets }, outcome: "none" };
  }
}
