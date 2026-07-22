import { getCurrentWindow } from "@tauri-apps/api/window";
import { loadCoreIntegration, unlockHubPasswordGate } from "./core";
import { createWhatsAppQaShell, type WhatsAppQaReason, type WhatsAppQaState } from "./whatsapp-qa-shell";
import "./whatsapp-qa.css";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("WhatsApp QA root is unavailable");

app.innerHTML = `
  <main class="qa-shell">
    <header class="titlebar" data-tauri-drag-region>
      <div class="brand"><span class="mark" aria-hidden="true">O</span><span>OSL WhatsApp QA</span></div>
      <div class="window-actions">
        <button id="window-minimize" type="button" aria-label="Minimize">−</button>
        <button id="window-maximize" type="button" aria-label="Maximize or restore">□</button>
        <button id="window-close" type="button" aria-label="Close">×</button>
      </div>
    </header>
    <section class="content">
      <div class="intro">
        <p class="eyebrow">DEDICATED NATIVE-APP TEST</p>
        <h1>WhatsApp Desktop protection</h1>
        <p>OSL will claim exactly one already-running official WhatsApp Desktop window. It will never open WhatsApp Web, install an app, or link an account.</p>
      </div>
      <p id="whatsapp-qa-ready" class="helper-ready" role="status">Ready for secret-safe unlock</p>
      <section id="unlock-panel" class="card" aria-labelledby="unlock-heading">
        <h2 id="unlock-heading">Unlock OSL</h2>
        <form id="identity-password-form" novalidate>
          <label for="identity-password">Main password</label>
          <input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="current-password" required aria-describedby="unlock-status" autofocus />
          <button id="identity-password-submit" type="submit" disabled>Unlock</button>
        </form>
        <p id="unlock-status" class="semantic-status" role="status">Waiting for production OSL unlock.</p>
      </section>
      <section id="host-panel" class="card" hidden aria-labelledby="host-heading">
        <div class="status-row"><div><p class="eyebrow">EXACT EXISTING SESSION</p><h2 id="host-heading">WhatsApp Desktop</h2></div><span id="host-badge" class="badge">Checking</span></div>
        <p id="host-status" class="semantic-status" role="status">Claiming the verified native window…</p>
        <div class="host-actions">
          <button id="host-focus" type="button">Focus claimed window</button>
          <button id="host-resize" type="button">Realign</button>
          <button id="host-detach" type="button" class="secondary">Detach safely</button>
        </div>
      </section>
      <section id="protected-panel" class="card" hidden aria-labelledby="protected-heading">
        <h2 id="protected-heading">Protected-message checks</h2>
        <p class="semantic-status">Disabled until OSL verifies the live account, conversation, recipients, composer, and transcript selectors.</p>
        <div class="protected-grid" aria-label="Unavailable protected-message controls">
          <button type="button" disabled>Protect text</button><button type="button" disabled>Decrypt</button>
          <button type="button" disabled>Burn</button><button type="button" disabled>Covertext</button>
          <button type="button" disabled>Image + caption</button><button type="button" disabled>File + caption</button>
        </div>
      </section>
    </section>
  </main>`;

const qa = createWhatsAppQaShell();
const password = document.querySelector<HTMLInputElement>("#identity-password")!;
const submit = document.querySelector<HTMLButtonElement>("#identity-password-submit")!;
const form = document.querySelector<HTMLFormElement>("#identity-password-form")!;
const unlockPanel = document.querySelector<HTMLElement>("#unlock-panel")!;
const unlockStatus = document.querySelector<HTMLElement>("#unlock-status")!;
const hostPanel = document.querySelector<HTMLElement>("#host-panel")!;
const protectedPanel = document.querySelector<HTMLElement>("#protected-panel")!;
const hostStatus = document.querySelector<HTMLElement>("#host-status")!;
const hostBadge = document.querySelector<HTMLElement>("#host-badge")!;

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
  protectedPanel.hidden = state.phase !== "open";
  hostBadge.textContent = state.phase === "open" ? "Native window claimed" : state.phase === "opening" ? "Checking" : "Failed closed";
  hostBadge.classList.toggle("ok", state.phase === "open");
  hostStatus.textContent = state.phase === "open"
    ? "Official installed WhatsApp Desktop is tethered to this trusted OSL window. Protected sending remains unavailable until live selectors verify."
    : state.phase === "opening" ? "Claiming exactly one verified existing WhatsApp Desktop window…"
    : state.reason ? reasonText[state.reason] : "The exact native-window operation failed closed.";
}

async function claim(): Promise<void> {
  renderHost({ ...qa.state(), phase: "opening" });
  renderHost(await qa.open());
}

password.addEventListener("input", () => { submit.disabled = password.value.length < 6 || password.value.length > 128; });
form.addEventListener("submit", async (event) => {
  event.preventDefault();
  submit.disabled = true; password.disabled = true; unlockStatus.textContent = "Verifying with production OSL…";
  try {
    const result = await unlockHubPasswordGate(password.value);
    password.value = "";
    if (result.outcome !== "unlocked") {
      unlockStatus.textContent = result.outcome === "wrong" ? "Unlock failed. Nothing changed." : "Main-role unlock required. Nothing was claimed.";
      return;
    }
    unlockPanel.hidden = true;
    await claim();
  } catch { unlockStatus.textContent = "Unlock unavailable. Nothing changed."; }
  finally { password.value = ""; password.disabled = false; submit.disabled = true; }
});

document.querySelector("#host-focus")?.addEventListener("click", async () => renderHost(await qa.focus()));
document.querySelector("#host-resize")?.addEventListener("click", async () => renderHost(await qa.resize()));
document.querySelector("#host-detach")?.addEventListener("click", async () => renderHost(await qa.close()));

const currentWindow = getCurrentWindow();
document.querySelector("#window-minimize")?.addEventListener("click", () => void currentWindow.minimize());
document.querySelector("#window-maximize")?.addEventListener("click", () => void currentWindow.toggleMaximize());
document.querySelector("#window-close")?.addEventListener("click", () => void currentWindow.close());

void loadCoreIntegration().then(({ readiness }) => {
  if (readiness.unlocked) { unlockPanel.hidden = true; void claim(); }
  else if (!readiness.identityLoaded || readiness.bootstrapStatus === "setupRequired") unlockStatus.textContent = "This QA build requires an existing configured OSL identity.";
}).catch(() => { unlockStatus.textContent = "Production OSL readiness is unavailable. Nothing changed."; });
