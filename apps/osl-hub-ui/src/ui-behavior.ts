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

/**
 * There is no inbound friend-request mechanism in this build. Adding someone
 * writes a record on THIS device only; nothing is delivered to them and nothing
 * ever arrives on its own. Any copy that implies an incoming request strands the
 * user waiting forever, so these lines name the action the user still owes.
 * See `plan/ADD-FRIEND-PROCEDURE.md`: the invite exchange is symmetric.
 */
export type FriendHandshakeState = "verified" | "key-change" | "invite-not-exchanged";

export function friendHandshakeState(
  safetyNumberVerified: boolean,
  pendingKeyChange: boolean,
): FriendHandshakeState {
  if (pendingKeyChange) return "key-change";
  return safetyNumberVerified ? "verified" : "invite-not-exchanged";
}

/** One short line for a person row. Never implies an inbound request. */
export function friendHandshakeSummary(
  safetyNumberVerified: boolean,
  pendingKeyChange: boolean,
): string {
  switch (friendHandshakeState(safetyNumberVerified, pendingKeyChange)) {
    case "key-change":
      return "Security change needs review";
    case "verified":
      return "Verified";
    case "invite-not-exchanged":
      return "Waiting on you — send them your invite, then verify";
  }
}

/** The longer explanation used wherever there is room for the next action. */
export function friendHandshakeDetail(
  safetyNumberVerified: boolean,
  pendingKeyChange: boolean,
): string {
  switch (friendHandshakeState(safetyNumberVerified, pendingKeyChange)) {
    case "key-change":
      return "Verification changed. Protected sends stay off until you review it.";
    case "verified":
      return "Verified. Approve each chat you want OSL to protect.";
    case "invite-not-exchanged":
      return "OSL sent them no request. Send them your invite so they can add you, then verify each other. Both of you must finish every step before a message can be read.";
  }
}

export interface FriendInviteCardOptions {
  readonly sectionClass: string;
  readonly labelId: string;
}

/**
 * The "Your friend ID" card. The note must state both directions: an invite
 * that only travels one way leaves the other person unable to add you.
 */
export function friendInviteCardMarkup(
  compactFriendId: string,
  escapeHtml: (value: string) => string,
  options: FriendInviteCardOptions,
): string {
  return `<section class="${options.sectionClass}" aria-labelledby="${options.labelId}"><div><span id="${options.labelId}">Your friend ID</span><code>${escapeHtml(compactFriendId)}</code></div><button class="button" id="copy-friend-code" type="button">Copy invite</button><p>Send your invite to someone you trust, and add theirs here. Both people must do this — no request ever arrives on its own.</p></section>`;
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
