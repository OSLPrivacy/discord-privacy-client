import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { loadCoreIntegration } from "./core";
import { createWhatsAppQaShell, type WhatsAppQaReason, type WhatsAppQaState } from "./whatsapp-qa-shell";
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
      <span class="scope">OSL controls appear only after exact chat verification</span>
    </section>
    <p id="whatsapp-qa-ready" class="helper-ready" role="status">Verifying OSL and WhatsApp Desktop…</p>
  </main>`;

const qa = createWhatsAppQaShell();
const hostPanel = document.querySelector<HTMLElement>("#host-panel")!;
const hostStatus = document.querySelector<HTMLElement>("#host-status")!;
const hostBadge = document.querySelector<HTMLElement>("#host-badge")!;

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
  hostStatus.textContent = state.phase === "open"
    ? "Official WhatsApp Desktop claimed · protected composer waiting for exact chat binding"
    : state.phase === "opening" ? "Claiming exactly one official WhatsApp Desktop window…"
    : state.reason ? reasonText[state.reason] : "The exact native-window operation failed closed.";
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
  } catch {
    hostStatus.textContent = "Official WhatsApp Desktop claimed · protection verification failed closed";
  }
}

const currentWindow = getCurrentWindow();
document.querySelector("#window-minimize")?.addEventListener("click", () => void currentWindow.minimize());
document.querySelector("#window-maximize")?.addEventListener("click", () => void currentWindow.toggleMaximize());
document.querySelector("#window-close")?.addEventListener("click", () => void currentWindow.close());

void loadCoreIntegration().then(({ readiness }) => {
  if (readiness.unlocked && readiness.identityLoaded) void claim();
  else document.querySelector("#whatsapp-qa-ready")!.textContent = "OSL device identity unavailable. Failed closed.";
}).catch(() => { document.querySelector("#whatsapp-qa-ready")!.textContent = "OSL readiness unavailable. Failed closed."; });
