import type { OslMailBurnReceipt, OslMailDeleteReceipt, OslMailRetrievedThread, OslMailSendReceipt, OslMailStatus, OslMailThreadSummary } from "./osl-mail-adapter";

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
function receipt(label: string, hash: string): string {
  return `<div class="osl-mail-receipt" role="status"><strong>${escape(label)}</strong><code>${escape(hash.slice(0, 12))}…</code></div>`;
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
  if (!model.threads.length) return '<div class="osl-mail-empty"><strong>Inbox clear</strong><span>No server-confirmed messages.</span></div>';
  return model.threads.map((thread) => `<button class="osl-mail-thread ${thread.unread ? "is-unread" : ""}" data-mail-thread="${escape(thread.threadId)}" type="button"><span class="osl-mail-thread-copy"><strong>${escape(thread.correspondent)}</strong><span>${escape(thread.subject || "(No subject)")}</span></span><span class="osl-mail-thread-meta">${transitBadge(thread.transit)}<time>${escape(date(thread.latestAt))}</time></span></button>`).join("");
}

function activeThread(model: OslMailViewModel): string {
  const thread = model.activeThread;
  if (!thread) return '<section class="osl-mail-reading is-empty"><span class="osl-mail-mark" aria-hidden="true">M</span><p>Select a message.</p></section>';
  const messages = thread.messages.map((message) => `<article class="osl-mail-message"><header><div><strong>${escape(message.from)}</strong><span>to ${escape(message.to.join(", "))}</span></div>${transitBadge(message.transit)}</header><h3>${escape(message.subject || "(No subject)")}</h3><p>${escape(message.body)}</p><time>${escape(date(message.receivedAt))}</time></article>`).join("");
  return `<section class="osl-mail-reading"><header class="osl-mail-retrieval"><span><strong>Ephemeral retrieval</strong><small>Visible on this device until acknowledged or expired.</small></span><time>${escape(date(thread.expiresAt))}</time></header><div class="osl-mail-message-list">${messages}</div><footer><button class="button primary" id="osl-mail-ack" type="button">Acknowledge device copy & delete server copy</button>${model.deleteReceipt ? receipt("Server deletion confirmed", model.deleteReceipt.receiptSha256) : ""}</footer></section>`;
}

function inbox(model: OslMailViewModel): string {
  return `<div class="osl-mail-inbox"><aside class="osl-mail-list" aria-label="Mailbox threads"><header><div><strong>Inbox</strong><span>${model.status?.unreadCount ?? 0} unread</span></div><button class="icon-button" data-mail-pane="compose" type="button" aria-label="Compose mail" title="Compose">＋</button></header>${threadList(model)}</aside>${activeThread(model)}</div>`;
}

function compose(model: OslMailViewModel): string {
  return `<section class="osl-mail-compose"><header><h2>New OSL message</h2><button class="text-button" data-mail-pane="inbox" type="button">Cancel</button></header><form id="osl-mail-compose-form"><label>To<input id="osl-mail-to" type="email" inputmode="email" autocomplete="email" placeholder="username@oslprivacy.com" required/></label><label>Subject<input id="osl-mail-subject" maxlength="512"/></label><label class="mail-body">Message<textarea id="osl-mail-body" required maxlength="262144"></textarea></label><div class="osl-mail-transit-guide"><span>${transitBadge("oslE2ee")} OSL Mail recipients only</span><span class="is-disabled">External outbound is unavailable in v1</span></div><button class="button primary" type="submit">Send protected</button></form>${model.sendReceipt ? receipt("Send accepted", model.sendReceipt.receiptSha256) : ""}</section>`;
}

function settings(model: OslMailViewModel): string {
  const address = model.status?.address ?? "";
  return `<section class="osl-mail-settings"><h2>Mail settings</h2><div class="settings-list"><label class="setting-line interactive"><span><strong>Mail notifications</strong><small>Generic local alerts only. Message content stays hidden.</small></span><input id="osl-mail-notifications" type="checkbox" ${model.notifications ? "checked" : ""}/></label><div class="setting-line"><span><strong>Retrieval</strong><small>Messages require an explicit device acknowledgment before server deletion is confirmed.</small></span><span class="status-tag">Ephemeral</span></div><div class="setting-line"><span><strong>External inbound</strong><small>May arrive as ordinary email before OSL can protect the local mailbox copy.</small></span><span class="status-tag">Standard email</span></div><div class="setting-line"><span><strong>External outbound</strong><small>Unavailable until OSL can prove the protected destination before send.</small></span><span class="status-tag">Off</span></div></div><details class="settings-disclosure danger-disclosure"><summary><span><strong>Burn mailbox</strong><small>Disable ${escape(address)} and request deletion of remaining server messages.</small></span></summary><form id="osl-mail-burn-form"><label>Type the full address<input id="osl-mail-burn-confirmation" autocomplete="off" required/></label><button class="button danger" type="submit">Burn mailbox</button></form></details>${model.burnReceipt ? receipt("Mailbox burn confirmed", model.burnReceipt.receiptSha256) : ""}</section>`;
}

export function oslMailViewMarkup(model: OslMailViewModel): string {
  const content = !model.available ? unavailable(model) : !model.status?.provisioned ? provision(model) : model.pane === "compose" ? compose(model) : model.pane === "settings" ? settings(model) : inbox(model);
  const receipts = `${model.deleteReceipt ? receipt("Server deletion confirmed", model.deleteReceipt.receiptSha256) : ""}${model.burnReceipt ? receipt("Mailbox burn confirmed", model.burnReceipt.receiptSha256) : ""}`;
  return `<main class="content-viewport osl-mail-page"><header class="osl-mail-header"><button class="home-logo-button" data-route="home" type="button" aria-label="OSL Home"><span class="osl-mail-back" aria-hidden="true">←</span></button><div><span class="osl-mail-mark" aria-hidden="true">M</span><h1 id="route-heading" tabindex="-1">OSL Mail</h1>${model.status?.address ? `<small>${escape(model.status.address)}</small>` : ""}</div><nav aria-label="Mail navigation"><button class="${model.pane === "inbox" ? "active" : ""}" data-mail-pane="inbox" type="button">Inbox</button><button class="${model.pane === "compose" ? "active" : ""}" data-mail-pane="compose" type="button" ${model.status?.provisioned ? "" : "disabled"}>Compose</button><button class="${model.pane === "settings" ? "active" : ""}" data-mail-pane="settings" type="button" ${model.status?.provisioned ? "" : "disabled"}>Settings</button></nav></header>${model.error ? `<p class="osl-mail-error" role="alert">${escape(model.error)}</p>` : ""}${receipts ? `<div class="osl-mail-receipt-strip">${receipts}</div>` : ""}${content}</main>`;
}
