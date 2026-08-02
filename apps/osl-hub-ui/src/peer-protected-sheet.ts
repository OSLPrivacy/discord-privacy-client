import { peerIsVerified, type HubPerson, type ManualPeerContext } from "./adapters";
import { LOCAL_TTL_OPTIONS, type LocalTtlSeconds } from "./local-protected-sheet";
import { utf8Length } from "./overlay-state";

const PEER_PROTECTED_DRAFT_BYTES = 1_000;

export type PeerProtectedPane = "write" | "open";
export type PeerMessageDirection = "sent" | "received";
export type PeerMessageState = "prepared" | "sent" | "received" | "opened-once";

export interface PeerMessageReceipt {
  direction: PeerMessageDirection;
  state: PeerMessageState;
}

export interface PeerProtectedSheetModel {
  open: boolean;
  context: ManualPeerContext | null;
  personId: string | null;
  displayName: string;
  pane: PeerProtectedPane;
  ttlSeconds: LocalTtlSeconds;
  viewOnce: boolean;
  decryptDisplayEnabled: boolean;
  busy: boolean;
  draft: string;
  openDraft: string;
  coverText: string;
  openedPlaintext: string;
  receipt: PeerMessageReceipt | null;
  /**
   * Local evidence that this person completed their half of the symmetric
   * handshake — something of theirs has been opened here. Encrypting and
   * uploading always succeeds; it says nothing about whether the recipient
   * bound a context and can decrypt. Default false: unconfirmed until proven.
   */
  handshakeConfirmed: boolean;
  status: string;
}

export function blankPeerProtectedModel(open = false): PeerProtectedSheetModel {
  return {
    open,
    context: null,
    personId: null,
    displayName: "",
    pane: "write",
    ttlSeconds: LOCAL_TTL_OPTIONS[0],
    viewOnce: false,
    decryptDisplayEnabled: true,
    busy: false,
    draft: "",
    openDraft: "",
    coverText: "",
    openedPlaintext: "",
    receipt: null,
    handshakeConfirmed: false,
    status: "",
  };
}

/**
 * The carrier text is a pointer to ciphertext that only a peer who has added
 * your invite, verified you and approved this app + friend can open. Until one
 * of their messages has been opened here, OSL has no evidence they did any of
 * that, so a prepared carrier must not read as a delivered message.
 */
export function peerHandshakeWarning(model: PeerProtectedSheetModel): string {
  if (model.handshakeConfirmed) return "";
  const name = model.displayName || "This person";
  return `Nothing has ever been opened from ${name} here, so OSL cannot tell whether they finished their half. If they have not added your invite, verified you and approved this app + friend, this text stays unreadable for them however you send it.`;
}

export function boundedPeerProtectedDraft(value: string): string {
  if (utf8Length(value) <= PEER_PROTECTED_DRAFT_BYTES) return value;
  let bounded = "";
  for (const character of value) {
    if (utf8Length(bounded) + utf8Length(character) > PEER_PROTECTED_DRAFT_BYTES) break;
    bounded += character;
  }
  return bounded;
}

export function peerProtectedDraftByteFeedback(value: string): string {
  return `${utf8Length(value)} / ${PEER_PROTECTED_DRAFT_BYTES.toLocaleString("en-US")} bytes`;
}

function hasOslPeerIdentity(person: HubPerson): boolean {
  const discordSnowflake = /^[1-9]\d{16,19}$/u;
  return person.personId.length > 0
    && person.oslUserId.length > 0
    && person.safetyNumber.length > 0
    && person.personId !== person.oslUserId
    && !discordSnowflake.test(person.personId)
    && !discordSnowflake.test(person.oslUserId);
}

export function verifiedPeerFriends(people: HubPerson[]): HubPerson[] {
  return people.filter((person) => peerIsVerified(person)
    && hasOslPeerIdentity(person));
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function ttlLabel(seconds: LocalTtlSeconds): string {
  if (seconds === 3_600) return "1 hour";
  if (seconds === 86_400) return "1 day";
  if (seconds === 259_200) return "3 days";
  return "7 days";
}

function closeButton(): string {
  return `<button class="local-protected-close" id="local-protected-close" type="button" aria-label="Close protection">×</button>`;
}

function chooserMarkup(model: PeerProtectedSheetModel, people: HubPerson[]): string {
  const friends = verifiedPeerFriends(people);
  const rows = friends.length
    ? friends.map((person) => `<button class="peer-friend-row" type="button" data-peer-person="${escapeHtml(person.personId)}" ${model.busy ? "disabled" : ""}><span>${escapeHtml(person.alias ?? "Trusted person")}</span><small>${model.busy ? "Opening…" : "Verified in OSL"}</small></button>`).join("")
    : `<p class="peer-empty">Add and verify a person first.</p>`;
  return `<aside class="local-protected-sheet peer-protected-sheet" aria-labelledby="peer-protected-title">
    <header><div><span>Private message</span><h2 id="peer-protected-title">Protect</h2></div>${closeButton()}</header>
    <div class="peer-choice-list" aria-label="Trusted people">${rows}</div>
    <output class="local-protected-status" aria-live="polite">${escapeHtml(model.status)}</output>
    <button class="peer-local-choice" id="protect-local-only" type="button"><span>Only this device</span><small>Not person-to-person</small></button>
    <footer>Choose who should be able to open it. OSL does not read this page.</footer>
  </aside>`;
}

function approvalMarkup(model: PeerProtectedSheetModel): string {
  return `<aside class="local-protected-sheet peer-protected-sheet" aria-labelledby="peer-protected-title">
    <header><div><button class="peer-back" id="peer-protected-back" type="button">← Protect</button><h2 id="peer-protected-title">${escapeHtml(model.displayName)}</h2></div>${closeButton()}</header>
    <section class="peer-approval">
      <p>Allow this verified friend to open protected text exchanged through this app.</p>
      <button class="local-primary" id="peer-approve" type="button" ${model.busy ? "disabled" : ""}>${model.busy ? "Approving…" : "Approve app + friend"}</button>
      <output class="local-protected-status" aria-live="polite">${escapeHtml(model.status)}</output>
    </section>
    <footer>Limited to this app + friend. Nothing is sent automatically.</footer>
  </aside>`;
}

function readyMarkup(model: PeerProtectedSheetModel): string {
  const ttlOptions = LOCAL_TTL_OPTIONS.map((seconds) => `<option value="${seconds}" ${model.ttlSeconds === seconds ? "selected" : ""}>${ttlLabel(seconds)}</option>`).join("");
  const boundedDraft = boundedPeerProtectedDraft(model.draft);
  const write = `<form id="peer-protect-form" class="local-protected-form">
      <label for="peer-protected-draft">Message</label>
      <textarea id="peer-protected-draft" rows="5" autocomplete="off" spellcheck="true" aria-describedby="peer-protected-draft-bytes" placeholder="Write privately">${escapeHtml(boundedDraft)}</textarea>
      <small id="peer-protected-draft-bytes" class="peer-draft-bytes" aria-live="polite">${peerProtectedDraftByteFeedback(boundedDraft)}</small>
      <div class="local-protected-options"><label class="peer-ttl"><span>Relay copy expires after</span><select id="peer-protected-ttl">${ttlOptions}</select><small>Copies already opened remain.</small></label><label class="local-view-once"><span>View once</span><input id="peer-protected-view-once" type="checkbox" ${model.viewOnce ? "checked" : ""}/></label></div>
      <button class="local-primary" type="submit" ${model.busy ? "disabled" : ""}>${model.busy ? "Encrypting…" : "Encrypt & copy"}</button>
      <small class="local-send-truth">OSL copies protected text. It never presses Send.</small>
    </form>
    ${model.coverText ? `<section class="local-capsule-result"><label for="peer-cover-output">Protected text</label><textarea id="peer-cover-output" rows="4" readonly>${escapeHtml(model.coverText)}</textarea><button class="local-copy" id="peer-cover-copy" type="button">Copy again</button><small>Check where you paste it.</small>${peerHandshakeWarning(model) ? `<p class="peer-handshake-warning" role="status">${escapeHtml(peerHandshakeWarning(model))}</p>` : ""}</section>` : ""}`;
  const open = `<form id="peer-open-form" class="local-protected-form">
      <label for="peer-cover-input">Protected text</label>
      <textarea id="peer-cover-input" maxlength="262144" rows="6" autocomplete="off" spellcheck="false" placeholder="Paste here yourself">${escapeHtml(model.openDraft)}</textarea>
      <button class="local-primary" type="submit" ${model.busy ? "disabled" : ""}>${model.busy ? "Opening…" : "Open"}</button>
    </form>
    <label class="local-decrypt-display"><span><strong>Show decrypted text</strong><small>For this app + friend.</small></span><input id="peer-decrypt-display" type="checkbox" ${model.decryptDisplayEnabled ? "checked" : ""}/></label>
    ${model.openedPlaintext ? `<section class="local-plaintext-result"><span>Decrypted here</span><p>${escapeHtml(model.openedPlaintext)}</p></section>` : ""}`;
  const receipt = model.receipt
    ? `<div class="peer-message-receipt" role="status"><span>${model.receipt.direction === "sent" ? "You" : escapeHtml(model.displayName)}</span><strong>${model.receipt.state === "opened-once" ? "Received · opened once" : model.receipt.state === "received" ? "Received" : model.receipt.state === "sent" ? "Sent" : "Prepared"}</strong></div>`
    : "";
  return `<aside class="local-protected-sheet peer-protected-sheet ready" aria-labelledby="peer-protected-title">
    <header><div><button class="peer-back" id="peer-protected-back" type="button">← Protect</button><h2 id="peer-protected-title">${escapeHtml(model.displayName)}</h2></div>${closeButton()}</header>
    <nav class="local-protected-tabs" aria-label="Person-to-person protection"><button type="button" data-peer-pane="write" class="${model.pane === "write" ? "active" : ""}">Write</button><button type="button" data-peer-pane="open" class="${model.pane === "open" ? "active" : ""}">Open</button></nav>
    <div class="local-protected-body">${model.pane === "write" ? write : open}</div>
    ${receipt}<output class="local-protected-status" aria-live="polite">${escapeHtml(model.status)}</output>
    <footer>Manual copy & paste · person-to-person encryption · no page access</footer>
  </aside>`;
}

export function peerProtectedSheetMarkup(model: PeerProtectedSheetModel, people: HubPerson[]): string {
  if (!model.open) return "";
  if (!model.context) return chooserMarkup(model, people);
  return model.context.scopeApproved ? readyMarkup(model) : approvalMarkup(model);
}
