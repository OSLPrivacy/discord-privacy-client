import type { LocalLoopbackContext } from "./adapters";
import { placementFailureNoticeMarkup, type PlacementFailureNotice } from "./placement-failure";
import { COVER_MESSAGE_BOX_RULE, PROTECTED_TEXT_BOX_RULE } from "./protected-box-shortcuts";
import type { SendMode } from "./state";
import { VIEW_ONCE_DISPLAY_TRUTH, viewOnceControlMarkup } from "./view-once-tier";

export type LocalProtectedPane = "write" | "open";

export interface LocalProtectedSheetModel {
  open: boolean;
  chatLabel: string;
  context: LocalLoopbackContext | null;
  pane: LocalProtectedPane;
  ttlSeconds: number;
  viewOnce: boolean;
  /** TASK 0594. Pro on this device; creating a view once message needs it (0590). */
  viewOnceCreationAllowed: boolean;
  decryptDisplayEnabled: boolean;
  busy: boolean;
  draft: string;
  capsule: string;
  openedPlaintext: string;
  status: string;
  /**
   * TASK 3426. Set when OSL tried to put this draft into another app's message
   * box and could not. Optional so callers that never place keep their existing
   * models; `null` and absent both mean "nothing failed".
   */
  placementFailure?: PlacementFailureNotice | null;
}

export const LOCAL_CHAT_LABEL_MAX_LENGTH = 48;
export const LOCAL_TTL_OPTIONS = [3_600, 86_400, 259_200, 604_800] as const;
export type LocalTtlSeconds = (typeof LOCAL_TTL_OPTIONS)[number];
const STORAGE_PREFIX = "osl-local-loopback-context-v1";

export function isLocalTtlSeconds(value: number): value is LocalTtlSeconds {
  return LOCAL_TTL_OPTIONS.includes(value as LocalTtlSeconds);
}

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
    ttlSeconds: LOCAL_TTL_OPTIONS[0],
    viewOnce: false,
    viewOnceCreationAllowed: false,
    decryptDisplayEnabled: true,
    busy: false,
    draft: "",
    capsule: "",
    openedPlaintext: "",
    status: "",
    placementFailure: null,
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

function ttlLabel(seconds: LocalTtlSeconds): string {
  if (seconds === 3_600) return "1 hour";
  if (seconds === 86_400) return "1 day";
  if (seconds === 259_200) return "3 days";
  return "7 days";
}

/** Render the canonical picker inside this detached sheet window. */
function canonicalTtlPicker(id: string, value: LocalTtlSeconds): string {
  const selectedLabel = ttlLabel(value);
  const options = LOCAL_TTL_OPTIONS.map((seconds) => {
    const selected = seconds === value;
    const label = ttlLabel(seconds);
    return `<button class="canonical-select-option" type="button" role="option" aria-selected="${selected}" data-value="${seconds}" data-label="${label}" onclick="const p=this.closest('.canonical-select');const s=p.querySelector('select');s.value=this.dataset.value;s.dispatchEvent(new Event('change',{bubbles:true}));p.querySelector('.canonical-select-trigger-label').textContent=this.dataset.label;p.querySelectorAll('.canonical-select-option').forEach((o)=>{const chosen=o===this;o.setAttribute('aria-selected',String(chosen));o.querySelector('input').checked=chosen;});p.closest('details').open=false;"><input tabindex="-1" aria-hidden="true" type="checkbox" ${selected ? "checked" : ""}/><span>${label}</span></button>`;
  }).join("");
  const nativeOptions = LOCAL_TTL_OPTIONS.map((seconds) => `<option value="${seconds}" ${value === seconds ? "selected" : ""}>${ttlLabel(seconds)}</option>`).join("");
  return `<details class="canonical-select"><summary class="canonical-select-trigger" role="button" aria-haspopup="listbox"><span class="canonical-select-trigger-label">${selectedLabel}</span><span aria-hidden="true">⌄</span></summary><select class="canonical-select-native" id="${id}" tabindex="-1" aria-hidden="true">${nativeOptions}</select><div class="canonical-select-list" role="listbox" aria-label="Opening authorization expires after">${options}</div></details>`;
}

export function localProtectedSheetMarkup(model: LocalProtectedSheetModel, sendMode: SendMode = "manual"): string {
  if (!model.open) return "";
  const maxCopyPayloadBytes = 1_000;
  const utf8Length = (value: string): number => new TextEncoder().encode(value).length;
  const boundedCopyPayload = (value: string): { value: string; bytes: number; clipped: boolean } => {
    let bounded = "";
    let bytes = 0;
    for (const character of value) {
      const characterBytes = utf8Length(character);
      if (bytes + characterBytes > maxCopyPayloadBytes) return { value: bounded, bytes, clipped: true };
      bounded += character;
      bytes += characterBytes;
    }
    return { value: bounded, bytes, clipped: false };
  };
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

  const boundedDraft = boundedCopyPayload(model.draft);
  const draftLimitNotice = !boundedDraft.clipped
    ? `${boundedDraft.bytes.toLocaleString("en-US")} / ${maxCopyPayloadBytes.toLocaleString("en-US")} bytes`
    : `Draft shortened to ${maxCopyPayloadBytes.toLocaleString("en-US")} bytes for copy.`;
  const manualMode = sendMode === "manual";
  const experimentalMode = sendMode === "double" || sendMode === "single";
  const primaryLabel = manualMode ? "Encrypt & prepare" : experimentalMode ? "Encrypt & copy fallback" : "Encrypt & copy";
  const sendTruth = manualMode
    ? "OSL prepares encrypted text below. You place it yourself; OSL does not copy or send."
    : experimentalMode
      ? `${sendMode === "double" ? "Double Enter" : "Single Enter"} is unavailable in this local sheet. After consent, OSL copies encrypted text and sends nothing.`
      : "OSL copies encrypted text only. You choose where to paste it and press Send yourself.";
  const resultCopyLabel = manualMode ? "Copy to clipboard" : "Copy again";
  const resultHint = manualMode ? "Select and place this encrypted text yourself, or copy only by pressing the button." : "Review the destination before you send.";
  // TASK 3426: a failed placement is announced directly above the person's own
  // text, so the sentence and the draft it is about are read together.
  const placementFailure = placementFailureNoticeMarkup(model.placementFailure ?? null);
  const write = `${placementFailure}<form id="local-protect-form" class="local-protected-form">
      <label for="local-protected-draft">Message</label>
      <textarea id="local-protected-draft" maxlength="1000" data-max-bytes="${maxCopyPayloadBytes}" data-osl-protected-box-rule="${PROTECTED_TEXT_BOX_RULE}" rows="5" autocomplete="off" spellcheck="true" aria-describedby="local-protected-draft-bytes" placeholder="Write privately">${escapeHtml(boundedDraft.value)}</textarea>
      <small id="local-protected-draft-bytes" class="local-draft-bytes" aria-live="polite">${draftLimitNotice}</small>
      <div class="local-protected-options"><label><span>Opening authorization expires after</span>${canonicalTtlPicker("local-protected-ttl", model.ttlSeconds as LocalTtlSeconds)}</label>${viewOnceControlMarkup({
        id: "local-protected-view-once",
        layout: "compact",
        className: "local-view-once",
        checked: model.viewOnce,
        creationAllowed: model.viewOnceCreationAllowed,
        detail: VIEW_ONCE_DISPLAY_TRUTH,
      })}</div>
      <small class="local-authorization-truth">After expiry, OSL refuses to open this text on this device.</small>
      <button class="local-primary" type="submit" ${model.busy ? "disabled" : ""}>${model.busy ? "Encrypting…" : primaryLabel}</button>
      <small class="local-send-truth">${escapeHtml(sendTruth)}</small>
    </form>
    ${model.capsule ? `<section class="local-capsule-result"><label for="local-capsule-output">Encrypted text</label><textarea id="local-capsule-output" rows="4" readonly data-osl-cover-message-box="${COVER_MESSAGE_BOX_RULE}">${escapeHtml(model.capsule)}</textarea><button class="local-copy" id="local-capsule-copy" type="button">${escapeHtml(resultCopyLabel)}</button><small>${escapeHtml(resultHint)}</small></section>` : ""}`;

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
    <footer>${manualMode ? "Manual placement" : "Clipboard handoff"} · no page access · not person-to-person E2EE</footer>
  </aside>`;
}
