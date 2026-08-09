import "@fontsource-variable/inter/wght.css";
import { invoke } from "@tauri-apps/api/core";
import "./whatsapp-overlay.css";
import { parseWhatsAppPreparedCarrier } from "./whatsapp-overlay-prepare";
import { reconcileWhatsAppPrivateBox } from "./whatsapp-private-box";

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error("Trusted WhatsApp composer overlay is incomplete");
  return element;
}

const draft = requireElement<HTMLTextAreaElement>("#protected-draft");
const counter = requireElement<HTMLElement>("#draft-bytes");
const status = requireElement<HTMLElement>("#overlay-status");
const copy = requireElement<HTMLButtonElement>("#protected-send");

let composing = false;
let busy = false;

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

reconcileDraft();
