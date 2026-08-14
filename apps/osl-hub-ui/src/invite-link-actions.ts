/**
 * TASK 0220 — the four invite-link actions on Add Friend: Create invite link,
 * Copy, Share, and Paste invite link.
 *
 * The link itself is `OneUseInviteLink` as `create_one_use_invite_link`
 * (apps/osl-hub/src/security.rs) returns it — a single-use
 * `https://invite.osl.local/one-use/<id>.<token>` address, not the standing
 * `OSLFR1.` friend code `friendInviteCardMarkup` (./ui-behavior.ts) renders.
 * The two invites answer different questions — "who is this device" versus
 * "let one specific person in, once" — so they get separate actions rather
 * than folding one into the other.
 *
 * Copy and Share only make sense once a link exists, so they render disabled
 * until `link` is set. Paste is independent of the other three: an operator
 * pasting a link someone else generated never needs to have created one.
 */

export interface OneUseInviteLink {
  readonly inviteId: string;
  readonly link: string;
  readonly recipientLabel: string;
  readonly expiresAt: number;
}

export interface InviteLinkActionsModel {
  /** The most recently created link this device has not consumed, or `null`. */
  readonly link: OneUseInviteLink | null;
  /** True while `create_one_use_invite_link` is in flight. */
  readonly creating: boolean;
  /** What is currently typed into the paste box. */
  readonly pasteValue: string;
}

export function inviteLinkActionsMarkup(
  model: InviteLinkActionsModel,
  escapeHtml: (value: string) => string,
): string {
  const hasLink = model.link !== null;
  const linkArea = model.link
    ? `<code class="invite-link-value" data-invite-link-value tabindex="0">${escapeHtml(model.link.link)}</code>`
    : `<p class="invite-link-empty" data-invite-link-empty>No invite link yet. Create one to send a single-use invite.</p>`;

  return `<section class="invite-link-actions" aria-label="Invite link actions">
    <div class="invite-link-create-row">
      <button class="button" id="invite-link-create" type="button" data-invite-link-create ${model.creating ? "disabled" : ""}>${model.creating ? "Creating…" : "Create invite link"}</button>
    </div>
    <div class="invite-link-display">
      ${linkArea}
      <div class="invite-link-share-row">
        <button class="button" id="invite-link-copy" type="button" data-invite-link-copy ${hasLink ? "" : "disabled"}>Copy</button>
        <button class="button" id="invite-link-share" type="button" data-invite-link-share ${hasLink ? "" : "disabled"}>Share</button>
      </div>
    </div>
    <form class="invite-link-paste-form" id="invite-link-paste-form" data-invite-link-paste-form>
      <label for="invite-link-paste-input"><span>Paste invite link</span>
        <input id="invite-link-paste-input" data-invite-link-paste-input placeholder="https://invite.osl.local/one-use/…" autocomplete="off" autocapitalize="none" spellcheck="false" value="${escapeHtml(model.pasteValue)}"/>
      </label>
      <button class="button primary" id="invite-link-paste-submit" type="submit" data-invite-link-paste-submit ${model.pasteValue.trim() ? "" : "disabled"}>Use invite link</button>
    </form>
  </section>`;
}

export function inviteLinkCopyFailureToast(reason: string): string {
  const detail = reason.trim();
  return detail ? `Could not copy the invite link · ${detail}` : "Could not copy the invite link";
}

/**
 * TASK 0221 -- connecting the four actions `inviteLinkActionsMarkup` draws to
 * the commands `cmd_osl_create_friend_invite_link` and
 * `cmd_osl_redeem_friend_invite_link` (crates/ipc/src/commands.rs, TASK 0218)
 * perform. Create asks the backend for a fresh link; Paste redeems whatever
 * link an operator typed or pasted in, exactly as `cmd_osl_redeem_friend_invite_link`
 * does -- one redeem turns into one `PendingFriendRequestRecord`, which is one
 * new row in Pending (`pending-requests-screen.ts`).
 *
 * Both commands are injected rather than imported, so this module and its
 * tests never need a live Tauri runtime: a caller wires the real `invoke`
 * calls in, a test wires in fakes.
 */

/** Mirrors `FriendInviteLinkResult` (crates/ipc/src/commands.rs). */
export interface CreatedFriendInviteLink {
  readonly inviteLink: string;
  readonly peerDiscordId: string;
  readonly scopeStorageKey: string;
}

/** Mirrors `PendingFriendRequestRecord` (crates/ipc/src/commands.rs). */
export interface RedeemedInviteLinkRequest {
  readonly peerDiscordId: string;
  readonly scopeStorageKey: string;
  readonly createdAtUnixSeconds: number;
}

export interface InviteLinkCommands {
  readonly createInviteLink: () => Promise<CreatedFriendInviteLink>;
  readonly redeemInviteLink: (inviteLink: string) => Promise<RedeemedInviteLinkRequest>;
}

/** The shape `pending-requests-screen.ts`'s `addPendingRequest` expects. */
export interface RedeemedPendingRequest {
  readonly id: string;
  readonly alias: string;
  readonly discordId: string;
}

/** A redeemed invite link has no alias yet -- only the peer's Discord id -- so the id doubles as the alias until the peer's profile arrives. */
export function pendingRequestFromRedeemedInvite(redeemed: RedeemedInviteLinkRequest): RedeemedPendingRequest {
  return {
    id: redeemed.peerDiscordId,
    alias: redeemed.peerDiscordId,
    discordId: redeemed.peerDiscordId,
  };
}

/** Create: ask the backend for a fresh link and fold it into the model as the current, unconsumed link. */
export async function runCreateInviteLink(
  model: InviteLinkActionsModel,
  createOneUseInviteLink: () => Promise<OneUseInviteLink>,
): Promise<InviteLinkActionsModel> {
  const link = await createOneUseInviteLink();
  return { ...model, link, creating: false };
}

/**
 * Paste: redeem whatever is typed into the paste box. Trims first so
 * whitespace pasted around a link is never treated as part of it. Returns
 * the pending request the redeem produced, ready for `addPendingRequest`.
 */
export async function runPasteInviteLink(
  pasteValue: string,
  redeemInviteLink: (inviteLink: string) => Promise<RedeemedInviteLinkRequest>,
): Promise<RedeemedPendingRequest> {
  const trimmed = pasteValue.trim();
  if (!trimmed) throw new Error("OSL: paste an invite link first");
  const redeemed = await redeemInviteLink(trimmed);
  return pendingRequestFromRedeemedInvite(redeemed);
}

/**
 * Mount the actions on an element and wire Create/Copy/Share/Paste to the
 * injected commands. `onRequestCreated` is how a redeemed paste reaches
 * Pending -- the caller decides where that state lives (`main.ts` holds the
 * app's real `PendingRequestsState`; a test can hand in a spy).
 */
export function attachInviteLinkActions(
  mount: HTMLElement,
  initial: InviteLinkActionsModel,
  commands: {
    readonly createInviteLink: () => Promise<OneUseInviteLink>;
    readonly redeemInviteLink: (inviteLink: string) => Promise<RedeemedInviteLinkRequest>;
    readonly copyToClipboard: (text: string) => Promise<void>;
    readonly shareLink: (text: string) => Promise<void>;
  },
  escapeHtml: (value: string) => string,
  onRequestCreated: (request: RedeemedPendingRequest) => void = () => {},
  onError: (message: string) => void = () => {},
): void {
  let model = initial;
  const draw = (): void => {
    mount.innerHTML = inviteLinkActionsMarkup(model, escapeHtml);
  };

  mount.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;

    if (target?.closest?.("[data-invite-link-create]")) {
      if (model.creating) return;
      model = { ...model, creating: true };
      draw();
      runCreateInviteLink(model, commands.createInviteLink).then(
        (next) => {
          model = next;
          draw();
        },
        (error: unknown) => {
          model = { ...model, creating: false };
          draw();
          onError(inviteLinkCopyFailureToast(error instanceof Error ? error.message : String(error)));
        },
      );
      return;
    }

    if (target?.closest?.("[data-invite-link-copy]")) {
      if (!model.link) return;
      commands.copyToClipboard(model.link.link).catch((error: unknown) => {
        onError(inviteLinkCopyFailureToast(error instanceof Error ? error.message : String(error)));
      });
      return;
    }

    if (target?.closest?.("[data-invite-link-share]")) {
      if (!model.link) return;
      commands.shareLink(model.link.link).catch((error: unknown) => {
        onError(inviteLinkCopyFailureToast(error instanceof Error ? error.message : String(error)));
      });
    }
  });

  mount.addEventListener("input", (event) => {
    const target = event.target as HTMLElement | null;
    if (!target?.matches?.("[data-invite-link-paste-input]")) return;
    model = { ...model, pasteValue: (target as HTMLInputElement).value };
    draw();
  });

  mount.addEventListener("submit", (event) => {
    const target = event.target as HTMLElement | null;
    if (!target?.matches?.("[data-invite-link-paste-form]")) return;
    event.preventDefault();
    const pasteValue = model.pasteValue;
    runPasteInviteLink(pasteValue, commands.redeemInviteLink).then(
      (request) => {
        model = { ...model, pasteValue: "" };
        draw();
        onRequestCreated(request);
      },
      (error: unknown) => {
        onError(inviteLinkCopyFailureToast(error instanceof Error ? error.message : String(error)));
      },
    );
  });

  draw();
}
