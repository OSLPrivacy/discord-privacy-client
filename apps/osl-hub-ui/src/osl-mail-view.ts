import type { OslMailBurnReceipt, OslMailDeleteReceipt, OslMailRetrievedThread, OslMailSendReceipt, OslMailStatus, OslMailThreadSummary } from "./osl-mail-adapter";
import { DELETION_REFERENCE_DISCLOSURE } from "./feature-claims";

export type OslMailPane = "inbox" | "compose" | "settings";
export interface OslMailViewModel {
  loading: boolean;
  available: boolean;
  signedUsername: string | null;
  status: OslMailStatus | null;
  threads: OslMailThreadSummary[];
  activeThread: OslMailRetrievedThread | null;
  pane: OslMailPane;
  notifications: boolean;
  deleteReceipt: OslMailDeleteReceipt | null;
  sendReceipt: OslMailSendReceipt | null;
  burnReceipt: OslMailBurnReceipt | null;
  error: string | null;
  threadSyncUnavailable: boolean;
  composeDraft?: OslMailComposeDraft;
}

export interface OslMailComposeDraft {
  to: string;
  subject: string;
  body: string;
}

function escape(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] ?? character);
}
function date(seconds: number): string {
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }).format(new Date(seconds * 1_000));
}
const transitCopy = {
  oslE2ee: { className: "is-e2ee", title: "Protected between verified OSL identities", label: "OSL protected" },
  externalSmtp: { className: "is-standard-email", title: "Ordinary email outside OSL protection", label: "Standard email" },
} as const;

function transitBadge(transit: "oslE2ee" | "externalSmtp"): string {
  const copy = transitCopy[transit];
  return `<span class="osl-mail-transit ${copy.className}" title="${copy.title}">${copy.label}</span>`;
}
function confirmation(label: string, hash: string): string {
  return `<div class="osl-mail-confirmation" role="status"><strong>${escape(label)}</strong><code>${escape(hash.slice(0, 12))}…</code></div>`;
}

/**
 * Master §7.5: "Requested deletion is never displayed as verified deletion."
 *
 * OSL never re-reads a burned or acknowledged mailbox, so the only fact it
 * holds is that the mail server accepted the request.  `receiptSha256` is
 * computed locally from a local timestamp and a local request id
 * (`apps/osl-hub/src/osl_mail.rs` `burn`) -- it is OSL's own digest, not remote
 * evidence, so it is labelled `OSL reference` and can never read as proof that a
 * remote copy is gone.  The wording here is the same shape the compliant
 * Spaces path already uses (`apps/osl-hub/src/spaces.rs:236-245`), and it obeys
 * OSL's own definition of a confirmed deletion: `scrub-delete-engine.ts:68`
 * emits `confirmed-deleted` only on an *absent readback*.  OSL Mail performs no
 * readback, so no OSL Mail outcome can earn that word.
 *
 * Enforced by `osl-mail-view.test.ts` -> "never displays a requested deletion as
 * a verified deletion".  Reintroducing the word `confirmed` on either receipt
 * turns that test red.
 */
export const OSL_MAIL_DELETION_UNVERIFIED_NOTE = "OSL has not re-read the mailbox, so server-side deletion is unverified.";

// `warning` carries the amber label from `.warning strong` (styles.css:814); a
// requested, unverified deletion must not read as a green success confirmation.
// Completing the left rail needs one rule in styles.css (owned by another lane):
// `.osl-mail-confirmation.warning { border-left-color: var(--warn); }`.
function requestReceipt(label: string, detail: string, hash: string): string {
  return `<div class="osl-mail-confirmation warning" role="status"><strong>${escape(label)}</strong><small>${escape(detail)} ${escape(OSL_MAIL_DELETION_UNVERIFIED_NOTE)}</small><p class="deletion-reference-disclosure" role="note">${escape(DELETION_REFERENCE_DISCLOSURE)}</p><code>OSL reference ${escape(hash.slice(0, 12))}…</code></div>`;
}

function deletionRequested(receipt: OslMailDeleteReceipt): string {
  const count = receipt.deletedMessageIds.length;
  return requestReceipt("Server deletion requested", `The server accepted the acknowledgment for ${count} ${count === 1 ? "message" : "messages"}.`, receipt.receiptSha256);
}

function burnRequested(receipt: OslMailBurnReceipt): string {
  const count = receipt.deletedMessages;
  return requestReceipt("Mailbox burn requested", `The server reported the address disabled and ${count} ${count === 1 ? "message" : "messages"} deleted.`, receipt.receiptSha256);
}

function unavailable(model: OslMailViewModel): string {
  if (model.loading) return '<section class="osl-mail-state" role="status"><span class="mail-spinner" aria-hidden="true"></span><h2>Checking OSL Mail</h2></section>';
  return `<section class="osl-mail-state" role="status"><span class="osl-mail-mark" aria-hidden="true">M</span><h2>OSL Mail unavailable</h2><p>The signed mailbox service did not return a valid response. No messages or account state are being invented.</p><button class="button" id="osl-mail-retry" type="button">Retry</button></section>`;
}

function provision(model: OslMailViewModel): string {
  const username = model.signedUsername;
  if (!username) return '<section class="osl-mail-state"><span class="osl-mail-mark" aria-hidden="true">M</span><h2>Choose your signed OSL username first</h2><p>Your mailbox address is derived from that signed username. OSL Mail never asks for a phone number.</p><button class="button" data-route="settings" data-profile-settings type="button">Open profile</button></section>';
  const address = `${username}@oslprivacy.com`;
  return `<section class="osl-mail-state"><span class="osl-mail-mark" aria-hidden="true">M</span><h2>${escape(address)}</h2><p>Create the mailbox bound to your signed OSL identity. No phone number or separate mail password.</p><button class="button primary" id="osl-mail-provision" type="button">Create mailbox</button></section>`;
}

function threadList(model: OslMailViewModel): string {
  if (model.threadSyncUnavailable && !model.threads.length) return '<div class="osl-mail-empty warning"><strong>Inbox sync unavailable</strong><span>Messages are not being reported as empty.</span></div>';
  if (!model.threads.length) return '<div class="osl-mail-empty"><strong>Inbox clear</strong><span>No server-confirmed messages.</span></div>';
  return model.threads.map((thread) => `<button class="osl-mail-thread ${thread.unread ? "is-unread" : ""}" data-mail-thread="${escape(thread.threadId)}" type="button"><span class="osl-mail-thread-copy"><strong>${escape(thread.correspondent)}</strong><span>${escape(thread.subject || "(No subject)")}</span></span><span class="osl-mail-thread-meta">${transitBadge(thread.transit)}<time>${escape(date(thread.latestAt))}</time></span></button>`).join("");
}

function activeThread(model: OslMailViewModel): string {
  const thread = model.activeThread;
  if (!thread) return '<section class="osl-mail-reading is-empty"><span class="osl-mail-mark" aria-hidden="true">M</span><p>Select a message.</p></section>';
  const messages = thread.messages.map((message) => `<article class="osl-mail-message"><header><div><strong>${escape(message.from)}</strong><span>to ${escape(message.to.join(", "))}</span></div>${transitBadge(message.transit)}</header><h3>${escape(message.subject || "(No subject)")}</h3><p>${escape(message.body)}</p><time>${escape(date(message.receivedAt))}</time></article>`).join("");
  return `<section class="osl-mail-reading"><header class="osl-mail-retrieval"><span><strong>Ephemeral retrieval</strong><small>Visible on this device until acknowledged or expired.</small></span><time>${escape(date(thread.expiresAt))}</time></header><div class="osl-mail-message-list">${messages}</div><footer><button class="button primary" id="osl-mail-ack" type="button">Acknowledge device copy & request server deletion</button>${model.deleteReceipt ? deletionRequested(model.deleteReceipt) : ""}</footer></section>`;
}

function inbox(model: OslMailViewModel): string {
  return `<div class="osl-mail-inbox"><aside class="osl-mail-list" aria-label="Mailbox threads"><header><div><strong>Inbox</strong><span>${model.status?.unreadCount ?? 0} unread</span></div><button class="icon-button" data-mail-pane="compose" type="button" aria-label="Compose mail" title="Compose">＋</button></header>${threadList(model)}</aside>${activeThread(model)}</div>`;
}

function compose(model: OslMailViewModel): string {
  return `<section class="osl-mail-compose"><header><h2>New OSL message</h2><button class="text-button" data-mail-pane="inbox" type="button">Cancel</button></header><form id="osl-mail-compose-form"><label>To<input id="osl-mail-to" type="email" inputmode="email" autocomplete="email" placeholder="username@oslprivacy.com" required/></label><label>Subject<input id="osl-mail-subject" maxlength="512"/></label><label class="mail-body">Message<textarea id="osl-mail-body" required maxlength="262144"></textarea></label><div class="osl-mail-transit-guide"><span>${transitBadge("oslE2ee")} OSL Mail recipients only</span><span class="is-disabled">External outbound is unavailable in v1</span></div><button class="button primary" id="osl-mail-send" data-osl-mail-send-choice="Send" type="button">Send protected</button></form>${model.sendReceipt ? confirmation("Send accepted", model.sendReceipt.receiptSha256) : ""}</section>`;
  const draft = model.composeDraft ?? { to: "", subject: "", body: "" };
  return `<section class="osl-mail-compose"><header><h2>New OSL message</h2><button class="text-button" data-mail-pane="inbox" type="button">Cancel</button></header><form id="osl-mail-compose-form"><label>To<input id="osl-mail-to" type="email" inputmode="email" autocomplete="email" placeholder="username@oslprivacy.com" value="${escape(draft.to)}" required/></label><label>Subject<input id="osl-mail-subject" maxlength="512" value="${escape(draft.subject)}"/></label><label class="mail-body">Message<textarea id="osl-mail-body" required maxlength="262144">${escape(draft.body)}</textarea></label><div class="osl-mail-transit-guide"><span>${transitBadge("oslE2ee")} OSL Mail recipients only</span><span class="is-disabled">External outbound is unavailable in v1</span></div><button class="button primary" type="submit">Send protected</button></form>${model.sendReceipt ? confirmation("Send accepted", model.sendReceipt.receiptSha256) : ""}</section>`;
}

function settings(model: OslMailViewModel): string {
  const address = model.status?.address ?? "";
  return `<section class="osl-mail-settings"><h2>Mail settings</h2><div class="settings-list"><label class="setting-line interactive"><span><strong>Mail notifications</strong><small>Generic local alerts only. Message content stays hidden.</small></span><input id="osl-mail-notifications" type="checkbox" ${model.notifications ? "checked" : ""}/></label><div class="setting-line"><span><strong>Retrieval</strong><small>Messages require an explicit device acknowledgment before OSL requests server deletion. OSL cannot verify that a remote copy is gone.</small></span><span class="status-tag">Ephemeral</span></div><div class="setting-line"><span><strong>External inbound</strong><small>May arrive as ordinary email before OSL can protect the local mailbox copy.</small></span><span class="status-tag">Standard email</span></div><div class="setting-line"><span><strong>External outbound</strong><small>Unavailable until OSL can prove the protected destination before send.</small></span><span class="status-tag">Off</span></div></div><details class="settings-disclosure danger-disclosure"><summary><span><strong>Burn mailbox</strong><small>Disable ${escape(address)} and request deletion of remaining server messages.</small></span></summary><form id="osl-mail-burn-form"><label>Type the full address<input id="osl-mail-burn-confirmation" autocomplete="off" required/></label><button class="button danger" type="submit">Burn mailbox</button></form></details>${model.burnReceipt ? burnRequested(model.burnReceipt) : ""}</section>`;
}

export function oslMailViewMarkup(model: OslMailViewModel): string {
  const content = !model.available ? unavailable(model) : !model.status?.provisioned ? provision(model) : model.pane === "compose" ? compose(model) : model.pane === "settings" ? settings(model) : inbox(model);
  const confirmations = `${model.deleteReceipt ? deletionRequested(model.deleteReceipt) : ""}${model.burnReceipt ? burnRequested(model.burnReceipt) : ""}`;
  return `<main class="content-viewport osl-mail-page"><header class="osl-mail-header"><button class="home-logo-button" data-route="home" type="button" aria-label="OSL Home"><span class="osl-mail-back" aria-hidden="true">←</span></button><div><span class="osl-mail-mark" aria-hidden="true">M</span><h1 id="route-heading" tabindex="-1">OSL Mail</h1>${model.status?.address ? `<small>${escape(model.status.address)}</small>` : ""}</div><nav aria-label="Mail navigation"><button class="${model.pane === "inbox" ? "active" : ""}" data-mail-pane="inbox" type="button">Inbox</button><button class="${model.pane === "compose" ? "active" : ""}" data-mail-pane="compose" type="button" ${model.status?.provisioned ? "" : "disabled"}>Compose</button><button class="${model.pane === "settings" ? "active" : ""}" data-mail-pane="settings" type="button" ${model.status?.provisioned ? "" : "disabled"}>Settings</button></nav></header>${model.error ? `<p class="osl-mail-error" role="alert">${escape(model.error)}</p>` : ""}${confirmations ? `<div class="osl-mail-confirmation-strip">${confirmations}</div>` : ""}${content}</main>`;
}
