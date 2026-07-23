import "@fontsource-variable/inter/wght.css";
import "./overlay.css";
import { burnNativeDiscordOverlayChat, getNativeDiscordOverlayState, listNativeDiscordOverlayAttachments, openNativeDiscordOverlayAttachment, openNativeDiscordOverlayText, prepareNativeDiscordOverlayText, revealNativeDiscordOverlayViewOnce, selectNativeDiscordOverlayAttachment, sendNativeDiscordOverlayCarrier, setNativeDiscordOverlaySecurity, type NativeDiscordCarrierLayout, type NativeDiscordCarrierMode } from "./native-overlay-adapter";
import { boundedProtectedDraft, MAX_PROTECTED_DRAFT_BYTES, NATIVE_OVERLAY_TTL_OPTIONS, overlayExpiryDelayMs, PROTECTED_DRAFT_WARNING_BYTES, type NativeOverlayTtlSeconds, utf8Length } from "./overlay-state";
import { OverlaySendGesture, type OverlaySendMode } from "./overlay-send-gesture";
import { CoarseTypingRate } from "./coarse-typing-rate";
import { TwoStepBurnConfirmation } from "./two-step-burn";
import {
  createDiscordProtectedTranscript,
  type DiscordProtectedTranscriptRow,
  type VerifiedOslTranscriptIdentity,
} from "./discord-protected-transcript";

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error("Trusted composer overlay is incomplete");
  return element;
}

const draft = requireElement<HTMLTextAreaElement>("#protected-draft");
const counter = requireElement<HTMLElement>("#draft-bytes");
const friendLabel = requireElement<HTMLElement>("#friend-label");
const ttl = requireElement<HTMLSelectElement>("#protected-ttl");
const viewOnce = requireElement<HTMLInputElement>("#protected-view-once");
const sendMode = requireElement<HTMLSelectElement>("#protected-send-mode");
const placementMode = requireElement<HTMLSelectElement>("#protected-placement-mode");
const decryptDisplay = requireElement<HTMLInputElement>("#protected-decrypt-display");
const currentExpiry = requireElement<HTMLElement>("#current-expiry");
const prepare = requireElement<HTMLButtonElement>("#prepare-protected");
const chooseAttachment = requireElement<HTMLButtonElement>("#choose-attachment");
const coverText = requireElement<HTMLButtonElement>("#covertext-mode");
const burnChat = requireElement<HTMLButtonElement>("#burn-protected-chat");
const status = requireElement<HTMLOutputElement>("#overlay-status");
const messageList = requireElement<HTMLElement>("#osl-message-list");

let composing = false;
let overlayReady = false;
let overlayInitRetryMs = 250;
let decryptDisplayEnabled = false;
let viewOnceEnabled = false;
let attachmentsEnabled = false;
let discordMarkerAvailable = false;
let coverTextEnabled = true;
let confirmedTtlSeconds: NativeOverlayTtlSeconds = NATIVE_OVERLAY_TTL_OPTIONS[0];
let securityBusy = false;
let receiveBusy = false;
let sendBusy = false;
let attachmentBusy = false;
let draftTooLarge = false;
let receiveTimer: number | undefined;
let gestureTimer: number | undefined;
let burnTimer: number | undefined;
let idlePollMs = 2_000;
const sendGesture = new OverlaySendGesture();
const typingRate = new CoarseTypingRate();
const burnConfirmation = new TwoStepBurnConfirmation();
const messageExpiryTimers = new Map<HTMLLIElement, number>();
const viewOnceBubbles = new Set<HTMLLIElement>();
const receivedPlaintextBubbles = new Set<HTMLLIElement>();
const outgoingBubbles = new Map<string, HTMLLIElement>();
const pendingAttachmentIds = new Set<string>();
const pendingViewOnceIds = new Set<string>();
const transcriptRows: DiscordProtectedTranscriptRow[] = [];
const transcriptActions = new Map<string, () => void>();
let transcriptSequence = 0;
let verifiedFriendIdentity: VerifiedOslTranscriptIdentity = {
  id: "verified-friend",
  displayName: "Private message",
  avatarFallback: "?",
  provenance: "verified-osl",
};
const localIdentity: VerifiedOslTranscriptIdentity = {
  id: "local-user",
  displayName: "You",
  avatarFallback: "Y",
  provenance: "verified-osl",
};

const transcript = createDiscordProtectedTranscript({
  document,
  ariaLabel: "Messages prepared or opened in this OSL panel",
  preferences: { theme: "discord-dark", density: "cozy", zoom: 1 },
  window: { rows: [], startIndex: 0, totalRowCount: 0 },
  onAction(actionId) { transcriptActions.get(actionId)?.(); },
});
messageList.append(transcript.root);

function syncTranscript(): void {
  transcript.updateWindow({ rows: transcriptRows, startIndex: 0, totalRowCount: transcriptRows.length });
}

function transcriptTimestamp(epochMs = Date.now()) {
  return {
    epochMs,
    label: new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(epochMs),
  };
}

function receiptStatus(label: string): "sent" | "received" | "opened" | "expired" {
  if (/opened/iu.test(label)) return "opened";
  if (/received/iu.test(label)) return "received";
  if (/expired/iu.test(label)) return "expired";
  return "sent";
}

function rowElement(key: string): HTMLLIElement {
  const item = transcript.root.querySelector<HTMLLIElement>(`[data-row-key="${CSS.escape(key)}"]`);
  if (!item) throw new Error("Protected transcript row was not rendered");
  return item;
}

function reconcileDraft(): void {
  const bounded = boundedProtectedDraft(draft.value);
  if (bounded !== draft.value) draft.value = bounded;
  const bytes = utf8Length(bounded);
  draftTooLarge = bytes > MAX_PROTECTED_DRAFT_BYTES;
  counter.textContent = draftTooLarge
    ? "Message is too large to send privately."
    : bytes >= PROTECTED_DRAFT_WARNING_BYTES
      ? `${Math.ceil((MAX_PROTECTED_DRAFT_BYTES - bytes) / 1024)} KiB remaining.`
      : "";
  refreshControls();
}

draft.addEventListener("compositionstart", () => { composing = true; });
draft.addEventListener("compositionend", () => { composing = false; reconcileDraft(); });
draft.addEventListener("input", (event) => {
  if (event.isTrusted) {
    typingRate.recordTrustedInput(performance.now(), true, Array.from(draft.value).length);
  }
  if (!composing) reconcileDraft();
  if (!draft.value) typingRate.reset();
});

function refreshControls(): void {
  prepare.disabled = sendBusy || !overlayReady || draftTooLarge;
  chooseAttachment.hidden = !attachmentsEnabled;
  chooseAttachment.disabled = sendBusy || attachmentBusy || !overlayReady || !attachmentsEnabled;
  coverText.disabled = sendBusy || !overlayReady || !discordMarkerAvailable;
  burnChat.disabled = sendBusy || !overlayReady;
  sendMode.disabled = sendBusy || !overlayReady;
  placementMode.disabled = sendBusy || !overlayReady || !discordMarkerAvailable;
  ttl.disabled = sendBusy || securityBusy || !overlayReady;
  decryptDisplay.disabled = sendBusy || securityBusy || !overlayReady;
  viewOnce.disabled = sendBusy || !overlayReady || !viewOnceEnabled;
}

function setBusy(busy: boolean): void {
  sendBusy = busy;
  refreshControls();
}

setBusy(true);

function removeBubble(item: HTMLLIElement): void {
  const timer = messageExpiryTimers.get(item);
  if (timer !== undefined) window.clearTimeout(timer);
  messageExpiryTimers.delete(item);
  viewOnceBubbles.delete(item);
  receivedPlaintextBubbles.delete(item);
  for (const [messageId, bubble] of outgoingBubbles) {
    if (bubble === item) outgoingBubbles.delete(messageId);
  }
  const attachmentId = item.dataset.attachmentId;
  if (attachmentId) pendingAttachmentIds.delete(attachmentId);
  const viewOnceId = item.dataset.viewOnceId;
  if (viewOnceId) pendingViewOnceIds.delete(viewOnceId);
  for (const action of item.querySelectorAll<HTMLElement>("[data-transcript-action]")) {
    const actionId = action.dataset.transcriptAction;
    if (!actionId) continue;
    transcriptActions.delete(actionId);
    if (actionId.startsWith("attachment:")) pendingAttachmentIds.delete(actionId.slice("attachment:".length));
    if (actionId.startsWith("view-once:")) pendingViewOnceIds.delete(actionId.slice("view-once:".length));
  }
  const key = item.dataset.rowKey;
  if (key) {
    const index = transcriptRows.findIndex((row) => row.key === key);
    if (index >= 0) transcriptRows.splice(index, 1);
  }
  item.textContent = "";
  syncTranscript();
}

function removeViewOnceBubbles(): void {
  for (const item of [...viewOnceBubbles]) removeBubble(item);
}

function applyDecryptDisplayVisibility(visible: boolean): void {
  if (!visible) removeViewOnceBubbles();
  for (const item of receivedPlaintextBubbles) {
    const key = item.dataset.rowKey;
    const row = transcriptRows.find((candidate) => candidate.key === key);
    if (row?.kind === "text" || row?.kind === "reply") row.plaintextHidden = !visible;
    const body = item.querySelector("p");
    if (body) body.hidden = !visible;
  }
  for (const row of transcriptRows) {
    if (row.kind === "receipt" && row.action?.id.startsWith("view-once:")) row.action.disabled = !visible;
  }
  syncTranscript();
  for (const item of transcript.root.querySelectorAll<HTMLLIElement>('[data-row-kind="receipt"]')) {
    const reveal = item.querySelector<HTMLButtonElement>("button");
    if (reveal) reveal.disabled = !visible;
  }
}

function clearMessageBubbles(): void {
  for (const item of [...messageExpiryTimers.keys()]) removeBubble(item);
  receivedPlaintextBubbles.clear();
  transcriptRows.splice(0);
  transcriptActions.clear();
  syncTranscript();
}

function appendBubble(direction: "outgoing" | "incoming", plaintext: string, receipt: string, expiresAt: number, viewOnceMessage: boolean): HTMLLIElement {
  const key = `message-${++transcriptSequence}`;
  transcriptRows.push({
    key,
    kind: "text",
    direction,
    author: direction === "outgoing" ? localIdentity : verifiedFriendIdentity,
    timestamp: transcriptTimestamp(),
    plaintext,
    plaintextHidden: direction === "incoming" && !viewOnceMessage && !decryptDisplayEnabled,
    receipt: { status: receiptStatus(receipt), label: receipt },
  });
  syncTranscript();
  const item = rowElement(key);
  item.classList.add(direction);
  const body = item.querySelector<HTMLParagraphElement>(".osl-discord-transcript__plaintext");
  if (direction === "incoming" && !viewOnceMessage) {
    receivedPlaintextBubbles.add(item);
    if (body) body.hidden = !decryptDisplayEnabled;
  }
  if (viewOnceMessage) viewOnceBubbles.add(item);
  const expiryTimer = window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(expiresAt, Date.now()));
  messageExpiryTimers.set(item, expiryTimer);
  while (transcriptRows.length > 24) {
    const oldest = transcript.root.querySelector<HTMLLIElement>("[data-row-key]");
    if (oldest) removeBubble(oldest);
    else break;
  }
  item.scrollIntoView({ block: "nearest" });
  return item;
}

function applyAcknowledgment(messageId: string, receipt: "received" | "opened"): void {
  const item = outgoingBubbles.get(messageId);
  const state = item?.querySelector<HTMLElement>(".osl-discord-transcript__receipt");
  if (!item || !state) return;
  const label = receipt === "opened" ? "Opened in OSL" : "Received by OSL";
  state.textContent = label;
  const row = transcriptRows.find((candidate) => candidate.key === item.dataset.rowKey);
  if ((row?.kind === "text" || row?.kind === "reply") && row.receipt) {
    row.receipt = { status: receipt, label };
    syncTranscript();
  }
  if (receipt === "opened") outgoingBubbles.delete(messageId);
}

function appendPendingAttachment(attachment: { attachmentId: string; originalFilename: string; plaintextSize: number; expiresAt: number; viewOnce: boolean }): void {
  if (pendingAttachmentIds.has(attachment.attachmentId)) return;
  pendingAttachmentIds.add(attachment.attachmentId);
  const key = `attachment-${++transcriptSequence}`;
  const actionId = `attachment:${attachment.attachmentId}`;
  const sizeLabel = attachment.plaintextSize >= 1024 * 1024
    ? `${(attachment.plaintextSize / (1024 * 1024)).toFixed(1)} MB`
    : `${Math.ceil(attachment.plaintextSize / 1024)} KB`;
  transcriptRows.push({
    key,
    kind: "text",
    direction: "incoming",
    author: verifiedFriendIdentity,
    timestamp: transcriptTimestamp(),
    plaintext: attachment.originalFilename,
    receipt: { status: "received", label: `${sizeLabel}${attachment.viewOnce ? " · view once" : ""}` },
    media: [{
      key: attachment.attachmentId,
      kind: "file",
      label: attachment.originalFilename,
      detail: sizeLabel,
      state: "available",
      action: { id: actionId, label: "Open privately" },
    }],
  });
  syncTranscript();
  const item = rowElement(key);
  item.classList.add("incoming", "attachment");
  item.dataset.attachmentId = attachment.attachmentId;
  transcriptActions.set(actionId, () => void (async () => {
    if (attachmentBusy) return;
    const open = item.querySelector<HTMLButtonElement>("button");
    if (!open) return;
    attachmentBusy = true;
    open.disabled = true;
    refreshControls();
    status.textContent = "Authenticating attachment…";
    const result = await openNativeDiscordOverlayAttachment(attachment.attachmentId);
    attachmentBusy = false;
    refreshControls();
    if (!result) {
      open.disabled = false;
      status.textContent = "That attachment could not be opened safely.";
      return;
    }
    removeBubble(item);
    status.textContent = result.viewOnceConsumed
      ? "Opened once in OSL's protected viewer."
      : "Opened in OSL's private viewer.";
  })());
  const expiryTimer = window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(attachment.expiresAt, Date.now()));
  messageExpiryTimers.set(item, expiryTimer);
}

function appendPendingViewOnce(message: { messageId: string; expiresAt: number }): void {
  if (pendingViewOnceIds.has(message.messageId)) return;
  pendingViewOnceIds.add(message.messageId);
  const key = `view-once-${++transcriptSequence}`;
  const actionId = `view-once:${message.messageId}`;
  const body = document.createElement("p");
  body.textContent = "View-once message";
  transcriptRows.push({
    key,
    kind: "receipt",
    direction: "incoming",
    author: verifiedFriendIdentity,
    timestamp: transcriptTimestamp(),
    status: "received",
    label: `${body.textContent} · Received by OSL · unopened`,
    action: { id: actionId, label: "Reveal once", disabled: !decryptDisplayEnabled },
  });
  syncTranscript();
  const item = rowElement(key);
  item.classList.add("incoming", "view-once-pending");
  item.dataset.viewOnceId = message.messageId;
  transcriptActions.set(actionId, () => void (async () => {
    if (receiveBusy || !decryptDisplayEnabled) return;
    const reveal = item.querySelector<HTMLButtonElement>("button");
    if (!reveal) return;
    reveal.textContent = "Reveal once";
    receiveBusy = true;
    reveal.disabled = !decryptDisplayEnabled;
    reveal.disabled = true;
    status.textContent = "Opening view-once message…";
    const opened = await revealNativeDiscordOverlayViewOnce(message.messageId);
    receiveBusy = false;
    if (!opened || !opened.viewOnceConsumed) {
      reveal.disabled = false;
      status.textContent = "That view-once message could not be opened safely.";
      scheduleReceivePoll(idlePollMs);
      return;
    }
    removeBubble(item);
    appendBubble("incoming", opened.plaintext, "Received · opened once", opened.expiresAt, true);
    status.textContent = "View-once message opened in OSL.";
    scheduleReceivePoll(0);
  })());
  const expiryTimer = window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(message.expiresAt, Date.now()));
  messageExpiryTimers.set(item, expiryTimer);
}

function scheduleReceivePoll(delayMs: number): void {
  if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
  receiveTimer = undefined;
  if (!overlayReady || !decryptDisplayEnabled || document.hidden) return;
  receiveTimer = window.setTimeout(() => void pollReceived(), delayMs);
}

async function pollReceived(): Promise<void> {
  if (receiveBusy || !overlayReady || !decryptDisplayEnabled || document.hidden) return;
  receiveBusy = true;
  try {
    const batch = await openNativeDiscordOverlayText();
    if (!batch) throw new Error("invalid receive response");
    for (const message of batch.messages) {
      appendBubble("incoming", message.plaintext, message.viewOnceConsumed ? "Received · opened once" : "Received · opened", message.expiresAt, message.viewOnceConsumed);
    }
    for (const message of batch.pendingViewOnce) appendPendingViewOnce(message);
    for (const acknowledgment of batch.acknowledgments) {
      applyAcknowledgment(acknowledgment.messageId, acknowledgment.status);
    }
    const attachments = attachmentsEnabled ? await listNativeDiscordOverlayAttachments() : [];
    if (!attachments) throw new Error("invalid attachment response");
    for (const attachment of attachments) appendPendingAttachment(attachment);
    idlePollMs = batch.messages.length > 0 || batch.pendingViewOnce.length > 0 || attachments.length > 0 ? 2_000 : Math.min(idlePollMs * 2, 10_000);
    if (batch.messages.length > 0) status.textContent = `${batch.messages.length} private ${batch.messages.length === 1 ? "message" : "messages"} received through OSL.`;
  } catch {
    idlePollMs = Math.min(idlePollMs * 2, 10_000);
  } finally {
    receiveBusy = false;
    scheduleReceivePoll(idlePollMs);
  }
}

document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    removeViewOnceBubbles();
    if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
    receiveTimer = undefined;
  } else {
    idlePollMs = 2_000;
    scheduleReceivePoll(0);
  }
});
window.addEventListener("blur", removeViewOnceBubbles);

function measuredCarrierLayout(): NativeDiscordCarrierLayout | undefined {
  const style = window.getComputedStyle(draft);
  const fontSize = Number.parseFloat(style.fontSize);
  const lineHeight = Number.parseFloat(style.lineHeight);
  const paddingInline = Number.parseFloat(style.paddingLeft) + Number.parseFloat(style.paddingRight);
  const contentWidth = draft.clientWidth - paddingInline;
  if (![fontSize, lineHeight, paddingInline, contentWidth].every(Number.isFinite)
    || fontSize <= 0 || lineHeight <= 0 || contentWidth <= 0) return undefined;
  return {
    contentWidthPx: contentWidth,
    // A bounded font-only estimate; no draft text or per-character cadence is measured.
    averageGraphemeWidthPx: fontSize * 0.56,
    lineHeightPx: lineHeight,
    zoom: 1,
    density: 1,
    padding: "shapeMatched",
    rowKind: "plainText",
  };
}

async function sendDraft(): Promise<void> {
  if (sendBusy || !overlayReady) return;
  const plaintext = boundedProtectedDraft(draft.value);
  if (!plaintext) { status.textContent = "Write a message first."; return; }
  if (utf8Length(plaintext) > MAX_PROTECTED_DRAFT_BYTES) { status.textContent = "This private message is too large."; return; }
  setBusy(true);
  status.textContent = "Encrypting…";
  const requestedViewOnce = viewOnce.checked;
  try {
    const refreshedState = await getNativeDiscordOverlayState();
    if (!refreshedState) throw new Error("overlay state changed");
    coverTextEnabled = refreshedState.covertextEnabled;
    const result = await prepareNativeDiscordOverlayText(plaintext, requestedViewOnce);
    if (!result || result.viewOnce !== requestedViewOnce) throw new Error("invalid protected response");
    let markerSent = false;
    if (discordMarkerAvailable && coverTextEnabled) {
      const requestedPlacement: NativeDiscordCarrierMode = placementMode.value === "compatibility" ? "compatibility" : "atomic";
      const charsPerSecond = typingRate.charsPerSecond();
      const carrier = await sendNativeDiscordOverlayCarrier(requestedPlacement, charsPerSecond, measuredCarrierLayout());
      markerSent = carrier?.status === "sent" && carrier.placed && carrier.enterSent;
    }
    const outgoing = appendBubble("outgoing", plaintext, result.viewOnce
      ? `Sent to OSL · view once${markerSent ? " · Discord marked" : " · OSL only"}`
      : `Sent to OSL${markerSent ? " · Discord marked" : " · OSL only"}`, result.expiresAt, result.viewOnce);
    outgoingBubbles.set(result.messageId, outgoing);
    draft.value = "";
    typingRate.reset();
    reconcileDraft();
    status.textContent = !discordMarkerAvailable
      ? "Sent privately through OSL only. No Discord marker was attempted."
      : !coverTextEnabled
        ? "Sent privately through OSL only. Covertext off · private messages travel through OSL only."
      : markerSent
      ? "Sent privately through OSL. Discord received only the private-message marker."
      : "Sent privately through OSL. Discord changed, so its marker was not sent.";
  } catch {
    status.textContent = "Protection stopped safely. Your draft is still here.";
  } finally {
    typingRate.reset();
    setBusy(false);
  }
}

prepare.addEventListener("click", () => void sendDraft());

coverText.addEventListener("click", () => {
  if (!overlayReady || !discordMarkerAvailable || sendBusy) return;
  status.textContent = "Use Covertext in the trusted OSL header.";
});

chooseAttachment.addEventListener("click", () => void (async () => {
  if (attachmentBusy || !overlayReady || !attachmentsEnabled) return;
  attachmentBusy = true;
  refreshControls();
  status.textContent = "Choose a file up to 500 MB…";
  const result = await selectNativeDiscordOverlayAttachment(viewOnce.checked);
  attachmentBusy = false;
  refreshControls();
  if (result === "cancelled") {
    status.textContent = "Attachment canceled.";
    return;
  }
  if (!result) {
    status.textContent = "Attachment protection stopped safely.";
    return;
  }
  appendBubble(
    "outgoing",
    result.originalFilename,
    result.viewOnce ? "Sent to OSL · attachment · view once" : "Sent to OSL · attachment",
    result.expiresAt,
    result.viewOnce,
  );
  status.textContent = "Attachment sent privately through OSL.";
})());

function resetBurnConfirmation(): void {
  burnConfirmation.reset();
  if (burnTimer !== undefined) window.clearTimeout(burnTimer);
  burnTimer = undefined;
  burnChat.textContent = "Burn";
}

burnChat.addEventListener("click", (event) => {
  const step = burnConfirmation.step(performance.now(), event.isTrusted);
  if (step === "ignored") return;
  if (step === "armed") {
    burnChat.textContent = "Confirm burn";
    status.textContent = "This removes this OSL chat locally and tries to delete its remote OSL blobs. Discord history and recipient copies stay untouched. Click again to confirm.";
    if (burnTimer !== undefined) window.clearTimeout(burnTimer);
    burnTimer = window.setTimeout(() => {
      burnTimer = undefined;
      if (burnConfirmation.expire(performance.now())) {
        burnChat.textContent = "Burn";
        status.textContent = "Burn confirmation expired. Nothing was deleted.";
      }
    }, 10_000);
    return;
  }
  void (async () => {
    if (burnTimer !== undefined) window.clearTimeout(burnTimer);
    burnTimer = undefined;
    setBusy(true);
    burnChat.textContent = "Burning…";
    status.textContent = "Burning this OSL chat…";
    const result = await burnNativeDiscordOverlayChat();
    if (!result) {
      resetBurnConfirmation();
      setBusy(false);
      status.textContent = "Burn stopped safely. Review this chat before trying again.";
      return;
    }
    const remote = result.remoteBlobDeletionsFailed === 0
      ? `${result.remoteBlobsDeleted} remote OSL blobs deleted.`
      : `${result.remoteBlobsDeleted} remote OSL blobs deleted; ${result.remoteBlobDeletionsFailed} could not be deleted and remain tracked for retry.`;
    clearMessageBubbles();
    status.textContent = `OSL chat burned. ${result.localProtectedRowsDestroyed} local protected rows removed. ${remote} Discord history and recipient copies were not deleted.`;
  })();
});

function clearGestureTimer(): void {
  if (gestureTimer !== undefined) window.clearTimeout(gestureTimer);
  gestureTimer = undefined;
}

sendMode.addEventListener("change", () => {
  const mode = sendMode.value;
  if (mode !== "button" && mode !== "double" && mode !== "single") {
    sendMode.value = "button";
    sendGesture.setMode("button");
  } else {
    sendGesture.setMode(mode as OverlaySendMode);
  }
  clearGestureTimer();
  status.textContent = mode === "single" ? "Single Enter is experimental. Shift+Enter always adds a line." : mode === "double" ? "Press Enter twice to send. Shift+Enter adds a line." : "Use Send privately. Enter adds a line.";
});

function keyboardGesture(event: KeyboardEvent) {
  return {
    key: event.key,
    shiftKey: event.shiftKey,
    repeat: event.repeat,
    isTrusted: event.isTrusted,
    isComposing: event.isComposing,
    now: performance.now(),
  };
}

draft.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    sendGesture.cancel();
    clearGestureTimer();
    status.textContent = "Send canceled. Your draft is still here.";
    return;
  }
  const mode = sendMode.value as OverlaySendMode;
  const plainTrustedEnter = event.key === "Enter" && !event.shiftKey && event.isTrusted && !event.isComposing;
  const result = sendGesture.keydown(keyboardGesture(event));
  if (plainTrustedEnter && mode !== "button") event.preventDefault();
  if (result === "send") {
    sendGesture.cancel();
    clearGestureTimer();
    void sendDraft();
  }
});

draft.addEventListener("keyup", (event) => {
  const result = sendGesture.keyup(keyboardGesture(event));
  if (result === "send") {
    clearGestureTimer();
    void sendDraft();
  } else if (result === "armed") {
    clearGestureTimer();
    status.textContent = "Press Enter again to send. Escape cancels.";
    gestureTimer = window.setTimeout(() => {
      gestureTimer = undefined;
      if (sendGesture.expire(performance.now())) status.textContent = "Double Enter expired. Your draft is still here.";
    }, 1_200);
  }
});

function expiryLabel(seconds: NativeOverlayTtlSeconds): string {
  if (seconds === 3_600) return "1 hour";
  if (seconds === 86_400) return "1 day";
  if (seconds === 259_200) return "3 days";
  return "7 days";
}

async function saveSecurity(): Promise<void> {
  if (!overlayReady || securityBusy) return;
  const requestedTtl = Number(ttl.value);
  if (!NATIVE_OVERLAY_TTL_OPTIONS.includes(requestedTtl as NativeOverlayTtlSeconds)) {
    ttl.value = String(confirmedTtlSeconds);
    return;
  }
  const previousTtl = confirmedTtlSeconds;
  const previousDecrypt = decryptDisplayEnabled;
  const requestedDecrypt = decryptDisplay.checked;
  if (!requestedDecrypt) {
    // Hiding is immediate and conservative; a failed save restores the exact
    // prior visibility below. Do not allow a receive poll to race the toggle.
    decryptDisplayEnabled = false;
    applyDecryptDisplayVisibility(false);
    if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
    receiveTimer = undefined;
  }
  securityBusy = true;
  refreshControls();
  status.textContent = "Saving protection…";
  const saved = await setNativeDiscordOverlaySecurity(requestedTtl as NativeOverlayTtlSeconds, decryptDisplay.checked);
  securityBusy = false;
  if (!saved) {
    ttl.value = String(previousTtl);
    decryptDisplay.checked = previousDecrypt;
    decryptDisplayEnabled = previousDecrypt;
    applyDecryptDisplayVisibility(previousDecrypt);
    if (previousDecrypt) scheduleReceivePoll(0);
    status.textContent = "That change was not saved. The previous protection stays active.";
    refreshControls();
    return;
  }
  confirmedTtlSeconds = saved.ttlSeconds;
  decryptDisplayEnabled = saved.decryptDisplayEnabled;
  viewOnceEnabled = saved.viewOnceEnabled;
  attachmentsEnabled = saved.attachmentsEnabled;
  discordMarkerAvailable = saved.discordMarkerAvailable;
  ttl.value = String(saved.ttlSeconds);
  decryptDisplay.checked = saved.decryptDisplayEnabled;
  currentExpiry.textContent = `Current: ${expiryLabel(saved.ttlSeconds)}`;
  // Existing non-view-once plaintext stays in this bounded DOM lifetime and
  // is revealed synchronously before polling resumes. No message is reopened.
  applyDecryptDisplayVisibility(decryptDisplayEnabled);
  refreshControls();
  status.textContent = "Protection updated.";
  if (decryptDisplayEnabled) scheduleReceivePoll(0);
  else {
    if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
    receiveTimer = undefined;
  }
}

ttl.addEventListener("change", () => void saveSecurity());
decryptDisplay.addEventListener("change", () => void saveSecurity());

async function initializeOverlay(): Promise<void> {
  try {
    const state = await getNativeDiscordOverlayState();
    if (!state) {
      status.textContent = "Verifying protected Discord…";
      window.setTimeout(() => void initializeOverlay(), overlayInitRetryMs);
      overlayInitRetryMs = Math.min(overlayInitRetryMs * 2, 1_000);
      return;
    }
    overlayInitRetryMs = 250;
    friendLabel.textContent = state.friendLabel;
    verifiedFriendIdentity = {
      id: "verified-friend",
      displayName: state.friendLabel,
      avatarFallback: Array.from(state.friendLabel.trim())[0]?.toUpperCase() || "?",
      provenance: "verified-osl",
    };
    ttl.value = String(state.ttlSeconds);
    confirmedTtlSeconds = state.ttlSeconds;
    currentExpiry.textContent = `Current: ${expiryLabel(state.ttlSeconds)}`;
    overlayReady = true;
    decryptDisplayEnabled = state.decryptDisplayEnabled;
    decryptDisplay.checked = state.decryptDisplayEnabled;
    applyDecryptDisplayVisibility(decryptDisplayEnabled);
    viewOnceEnabled = state.viewOnceEnabled;
    attachmentsEnabled = state.attachmentsEnabled;
    discordMarkerAvailable = state.discordMarkerAvailable;
    coverTextEnabled = state.covertextEnabled;
    coverText.setAttribute("aria-pressed", String(coverTextEnabled));
    setBusy(false);
    status.textContent = !state.discordMarkerAvailable
      ? "Ready for OSL-only messages. Discord marker placement is unavailable."
      : state.decryptDisplayEnabled
        ? "Ready."
        : "Receiving private text is off for this friend.";
    if (decryptDisplayEnabled) scheduleReceivePoll(0);
  } catch {
    status.textContent = "Verifying protected Discord…";
    window.setTimeout(() => void initializeOverlay(), overlayInitRetryMs);
    overlayInitRetryMs = Math.min(overlayInitRetryMs * 2, 1_000);
  }
}

void initializeOverlay();
