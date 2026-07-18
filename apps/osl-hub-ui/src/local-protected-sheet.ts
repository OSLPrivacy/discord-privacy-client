import type { LocalLoopbackContext } from "./adapters";

export type LocalProtectedPane = "write" | "open";

export interface LocalProtectedSheetModel {
  open: boolean;
  chatLabel: string;
  context: LocalLoopbackContext | null;
  pane: LocalProtectedPane;
  ttlSeconds: number;
  viewOnce: boolean;
  busy: boolean;
  capsule: string;
  openedPlaintext: string;
  status: string;
}

export const LOCAL_CHAT_LABEL_MAX_LENGTH = 48;
export const LOCAL_TTL_OPTIONS = [0, 3_600, 86_400, 604_800] as const;
const STORAGE_PREFIX = "osl-local-loopback-context-v1";

export function validLocalChatLabel(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.length > 0
    && trimmed.length <= LOCAL_CHAT_LABEL_MAX_LENGTH
    && !/[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069]/u.test(trimmed);
}

export function localConversationStorageKey(serviceId: string, accountId: string): string {
  if (!/^[a-z0-9_-]{1,32}$/u.test(serviceId) || !/^[A-Za-z0-9._:-]{1,160}$/u.test(accountId)) {
    throw new Error("invalid local profile identity");
  }
  return `${STORAGE_PREFIX}:${serviceId}:${accountId}`;
}

export function loadOrCreateLocalConversationId(
  storage: Pick<Storage, "getItem" | "setItem">,
  serviceId: string,
  accountId: string,
  randomBytes: (bytes: Uint8Array) => Uint8Array = (bytes) => crypto.getRandomValues(bytes),
): string {
  const key = localConversationStorageKey(serviceId, accountId);
  const existing = storage.getItem(key);
  if (existing && /^local-[a-f0-9]{32}$/u.test(existing)) return existing;
  const bytes = randomBytes(new Uint8Array(16));
  if (bytes.length !== 16) throw new Error("local random source failed");
  const id = `local-${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
  storage.setItem(key, id);
  return id;
}

export function blankLocalProtectedModel(open = false): LocalProtectedSheetModel {
  return {
    open,
    chatLabel: "",
    context: null,
    pane: "write",
    ttlSeconds: 0,
    viewOnce: false,
    busy: false,
    capsule: "",
    openedPlaintext: "",
    status: "",
  };
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

function ttlLabel(seconds: number): string {
  if (seconds === 3_600) return "1 hour";
  if (seconds === 86_400) return "1 day";
  if (seconds === 604_800) return "7 days";
  return "No timer";
}

export function localProtectedSheetMarkup(model: LocalProtectedSheetModel): string {
  if (!model.open) return "";
  const close = `<button class="local-protected-close" id="local-protected-close" type="button" aria-label="Close local protection">×</button>`;
  if (!model.context) {
    return `<aside class="local-protected-sheet" aria-labelledby="local-protected-title">
      <header><div><span>On this device</span><h2 id="local-protected-title">Protect locally</h2></div>${close}</header>
      <form id="local-context-form" class="local-context-form">
        <label for="local-chat-label">Chat name</label>
        <input id="local-chat-label" maxlength="${LOCAL_CHAT_LABEL_MAX_LENGTH}" autocomplete="off" spellcheck="false" placeholder="e.g. Rose" value="${escapeHtml(model.chatLabel)}" autofocus/>
        <p>Only a random ID is saved. OSL cannot see the service page.</p>
        <button class="local-primary" type="submit" ${model.busy ? "disabled" : ""}>${model.busy ? "Starting…" : "Start"}</button>
        <output class="local-protected-status" aria-live="polite">${escapeHtml(model.status)}</output>
      </form>
    </aside>`;
  }

  const ttlOptions = LOCAL_TTL_OPTIONS.map((seconds) => `<option value="${seconds}" ${model.ttlSeconds === seconds ? "selected" : ""}>${ttlLabel(seconds)}</option>`).join("");
  const write = `<form id="local-protect-form" class="local-protected-form">
      <label for="local-protected-draft">Message</label>
      <textarea id="local-protected-draft" maxlength="1000" rows="5" autocomplete="off" spellcheck="true" placeholder="Write privately"></textarea>
      <div class="local-protected-options"><label><span>Delete key after</span><select id="local-protected-ttl">${ttlOptions}</select></label><label class="local-view-once"><span>View once</span><input id="local-protected-view-once" type="checkbox" ${model.viewOnce ? "checked" : ""}/></label></div>
      <button class="local-primary" type="submit" ${model.busy ? "disabled" : ""}>${model.busy ? "Encrypting…" : "Encrypt"}</button>
    </form>
    ${model.capsule ? `<section class="local-capsule-result"><label for="local-capsule-output">Encrypted text</label><textarea id="local-capsule-output" rows="4" readonly>${escapeHtml(model.capsule)}</textarea><button class="local-copy" id="local-capsule-copy" type="button">Copy</button><small>Paste and send it yourself.</small></section>` : ""}`;

  const open = `<form id="local-open-form" class="local-protected-form">
      <label for="local-capsule-input">Encrypted text</label>
      <textarea id="local-capsule-input" maxlength="262144" rows="6" autocomplete="off" spellcheck="false" placeholder="Paste here yourself"></textarea>
      <button class="local-primary" type="submit" ${model.busy ? "disabled" : ""}>${model.busy ? "Opening…" : "Open locally"}</button>
    </form>
    ${model.openedPlaintext ? `<section class="local-plaintext-result"><span>On this device</span><p>${escapeHtml(model.openedPlaintext)}</p></section>` : ""}`;

  return `<aside class="local-protected-sheet ready" aria-labelledby="local-protected-title">
    <header><div><span>On this device</span><h2 id="local-protected-title">${escapeHtml(model.chatLabel)}</h2></div>${close}</header>
    <nav class="local-protected-tabs" aria-label="Local protection"><button type="button" data-local-pane="write" class="${model.pane === "write" ? "active" : ""}">Write</button><button type="button" data-local-pane="open" class="${model.pane === "open" ? "active" : ""}">Open</button></nav>
    <div class="local-protected-body">${model.pane === "write" ? write : open}</div>
    <output class="local-protected-status" aria-live="polite">${escapeHtml(model.status)}</output>
    <footer>Manual copy & paste · no page access · not person-to-person E2EE</footer>
  </aside>`;
}
