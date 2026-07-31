import "@fontsource-variable/inter/wght.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import oslLogoUrl from "./assets/logo-mark.svg";
import { getSignalProtectedSendReadiness, signalQaNativeDependencies } from "./signal-qa-ipc";
import {
  createSignalQaAttestationView,
  createSignalQaShell,
  type SignalQaSemanticReceipt,
  type SignalQaShellState,
} from "./signal-qa-shell";
import "./signal-qa.css";

function requireRoot(): HTMLElement {
  const element = document.querySelector<HTMLElement>("#app");
  if (!element) throw new Error("Signal QA shell root is missing");
  return element;
}

const root = requireRoot();
const shell = createSignalQaShell(signalQaNativeDependencies);
const attestation = createSignalQaAttestationView({ getSignalProtectedSendReadiness, nowMs: () => Date.now() });
let shellState: SignalQaShellState = shell.state();
let binding: SignalQaSemanticReceipt | null = null;
let busy = true;

function escapeHtml(value: string): string {
  return value.replace(/[&<>'"]/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;",
  })[character] ?? character);
}

function statusLabel(status: string): string {
  if (status === "open") return "Claimed";
  return status.replace(/([A-Z])/g, " $1").replace(/^./, (value) => value.toUpperCase());
}

function titlebar(): string {
  return `<header class="titlebar" data-tauri-drag-region><div class="brand" data-tauri-drag-region><img src="${oslLogoUrl}" alt=""><span>OSL · Signal QA</span></div><div class="window-controls"><button type="button" data-window="minimize" aria-label="Minimize">−</button><button type="button" data-window="close" aria-label="Close">×</button></div></header>`;
}

function render(): void {
  const claimed = shellState.phase === "open";
  const failure = shellState.failure
    ? `Blocked: ${escapeHtml(statusLabel(shellState.nativeReason ?? shellState.failure))}`
    : claimed ? "Exact installed Signal Desktop window claimed." : "Locating the existing Signal Desktop window…";
  const checks = [
    ["Exact Signal window", binding?.windowClaim ?? (claimed ? "passed" : "notRun")],
    ["Destination binding", binding?.destination ?? "notRun"],
    ["Composer binding", binding?.composer ?? "notRun"],
    ["Fresh attestation", binding?.freshness ?? "notRun"],
  ];
  root.innerHTML = `<div class="signal-app">${titlebar()}<main id="workspace-render-surface" class="workspace minimal-workspace"><header class="workspace-heading"><div><p class="eyebrow">Desktop-only test lane</p><h1>Signal</h1><p class="muted">Official installed Signal Desktop. No setup flow, password screen, browser route, credential access, or profile replacement.</p></div><span class="state ${claimed ? "ok" : ""}">${statusLabel(shellState.phase)}</span></header><section class="panel"><div class="panel-title"><div><h2>Native window</h2><p>${failure}</p></div><button id="signal-retry" type="button" ${busy || claimed ? "disabled" : ""}>Retry claim</button></div></section><section class="panel"><div class="panel-title"><div><h2>Protected-send binding</h2><p>A claimed window does not authorize sending. Exact destination and composer evidence must be fresh.</p></div><button id="signal-refresh" type="button" ${busy || !claimed ? "disabled" : ""}>Verify now</button></div><div class="checks">${checks.map(([label, status]) => `<div><span>${label}</span><strong class="check-${status}">${statusLabel(status)}</strong></div>`).join("")}</div><p class="receipt">Lifecycle ${binding?.lifecycleGeneration ?? 0} · Attestation ${binding?.attestationGeneration ?? 0} · ${binding?.validForMs ?? 0} ms remaining</p><button class="primary full" id="protected-composer" type="button" disabled aria-disabled="true">Protected messaging blocked until the native Signal adapter proves the exact destination and composer</button></section></main></div>`;
  bindControls();
}

function bindControls(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-window]").forEach((button) => button.addEventListener("click", async () => {
    const appWindow = getCurrentWindow();
    if (button.dataset.window === "minimize") await appWindow.minimize();
    else await appWindow.close();
  }));
  document.querySelector("#signal-retry")?.addEventListener("click", () => { void claimSignal(); });
  document.querySelector("#signal-refresh")?.addEventListener("click", () => { void refreshBinding(); });
}

async function claimSignal(): Promise<void> {
  if (busy) return;
  busy = true;
  render();
  try {
    shellState = await shell.open();
    if (shellState.phase === "open") binding = await attestation.refresh(true);
  } finally {
    busy = false;
    render();
  }
}

async function refreshBinding(): Promise<void> {
  if (busy || shellState.phase !== "open") return;
  busy = true;
  render();
  try { binding = await attestation.refresh(true); } finally { busy = false; render(); }
}

render();
busy = false;
void claimSignal();
