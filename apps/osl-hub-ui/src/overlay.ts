import "@fontsource-variable/inter/wght.css";
import { invoke } from "@tauri-apps/api/core";
import "./overlay.css";
import { boundedProtectedDraft, utf8Length } from "./overlay-state";
import { parseWhatsAppPreparedCarrier } from "./whatsapp-overlay-prepare";

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error("Trusted composer overlay is incomplete");
  return element;
}

const draft = requireElement<HTMLTextAreaElement>("#protected-draft");
const counter = requireElement<HTMLElement>("#draft-bytes");
const status = requireElement<HTMLElement>("#overlay-status");
const copy = requireElement<HTMLButtonElement>("#protected-send");

let composing = false;
let busy = false;

function reconcileDraft(): void {
  const bounded = boundedProtectedDraft(draft.value);
  if (bounded !== draft.value) draft.value = bounded;
  const bytes = utf8Length(bounded);
  counter.textContent = `${bytes} / 1000 bytes`;
  copy.disabled = busy || bytes === 0;
}

draft.addEventListener("compositionstart", () => { composing = true; });
draft.addEventListener("compositionend", () => { composing = false; reconcileDraft(); });
draft.addEventListener("input", () => { if (!composing) reconcileDraft(); });
copy.addEventListener("click", async () => {
  if (busy) return;
  const plaintext = boundedProtectedDraft(draft.value);
  if (!plaintext) return;
  busy = true;
  reconcileDraft();
  status.textContent = "Revalidating and encrypting…";
  try {
    const prepared = parseWhatsAppPreparedCarrier(
      await invoke("prepare_whatsapp_qa_protected_text", { plaintext }),
    );
    await navigator.clipboard.writeText(prepared.coverText);
    draft.value = "";
    status.textContent = "Protected carrier copied · paste and send in this chat";
  } catch {
    status.textContent = "Context changed or copy failed · nothing was sent";
  } finally {
    busy = false;
    reconcileDraft();
  }
});
