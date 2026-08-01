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
  /**
   * The full `OSLFR1.` invite exactly as `export_friend_code` produced it.
   * Required, not optional: a card that silently omits it is the defect this
   * field exists to prevent.
   */
  readonly friendCode: string;
}

/**
 * The "Your friend ID" card. The note must state both directions: an invite
 * that only travels one way leaves the other person unable to add you.
 *
 * The card renders the whole invite, not only the shortened friend ID. The
 * shortened form is a label -- it cannot be sent to anyone, because it is not
 * the value `add_hub_friend` accepts. Printing only that left every desktop
 * without a working clipboard helper with no way at all to get the invite out
 * of the app. Selectable text needs no helper and no permission, so it is the
 * export route that cannot fail; the copy button is the convenience on top.
 */
export function friendInviteCardMarkup(
  compactFriendId: string,
  escapeHtml: (value: string) => string,
  options: FriendInviteCardOptions,
): string {
  const inviteLabelId = `${options.labelId}-full-invite`;
  return `<section class="${options.sectionClass}" aria-labelledby="${options.labelId}"><div><span id="${options.labelId}">Your friend ID</span><code>${escapeHtml(compactFriendId)}</code></div><button class="button" id="copy-friend-code" type="button">Copy invite</button><div class="friend-invite-full"><span id="${inviteLabelId}">Your full invite</span><code class="friend-invite-code" data-friend-invite tabindex="0" aria-labelledby="${inviteLabelId}">${escapeHtml(options.friendCode)}</code></div><p>Send your invite to someone you trust, and add theirs here. Both people must do this — no request ever arrives on its own. If copying does not work, select the invite above and copy it by hand.</p></section>`;
}

/**
 * The toast for an invite export that did not happen, keeping the backend's
 * own reason.
 *
 * The generic half stays first so the outcome is readable at a glance, and the
 * specific half follows so the operator can tell a missing clipboard tool from
 * a locked identity. `reason` has already been through
 * `sanitizeBackendMessage`, which is where the judgement about what may be
 * shown lives.
 */
export function inviteCopyFailureToast(reason: string): string {
  const detail = reason.trim();
  return detail ? `Could not copy the invite · ${detail}` : "Could not copy the invite";
}

/**
 * The add-friend status line for an attempt that changed nothing.
 *
 * "Nothing changed" was already true and stays. What was missing is *why*: a
 * malformed paste, an invalid signature and an identity-key change are three
 * different problems with three different fixes, and the operator could not
 * tell them apart.
 */
export function addFriendFailureStatus(reason: string): string {
  const detail = reason.trim();
  return detail
    ? `The invite could not be added. Nothing changed. ${detail}`
    : "The invite could not be added. Nothing changed.";
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

export const EMPTY_VERIFICATION_CODE_REFUSAL =
  "Enter the code shown on your friend's screen before accepting.";

/**
 * Whether the owned-confirmation submit button is unusable.
 *
 * Being mid-submit is the only reason, and it is not derived from any observed
 * event. The verify dialog used to render this button `disabled` for the whole
 * dialog kind and re-enable it only from an `input` listener, so a paste that
 * fires no `input` — or any programmatic fill — left a dead button on the one
 * screen a first-time user has to get through, with no other way forward from
 * it. Whether the field is empty is decided when the button is pressed, by
 * reading the field, rather than predicted from events that may never arrive.
 */
export function ownedConfirmationSubmitDisabled(busy: boolean): boolean {
  return busy;
}

/**
 * What to do with whatever is in the verification field at submit time.
 *
 * The code is handed on exactly as it was entered — grouping and separators are
 * normalised by the constant-time comparison in Rust, not here — so a pasted
 * value survives untouched. Blank is refused visibly instead of being made
 * unreachable by a disabled control.
 */
export function verificationSubmission(
  fieldValue: string,
): { code: string } | { refusal: string } {
  return fieldValue.trim().length === 0
    ? { refusal: EMPTY_VERIFICATION_CODE_REFUSAL }
    : { code: fieldValue };
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
