import "@fontsource-variable/inter/wght.css";
import { invoke } from "@tauri-apps/api/core";
import "./whatsapp-overlay.css";
import { parseWhatsAppPreparedCarrier } from "./whatsapp-overlay-prepare";
import { reconcileWhatsAppPrivateBox } from "./whatsapp-private-box";
import {
  addDroppedWhatsAppAttachmentFiles,
  addPickedWhatsAppAttachmentFiles,
  type WhatsAppAttachmentFile,
  type WhatsAppAttachmentTray,
} from "./whatsapp-attachment-tray";

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error("Trusted WhatsApp composer overlay is incomplete");
  return element;
}

const draft = requireElement<HTMLTextAreaElement>("#protected-draft");
const counter = requireElement<HTMLElement>("#draft-bytes");
const status = requireElement<HTMLElement>("#overlay-status");
const copy = requireElement<HTMLButtonElement>("#protected-send");
const attachmentPicker = requireElement<HTMLButtonElement>("#whatsapp-attachment-picker");
const attachmentInput = requireElement<HTMLInputElement>("#whatsapp-attachment-input");
const attachmentTray = requireElement<HTMLElement>("#whatsapp-attachment-tray");

let composing = false;
let busy = false;
let attachments: WhatsAppAttachmentTray = { cards: [], rejected: [] };

function renderAttachmentTray(): void {
  const cards = attachments.cards.map((card) => {
    const item = document.createElement("div");
    item.className = "wa-attachment-card";
    item.textContent = `${card.name} · ${card.sizeLabel} · Unsent`;
    return item;
  });
  attachmentTray.replaceChildren(...cards);
  if (attachments.rejected.length) status.textContent = attachments.rejected.at(-1) ?? "Attachment was not added.";
}

function stageAttachments(files: Iterable<WhatsAppAttachmentFile>, source: "picker" | "drop"): void {
  attachments = source === "picker"
    ? addPickedWhatsAppAttachmentFiles(attachments, files)
    : addDroppedWhatsAppAttachmentFiles(attachments, files);
  renderAttachmentTray();
}

function reconcileDraft(): void {
  const state = reconcileWhatsAppPrivateBox(draft.value);
  if (state.privateDraft !== draft.value) draft.value = state.privateDraft;
  counter.textContent = state.byteCountText;
  copy.disabled = busy || state.privateDraft.length === 0;
}

draft.addEventListener("compositionstart", () => { composing = true; });
draft.addEventListener("compositionend", () => { composing = false; reconcileDraft(); });
draft.addEventListener("input", () => { if (!composing) reconcileDraft(); });
copy.addEventListener("click", async () => {
  if (busy) return;
  const plaintext = reconcileWhatsAppPrivateBox(draft.value).privateDraft;
  if (!plaintext) return;
  busy = true;
  reconcileDraft();
  status.textContent = "Revalidating and encrypting.";
  try {
    const prepared = parseWhatsAppPreparedCarrier(
      await invoke("prepare_whatsapp_qa_protected_text", { plaintext }),
    );
    await navigator.clipboard.writeText(prepared.coverText);
    draft.value = "";
    status.textContent = "Protected carrier copied. Paste and send in this chat.";
  } catch {
    status.textContent = "Context changed or copy failed. Nothing was sent.";
  } finally {
    busy = false;
    reconcileDraft();
  }
});

attachmentPicker.addEventListener("click", () => attachmentInput.click());
attachmentInput.addEventListener("change", () => {
  stageAttachments(attachmentInput.files ?? [], "picker");
  attachmentInput.value = "";
});
for (const eventName of ["dragenter", "dragover"] as const) {
  attachmentTray.addEventListener(eventName, (event) => {
    event.preventDefault();
    attachmentTray.classList.add("is-dragging");
  });
}
for (const eventName of ["dragleave", "drop"] as const) {
  attachmentTray.addEventListener(eventName, (event) => {
    event.preventDefault();
    attachmentTray.classList.remove("is-dragging");
    if (eventName === "drop") stageAttachments(event.dataTransfer?.files ?? [], "drop");
  });
}

reconcileDraft();
renderAttachmentTray();
