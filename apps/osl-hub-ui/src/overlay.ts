import "@fontsource-variable/inter/wght.css";
import "./overlay.css";
import { burnNativeDiscordOverlayChat, getNativeDiscordOverlayState, listNativeDiscordOverlayAttachments, openNativeDiscordOverlayAttachment, openNativeDiscordOverlayText, prepareNativeDiscordOverlayText, revealNativeDiscordOverlayViewOnce, selectNativeDiscordOverlayAttachment, sendNativeDiscordOverlayCarrier, setNativeDiscordOverlaySecurity, type NativeDiscordCarrierMode } from "./native-overlay-adapter";
import { boundedProtectedDraft, MAX_PROTECTED_DRAFT_BYTES, NATIVE_OVERLAY_TTL_OPTIONS, overlayExpiryDelayMs, PROTECTED_DRAFT_WARNING_BYTES, type NativeOverlayTtlSeconds, utf8Length } from "./overlay-state";
import { OverlaySendGesture, type OverlaySendMode } from "./overlay-send-gesture";
import { CoarseTypingRate } from "./coarse-typing-rate";
import { TwoStepBurnConfirmation } from "./two-step-burn";

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
const burnChat = requireElement<HTMLButtonElement>("#burn-protected-chat");
const status = requireElement<HTMLOutputElement>("#overlay-status");
const messageList = requireElement<HTMLOListElement>("#osl-message-list");

let composing = false;
let overlayReady = false;
let overlayInitRetryMs = 250;
let decryptDisplayEnabled = false;
let viewOnceEnabled = false;
let attachmentsEnabled = false;
let discordMarkerAvailable = false;
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
const outgoingBubbles = new Map<string, HTMLLIElement>();
const pendingAttachmentIds = new Set<string>();
const pendingViewOnceIds = new Set<string>();

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
  for (const [messageId, bubble] of outgoingBubbles) {
    if (bubble === item) outgoingBubbles.delete(messageId);
  }
  const attachmentId = item.dataset.attachmentId;
  if (attachmentId) pendingAttachmentIds.delete(attachmentId);
  const viewOnceId = item.dataset.viewOnceId;
  if (viewOnceId) pendingViewOnceIds.delete(viewOnceId);
  item.textContent = "";
  item.remove();
}

function removeViewOnceBubbles(): void {
  for (const item of [...viewOnceBubbles]) removeBubble(item);
}

function clearMessageBubbles(): void {
  for (const item of [...messageExpiryTimers.keys()]) removeBubble(item);
  messageList.replaceChildren();
}

function appendBubble(direction: "outgoing" | "incoming", plaintext: string, receipt: string, expiresAt: number, viewOnceMessage: boolean): HTMLLIElement {
  const item = document.createElement("li");
  item.className = direction;
  const body = document.createElement("p");
  body.textContent = plaintext;
  const state = document.createElement("span");
  state.textContent = receipt;
  item.append(body, state);
  messageList.append(item);
  if (viewOnceMessage) viewOnceBubbles.add(item);
  const expiryTimer = window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(expiresAt, Date.now()));
  messageExpiryTimers.set(item, expiryTimer);
  while (messageList.childElementCount > 24) {
    const oldest = messageList.firstElementChild;
    if (oldest instanceof HTMLLIElement) removeBubble(oldest);
    else oldest?.remove();
  }
  item.scrollIntoView({ block: "nearest" });
  return item;
}

function applyAcknowledgment(messageId: string, receipt: "received" | "opened"): void {
  const item = outgoingBubbles.get(messageId);
  const state = item?.querySelector("span");
  if (!item || !state) return;
  state.textContent = receipt === "opened" ? "Opened in OSL" : "Received by OSL";
  if (receipt === "opened") outgoingBubbles.delete(messageId);
}

function appendPendingAttachment(attachment: { attachmentId: string; originalFilename: string; plaintextSize: number; expiresAt: number; viewOnce: boolean }): void {
  if (pendingAttachmentIds.has(attachment.attachmentId)) return;
  pendingAttachmentIds.add(attachment.attachmentId);
  const item = document.createElement("li");
  item.className = "incoming attachment";
  item.dataset.attachmentId = attachment.attachmentId;
  const body = document.createElement("p");
  body.textContent = attachment.originalFilename;
  const receipt = document.createElement("span");
  const sizeLabel = attachment.plaintextSize >= 1024 * 1024
    ? `${(attachment.plaintextSize / (1024 * 1024)).toFixed(1)} MB`
    : `${Math.ceil(attachment.plaintextSize / 1024)} KB`;
  receipt.textContent = `${sizeLabel}${attachment.viewOnce ? " · view once" : ""}`;
  const open = document.createElement("button");
  open.type = "button";
  open.textContent = "Open privately";
  open.addEventListener("click", () => void (async () => {
    if (attachmentBusy) return;
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
  item.append(body, receipt, open);
  messageList.append(item);
  const expiryTimer = window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(attachment.expiresAt, Date.now()));
  messageExpiryTimers.set(item, expiryTimer);
}

function appendPendingViewOnce(message: { messageId: string; expiresAt: number }): void {
  if (pendingViewOnceIds.has(message.messageId)) return;
  pendingViewOnceIds.add(message.messageId);
  const item = document.createElement("li");
  item.className = "incoming view-once-pending";
  item.dataset.viewOnceId = message.messageId;
  const body = document.createElement("p");
  body.textContent = "View-once message";
  const receipt = document.createElement("span");
  receipt.textContent = "Received by OSL · unopened";
  const reveal = document.createElement("button");
  reveal.type = "button";
  reveal.textContent = "Reveal once";
  reveal.addEventListener("click", () => void (async () => {
    if (receiveBusy) return;
    receiveBusy = true;
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
  item.append(body, receipt, reveal);
  messageList.append(item);
  const expiryTimer = window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(message.expiresAt, Date.now()));
  messageExpiryTimers.set(item, expiryTimer);
}

function scheduleReceivePoll(delayMs: number): void {
  if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
  receiveTimer = undefined;
  if (!overlayReady || document.hidden) return;
  receiveTimer = window.setTimeout(() => void pollReceived(), delayMs);
}

async function pollReceived(): Promise<void> {
  if (receiveBusy || !overlayReady || document.hidden) return;
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

async function sendDraft(): Promise<void> {
  if (sendBusy || !overlayReady) return;
  const plaintext = boundedProtectedDraft(draft.value);
  if (!plaintext) { status.textContent = "Write a message first."; return; }
  if (utf8Length(plaintext) > MAX_PROTECTED_DRAFT_BYTES) { status.textContent = "This private message is too large."; return; }
  setBusy(true);
  status.textContent = "Encrypting…";
  const requestedViewOnce = viewOnce.checked;
  try {
    const result = await prepareNativeDiscordOverlayText(plaintext, requestedViewOnce);
    if (!result || result.viewOnce !== requestedViewOnce) throw new Error("invalid protected response");
    let markerSent = false;
    if (discordMarkerAvailable) {
      const requestedPlacement: NativeDiscordCarrierMode = placementMode.value === "compatibility" ? "compatibility" : "atomic";
      const charsPerSecond = typingRate.charsPerSecond();
      const carrier = await sendNativeDiscordOverlayCarrier(requestedPlacement, charsPerSecond);
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
  burnChat.textContent = "Burn OSL chat";
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
        burnChat.textContent = "Burn OSL chat";
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
  securityBusy = true;
  refreshControls();
  status.textContent = "Saving protection…";
  const saved = await setNativeDiscordOverlaySecurity(requestedTtl as NativeOverlayTtlSeconds, decryptDisplay.checked);
  securityBusy = false;
  if (!saved) {
    ttl.value = String(previousTtl);
    decryptDisplay.checked = previousDecrypt;
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
  refreshControls();
  status.textContent = "Protection updated.";
  if (decryptDisplayEnabled) scheduleReceivePoll(0);
  else {
    if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
    receiveTimer = undefined;
    clearMessageBubbles();
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
    ttl.value = String(state.ttlSeconds);
    confirmedTtlSeconds = state.ttlSeconds;
    currentExpiry.textContent = `Current: ${expiryLabel(state.ttlSeconds)}`;
    overlayReady = true;
    decryptDisplayEnabled = state.decryptDisplayEnabled;
    decryptDisplay.checked = state.decryptDisplayEnabled;
    viewOnceEnabled = state.viewOnceEnabled;
    attachmentsEnabled = state.attachmentsEnabled;
    discordMarkerAvailable = state.discordMarkerAvailable;
    setBusy(false);
    status.textContent = !state.discordMarkerAvailable
      ? "Ready for OSL-only messages. Discord marker placement is unavailable."
      : state.decryptDisplayEnabled
        ? "Ready."
        : "Receiving private text is off for this friend.";
    scheduleReceivePoll(0);
  } catch {
    status.textContent = "Verifying protected Discord…";
    window.setTimeout(() => void initializeOverlay(), overlayInitRetryMs);
    overlayInitRetryMs = Math.min(overlayInitRetryMs * 2, 1_000);
  }
}

void initializeOverlay();
