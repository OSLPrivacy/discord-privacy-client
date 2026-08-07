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
