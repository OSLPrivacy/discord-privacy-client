export class RecoveryCaptureGate {
  private generation = 0;
  private provenGeneration: number | null = null;

  checkpoint(): number {
    return this.generation;
  }

  invalidate(): void {
    this.generation += 1;
    this.provenGeneration = null;
  }

  accept(checkpoint: number): boolean {
    if (checkpoint !== this.generation) return false;
    this.provenGeneration = checkpoint;
    return true;
  }

  canRender(): boolean {
    return this.provenGeneration === this.generation;
  }
}

export interface RecoveryFocusActions {
  scheduleNativeHostRealignment(): void;
  hasRecoverySecrets(): boolean;
  proveRecoveryCaptureProtection(): Promise<boolean>;
  invalidateRecoveryCapture(): void;
  setScreenshotProtectionEnabled(enabled: boolean): void;
  render(): void;
}

export function dispatchMainWindowFocusChanged(
  focused: boolean,
  actions: RecoveryFocusActions,
): void {
  if (focused) {
    actions.scheduleNativeHostRealignment();
    if (actions.hasRecoverySecrets()) {
      void actions.proveRecoveryCaptureProtection().then(() => actions.render());
    }
    return;
  }

  actions.invalidateRecoveryCapture();
  actions.setScreenshotProtectionEnabled(false);
  if (actions.hasRecoverySecrets()) actions.render();
}

export interface WindowFocusChangedEvent {
  payload: boolean;
}

export function bindMainWindowFocusChanges(
  register: (handler: (event: WindowFocusChangedEvent) => void) => Promise<unknown>,
  actions: RecoveryFocusActions,
): Promise<unknown> {
  return register(({ payload }) => dispatchMainWindowFocusChanged(payload, actions));
}

export interface FriendRemovalControl extends EventTarget {
  readonly dataset: { readonly removePerson?: string };
}

export interface FriendRemovalRoot {
  querySelectorAll(selector: string): Iterable<FriendRemovalControl>;
}

export const FRIEND_REMOVAL_SELECTOR = "[data-remove-person]";

export type FriendTrustAction = "verified" | "verify";

export function friendTrustAction(
  safetyNumberVerified: boolean,
  pendingKeyChange: boolean,
): FriendTrustAction {
  return safetyNumberVerified && !pendingKeyChange ? "verified" : "verify";
}

export interface FriendVerificationCopy {
  readonly heading: string;
  readonly code: string;
  readonly instruction: string;
  readonly consequence: string;
  readonly invalidationNotice: string;
}

/**
 * The words the safety-number ceremony puts on screen.
 *
 * The number is derived from BOTH identities, so the two devices display the
 * same digits and the operator's job is to *compare* them. The previous copy
 * told the operator to read their own code aloud and type back what their
 * friend read to them — an instruction that, followed literally, made the
 * ceremony fail, because each device could only ever accept the value already
 * on its own screen. Returned as data rather than markup so the instruction can
 * be checked against what the ceremony actually accepts.
 */
export function friendVerificationCopy(
  alias: string | null,
  safetyNumber: string | null,
): FriendVerificationCopy {
  return {
    heading: `Your verification code for ${alias ?? "this friend"}. Their device shows this same code:`,
    code: safetyNumber && safetyNumber.length > 0 ? safetyNumber : "Unavailable",
    instruction: "Ask your friend to read out the code on their screen, over a channel that is not this app, and type it here",
    consequence: "The codes match only if you are talking to the device OSL holds keys for. Accepting lets OSL encrypt to that key. It does not turn on decryption in any chat or approve any conversation.",
    invalidationNotice: "Verifications recorded by earlier versions of OSL have been cleared. Those compared a code OSL generated against itself, so they proved nothing; every friend has to be verified again.",
  };
}

export function shouldClearRemovedFriendChat(
  activePersonId: string | null,
  removedPersonId: string,
): boolean {
  return removedPersonId.length > 0 && activePersonId === removedPersonId;
}

export interface FriendRemovalDependencies {
  isTauriRuntime(): boolean;
  invoke(command: "remove_hub_friend", args: { personId: string }): Promise<unknown>;
  recordBackendFailure(command: "remove_hub_friend", error: unknown): void;
}

function safeFriendPersonId(personId: string): boolean {
  return personId.length > 0
    && personId.length <= 180
    && !/[<>\u0000-\u001f\u007f]/.test(personId);
}

export async function removeHubFriend(
  personId: string,
  dependencies: FriendRemovalDependencies,
): Promise<boolean> {
  if (!dependencies.isTauriRuntime() || !safeFriendPersonId(personId)) return false;
  try {
    await dependencies.invoke("remove_hub_friend", { personId });
    return true;
  } catch (error) {
    dependencies.recordBackendFailure("remove_hub_friend", error);
    return false;
  }
}

export function friendRemovalButtonMarkup(
  personId: string,
  escapeAttribute: (value: string) => string,
): string {
  return `<button class="button compact danger" type="button" data-remove-person="${escapeAttribute(personId)}">Remove friend</button>`;
}

export function bindFriendRemovalControls(
  root: FriendRemovalRoot,
  requestFriendRemoval: (personId: string) => void,
): void {
  for (const control of root.querySelectorAll(FRIEND_REMOVAL_SELECTOR)) {
    control.addEventListener("click", () => requestFriendRemoval(control.dataset.removePerson ?? ""));
  }
}
