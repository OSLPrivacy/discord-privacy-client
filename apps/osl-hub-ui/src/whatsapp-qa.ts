import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { loadCoreIntegration } from "./core";
import { createWhatsAppQaShell, type WhatsAppQaReason, type WhatsAppQaState } from "./whatsapp-qa-shell";
import {
  isCompleteWhatsAppVisualBinding,
  parseWhatsAppVisualBindingBeginReceipt,
  parseWhatsAppVisualBindingConfirmReceipt,
  type WhatsAppVisualBindingBeginReceipt,
} from "./whatsapp-visual-binding";
import { parseWhatsAppProtectedOpenReceipt } from "./whatsapp-protected-open";
import "./whatsapp-qa.css";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("WhatsApp QA root is unavailable");

app.innerHTML = `
  <main class="qa-shell">
    <header class="titlebar" data-tauri-drag-region>
      <div class="brand"><span class="mark" aria-hidden="true">O</span><span>WhatsApp</span></div>
      <div class="window-actions">
        <button id="window-minimize" type="button" aria-label="Minimize">−</button>
        <button id="window-maximize" type="button" aria-label="Maximize or restore">□</button>
        <button id="window-close" type="button" aria-label="Close">×</button>
      </div>
    </header>
    <section id="host-panel" class="protection-strip" hidden aria-label="Protection status">
      <span id="host-badge" class="status-dot" aria-hidden="true"></span>
      <p id="host-status" role="status">Verifying the exact WhatsApp window…</p>
      <button id="bind-current-chat" class="bind-trigger" type="button" hidden>Bind current chat</button>
      <span class="scope">OSL controls appear only after exact chat verification</span>
    </section>
    <section id="visual-binding" class="binding-card" hidden aria-labelledby="binding-title">
      <div class="binding-heading">
        <div>
          <p class="eyebrow">One-time QA calibration</p>
          <h1 id="binding-title">Bind the visible peer chat</h1>
        </div>
        <button id="binding-close" class="icon-button" type="button" aria-label="Close visual binding">×</button>
      </div>
      <div id="binding-intro">
        <p>In WhatsApp, open the one-to-one conversation with this VM’s paired OSL QA peer. Keep the correct account and chat visible.</p>
        <ol class="binding-regions" aria-label="Fixed regions OSL checks">
          <li>Account header</li><li>Chat header</li><li>Composer</li><li>Transcript frame</li>
        </ol>
        <p class="privacy-note">OSL captures only these fixed regions to create local visual anchors. It does not retain screenshot content or read WhatsApp storage.</p>
        <button id="binding-begin" type="button">Check fixed regions</button>
      </div>
      <div id="binding-confirm-step" hidden>
        <p>The fixed regions were captured locally. Confirm only if the visible account and one-to-one chat are correct.</p>
        <label class="binding-attestation">
          <input id="binding-attestation" type="checkbox">
          <span>I confirm this WhatsApp account is the intended QA account and the visible chat is with the paired QA peer.</span>
        </label>
        <button id="binding-confirm" type="button" disabled>Confirm and bind</button>
      </div>
      <p id="binding-status" class="binding-status" role="status" aria-live="polite"></p>
    </section>
    <section id="protected-open" class="protected-open" hidden aria-labelledby="protected-open-title">
      <div>
        <p class="eyebrow">Trusted OSL display</p>
        <h2 id="protected-open-title">Open received protected text</h2>
        <p>Copy the carrier from the bound WhatsApp chat, then paste it here. OSL does not read WhatsApp or your clipboard automatically.</p>
      </div>
      <textarea id="protected-carrier" rows="3" spellcheck="false" autocomplete="off" placeholder="Paste protected carrier"></textarea>
      <div class="protected-open-actions">
        <button id="protected-open-button" type="button">Open locally</button>
        <span id="protected-open-status" role="status" aria-live="polite"></span>
      </div>
      <pre id="protected-plaintext" hidden aria-label="Decrypted protected text"></pre>
    </section>
    <p id="whatsapp-qa-ready" class="helper-ready" role="status">Verifying OSL and WhatsApp Desktop…</p>
  </main>`;

const qa = createWhatsAppQaShell();
const hostPanel = document.querySelector<HTMLElement>("#host-panel")!;
const hostStatus = document.querySelector<HTMLElement>("#host-status")!;
const hostBadge = document.querySelector<HTMLElement>("#host-badge")!;
const bindTrigger = document.querySelector<HTMLButtonElement>("#bind-current-chat")!;
const bindingCard = document.querySelector<HTMLElement>("#visual-binding")!;
const bindingIntro = document.querySelector<HTMLElement>("#binding-intro")!;
const bindingConfirmStep = document.querySelector<HTMLElement>("#binding-confirm-step")!;
const bindingBegin = document.querySelector<HTMLButtonElement>("#binding-begin")!;
const bindingConfirm = document.querySelector<HTMLButtonElement>("#binding-confirm")!;
const bindingAttestation = document.querySelector<HTMLInputElement>("#binding-attestation")!;
const bindingStatus = document.querySelector<HTMLElement>("#binding-status")!;
const protectedOpen = document.querySelector<HTMLElement>("#protected-open")!;
const protectedCarrier = document.querySelector<HTMLTextAreaElement>("#protected-carrier")!;
const protectedOpenButton = document.querySelector<HTMLButtonElement>("#protected-open-button")!;
const protectedOpenStatus = document.querySelector<HTMLElement>("#protected-open-status")!;
const protectedPlaintext = document.querySelector<HTMLElement>("#protected-plaintext")!;
let activeBinding: WhatsAppVisualBindingBeginReceipt | null = null;
let nativeWindowClaimed = false;

interface ProtectionReceipt {
  provider: "whatsapp";
  status: string;
  accountVerified: boolean;
  chatVerified: boolean;
  recipientSetVerified: boolean;
  composerVerified: boolean;
  transcriptVerified: boolean;
  protectedControlsAvailable: boolean;
}

const reasonText: Record<WhatsAppQaReason, string> = {
  none: "No failure.", platformUnsupported: "This QA build requires Windows.",
  appNotInstalled: "The official Microsoft Store WhatsApp app was not found.",
  existingSessionUnavailable: "No eligible running WhatsApp Desktop main window was found.",
  existingSessionAmbiguous: "More than one eligible WhatsApp window was found. OSL failed closed.",
  windowIdentityChanged: "The claimed WhatsApp window identity changed. OSL detached.",
  ownerWindowUnavailable: "The trusted OSL owner window is unavailable.",
  windowOperationRejected: "Windows rejected the bounded window operation.",
  notHosted: "No verified WhatsApp window is currently claimed.",
};

function renderHost(state: WhatsAppQaState): void {
  hostPanel.hidden = false;
  hostBadge.setAttribute("aria-label", state.phase === "open" ? "Native window claimed" : state.phase === "opening" ? "Checking" : "Failed closed");
  hostBadge.classList.toggle("claimed", state.phase === "open");
  hostBadge.classList.remove("ok");
  nativeWindowClaimed = state.phase === "open";
  bindTrigger.hidden = !nativeWindowClaimed;
  hostStatus.textContent = state.phase === "open"
    ? "Official WhatsApp Desktop claimed · protected composer waiting for exact chat binding"
    : state.phase === "opening" ? "Claiming exactly one official WhatsApp Desktop window…"
    : state.reason ? reasonText[state.reason] : "The exact native-window operation failed closed.";
}

function resetBinding(): void {
  activeBinding = null;
  bindingIntro.hidden = false;
  bindingConfirmStep.hidden = true;
  bindingAttestation.checked = false;
  bindingConfirm.disabled = true;
  bindingBegin.disabled = false;
  bindingStatus.textContent = "";
  protectedOpen.hidden = true;
  protectedCarrier.value = "";
  protectedPlaintext.textContent = "";
  protectedPlaintext.hidden = true;
  protectedOpenStatus.textContent = "";
}

function openBinding(): void {
  if (!nativeWindowClaimed) return;
  resetBinding();
  bindingCard.hidden = false;
  bindingBegin.focus();
}

async function beginVisualBinding(): Promise<void> {
  if (!nativeWindowClaimed || activeBinding) return;
  bindingBegin.disabled = true;
  bindingStatus.textContent = "Checking the four fixed regions…";
  try {
    activeBinding = parseWhatsAppVisualBindingBeginReceipt(
      await invoke("begin_whatsapp_visual_binding"),
    );
    bindingIntro.hidden = true;
    bindingConfirmStep.hidden = false;
    bindingStatus.textContent = "Fixed-region check complete. Review the attestation.";
    bindingAttestation.focus();
  } catch {
    activeBinding = null;
    bindingBegin.disabled = false;
    bindingStatus.textContent = "Visual binding could not be started. Protected controls remain locked.";
  }
}

async function confirmVisualBinding(): Promise<void> {
  if (!nativeWindowClaimed || !activeBinding || !bindingAttestation.checked) return;
  const captureId = activeBinding.captureId;
  bindingConfirm.disabled = true;
  bindingAttestation.disabled = true;
  bindingStatus.textContent = "Verifying account, peer chat, composer, and transcript…";
  try {
    const receipt = parseWhatsAppVisualBindingConfirmReceipt(
      await invoke("confirm_whatsapp_visual_binding", {
        captureId,
        attested: true,
      }),
    );
    if (!isCompleteWhatsAppVisualBinding(receipt, captureId)) {
      throw new Error("visual binding incomplete");
    }
    hostBadge.classList.add("ok");
    hostStatus.textContent = "Exact account, paired peer chat, composer, and transcript visually bound";
    bindingStatus.textContent = "Binding verified. Protected controls may now be requested for this exact context.";
    protectedOpen.hidden = false;
    bindingAttestation.disabled = true;
  } catch {
    activeBinding = null;
    bindingAttestation.disabled = false;
    bindingAttestation.checked = false;
    bindingConfirm.disabled = true;
    bindingIntro.hidden = false;
    bindingConfirmStep.hidden = true;
    bindingBegin.disabled = false;
    bindingStatus.textContent = "Binding was rejected or changed. Protected controls remain locked.";
    hostBadge.classList.remove("ok");
  }
}

async function openProtectedCarrier(): Promise<void> {
  const coverText = protectedCarrier.value.trim();
  if (!coverText || protectedOpenButton.disabled) return;
  protectedOpenButton.disabled = true;
  protectedPlaintext.textContent = "";
  protectedPlaintext.hidden = true;
  protectedOpenStatus.textContent = "Revalidating the bound chat and opening locally…";
  try {
    const receipt = parseWhatsAppProtectedOpenReceipt(
      await invoke("open_whatsapp_qa_protected_text", { coverText }),
    );
    protectedPlaintext.textContent = receipt.plaintext;
    protectedPlaintext.hidden = false;
    protectedCarrier.value = "";
    protectedOpenStatus.textContent = "Opened with peer E2EE · WhatsApp history unchanged";
  } catch {
    protectedOpenStatus.textContent = "Could not open. The carrier, peer, replay state, or visual binding was rejected.";
  } finally {
    protectedOpenButton.disabled = false;
  }
}

async function claim(): Promise<void> {
  renderHost({ ...qa.state(), phase: "opening" });
  const state = await qa.open();
  renderHost(state);
  if (state.phase !== "open") return;
  try {
    const receipt = await invoke<ProtectionReceipt>("get_whatsapp_qa_protection_status");
    const complete = receipt.provider === "whatsapp" && receipt.protectedControlsAvailable
      && receipt.accountVerified && receipt.chatVerified && receipt.recipientSetVerified
      && receipt.composerVerified && receipt.transcriptVerified;
    hostBadge.classList.toggle("ok", complete);
    hostStatus.textContent = complete
      ? "Exact account, chat, recipients, composer, and transcript verified · protected composer ready"
      : `Official WhatsApp Desktop claimed · protected composer locked (${receipt.status})`;
    protectedOpen.hidden = !complete;
  } catch {
    hostStatus.textContent = "Official WhatsApp Desktop claimed · protection verification failed closed";
  }
}

const currentWindow = getCurrentWindow();
document.querySelector("#window-minimize")?.addEventListener("click", () => void currentWindow.minimize());
document.querySelector("#window-maximize")?.addEventListener("click", () => void currentWindow.toggleMaximize());
document.querySelector("#window-close")?.addEventListener("click", () => void currentWindow.close());
bindTrigger.addEventListener("click", () => openBinding());
document.querySelector("#binding-close")?.addEventListener("click", () => {
  bindingCard.hidden = true;
  resetBinding();
  bindTrigger.focus();
});
bindingBegin.addEventListener("click", () => void beginVisualBinding());
bindingAttestation.addEventListener("change", () => {
  bindingConfirm.disabled = !bindingAttestation.checked || !activeBinding;
});
bindingConfirm.addEventListener("click", () => void confirmVisualBinding());
protectedOpenButton.addEventListener("click", () => void openProtectedCarrier());

void loadCoreIntegration().then(({ readiness }) => {
  if (readiness.unlocked && readiness.identityLoaded) void claim();
  else document.querySelector("#whatsapp-qa-ready")!.textContent = "OSL device identity unavailable. Failed closed.";
}).catch(() => { document.querySelector("#whatsapp-qa-ready")!.textContent = "OSL readiness unavailable. Failed closed."; });
