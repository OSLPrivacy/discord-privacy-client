import "@fontsource-variable/inter/wght.css";
import "./overlay.css";
import { boundedProtectedDraft, utf8Length } from "./overlay-state";

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error("Trusted composer overlay is incomplete");
  return element;
}

const draft = requireElement<HTMLTextAreaElement>("#protected-draft");
const counter = requireElement<HTMLElement>("#draft-bytes");

let composing = false;

function reconcileDraft(): void {
  const bounded = boundedProtectedDraft(draft.value);
  if (bounded !== draft.value) draft.value = bounded;
  counter.textContent = `${utf8Length(bounded)} / 1000 bytes`;
}

draft.addEventListener("compositionstart", () => { composing = true; });
draft.addEventListener("compositionend", () => { composing = false; reconcileDraft(); });
draft.addEventListener("input", () => { if (!composing) reconcileDraft(); });
