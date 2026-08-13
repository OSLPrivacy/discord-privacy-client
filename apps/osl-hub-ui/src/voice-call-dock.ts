/**
 * Persistent voice-call chrome.
 *
 * This module deliberately stores no call fields.  The authority passed by the
 * encrypted media runtime owns the roster, transport, clock, devices and local
 * controls; every paint takes a fresh snapshot and every action carries the
 * session id that was painted.  A stale or detached dock therefore cannot act
 * on a replacement call.
 */

export type VoiceConnectionState = "connecting" | "connected" | "reconnecting" | "device-lost";

export interface VerifiedVoiceParticipant {
  readonly accountId: string;
  readonly displayName: string;
  readonly verification: "verified";
  readonly speaking: boolean;
}

export interface VoiceDeviceChoice {
  readonly id: string;
  readonly label: string;
  readonly available: boolean;
}

export interface VoiceCallSnapshot {
  readonly sessionId: string;
  readonly revision: number;
  readonly roomLabel: string;
  readonly connection: VoiceConnectionState;
  /** Elapsed time supplied by the call clock, never reconstructed by the dock. */
  readonly elapsedMs: number;
  readonly participants: readonly VerifiedVoiceParticipant[];
  readonly muted: boolean;
  readonly deafened: boolean;
  readonly expanded: boolean;
  readonly inputDevices: readonly VoiceDeviceChoice[];
  readonly outputDevices: readonly VoiceDeviceChoice[];
  readonly selectedInputDeviceId: string | null;
  readonly selectedOutputDeviceId: string | null;
}

export interface VoiceCallAuthority {
  /** Null means that capture, transport and media keys have been torn down. */
  snapshot(): VoiceCallSnapshot | null;
  subscribe(listener: () => void): () => void;
  toggleMute(expectedSessionId: string): Promise<void>;
  toggleDeafen(expectedSessionId: string): Promise<void>;
  chooseInputDevice(expectedSessionId: string, deviceId: string): Promise<void>;
  chooseOutputDevice(expectedSessionId: string, deviceId: string): Promise<void>;
  setExpanded(expectedSessionId: string, expanded: boolean): Promise<void>;
  leave(expectedSessionId: string): Promise<void>;
}

const CONNECTION_COPY: Readonly<Record<VoiceConnectionState, string>> = {
  connecting: "Connecting",
  connected: "Connected · encrypted",
  reconnecting: "Reconnecting",
  "device-lost": "Audio device lost",
};

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

export function formatVoiceCallElapsed(elapsedMs: number): string {
  const seconds = Math.max(0, Math.floor(elapsedMs / 1_000));
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  const remainder = seconds % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}`
    : `${minutes}:${String(remainder).padStart(2, "0")}`;
}

function validateSnapshot(snapshot: VoiceCallSnapshot): void {
  if (!snapshot.sessionId || !snapshot.roomLabel || snapshot.revision < 0 || snapshot.elapsedMs < 0) {
    throw new Error("voice call authority returned an invalid session snapshot");
  }
  if (snapshot.participants.length === 0) throw new Error("voice call authority returned an empty verified roster");
  const participants = new Set<string>();
  for (const participant of snapshot.participants) {
    if (!participant.accountId || !participant.displayName || participant.verification !== "verified") {
      throw new Error("voice call authority returned an unverified participant");
    }
    if (participants.has(participant.accountId)) throw new Error("voice call authority returned a duplicate participant");
    participants.add(participant.accountId);
  }
  validateDeviceSelection(snapshot.inputDevices, snapshot.selectedInputDeviceId, "input");
  validateDeviceSelection(snapshot.outputDevices, snapshot.selectedOutputDeviceId, "output");
}

function validateDeviceSelection(devices: readonly VoiceDeviceChoice[], selected: string | null, kind: string): void {
  if (new Set(devices.map((device) => device.id)).size !== devices.length) {
    throw new Error(`voice call authority returned duplicate ${kind} devices`);
  }
  if (selected !== null && !devices.some((device) => device.id === selected && device.available)) {
    throw new Error(`voice call authority selected an unavailable ${kind} device`);
  }
}

function participantMarkup(participant: VerifiedVoiceParticipant): string {
  const initial = participant.displayName.trim().slice(0, 1).toLocaleUpperCase("en-US") || "?";
  return `<li class="voice-call-participant${participant.speaking ? " speaking" : ""}" data-verified-participant="${escapeHtml(participant.accountId)}"><span class="voice-call-avatar" aria-hidden="true">${escapeHtml(initial)}</span><span>${escapeHtml(participant.displayName)}</span><span class="voice-call-verified">Verified</span></li>`;
}

function deviceOptions(devices: readonly VoiceDeviceChoice[], selected: string | null): string {
  return devices.map((device) => `<option value="${escapeHtml(device.id)}"${device.id === selected ? " selected" : ""}${device.available ? "" : " disabled"}>${escapeHtml(device.label)}${device.available ? "" : " · unavailable"}</option>`).join("");
}

export function voiceCallDockMarkup(authority: VoiceCallAuthority): string {
  const snapshot = authority.snapshot();
  if (snapshot === null) return "";
  validateSnapshot(snapshot);
  const session = escapeHtml(snapshot.sessionId);
  const participantSummary = `${snapshot.participants.length} verified participant${snapshot.participants.length === 1 ? "" : "s"}`;
  const expanded = snapshot.expanded
    ? `<div class="voice-call-details" id="voice-call-details-${session}"><ul class="voice-call-participants" aria-label="Verified call participants">${snapshot.participants.map(participantMarkup).join("")}</ul><div class="voice-call-devices"><label>Microphone<select data-voice-input data-voice-session="${session}" aria-label="Voice microphone">${deviceOptions(snapshot.inputDevices, snapshot.selectedInputDeviceId)}</select></label><label>Speaker<select data-voice-output data-voice-session="${session}" aria-label="Voice speaker">${deviceOptions(snapshot.outputDevices, snapshot.selectedOutputDeviceId)}</select></label></div></div>`
    : "";
  return `<aside class="voice-call-dock${snapshot.expanded ? " expanded" : ""}" data-voice-call-dock data-session-id="${session}" data-session-revision="${snapshot.revision}" data-connection="${snapshot.connection}" aria-label="Active encrypted voice call"><div class="voice-call-summary"><span class="voice-call-live" aria-hidden="true"></span><span class="voice-call-title"><strong>${escapeHtml(snapshot.roomLabel)}</strong><small><span data-voice-connection>${CONNECTION_COPY[snapshot.connection]}</span> · <time data-voice-elapsed>${formatVoiceCallElapsed(snapshot.elapsedMs)}</time> · ${participantSummary}</small></span><div class="voice-call-controls" role="group" aria-label="Voice call controls"><button type="button" data-voice-mute data-voice-session="${session}" aria-pressed="${snapshot.muted}" aria-label="${snapshot.muted ? "Unmute microphone" : "Mute microphone"}">${snapshot.muted ? "Unmute" : "Mute"}</button><button type="button" data-voice-deafen data-voice-session="${session}" aria-pressed="${snapshot.deafened}" aria-label="${snapshot.deafened ? "Undeafen audio" : "Deafen audio"}">${snapshot.deafened ? "Undeafen" : "Deafen"}</button><button type="button" data-voice-expand data-voice-session="${session}" aria-expanded="${snapshot.expanded}" aria-controls="voice-call-details-${session}">${snapshot.expanded ? "Collapse" : "Expand"}</button><button class="danger" type="button" data-voice-leave data-voice-session="${session}">Leave</button></div></div>${expanded}</aside>`;
}

function expectedSession(element: HTMLElement): string {
  const sessionId = element.dataset.voiceSession;
  if (!sessionId) throw new Error("voice call control is detached from its session");
  return sessionId;
}

export type VoiceCallControlAction = "mute" | "deafen" | "expand" | "collapse" | "input" | "output" | "leave";

/** One command dispatcher shared by DOM bindings and non-DOM acceptance checks. */
export async function dispatchVoiceCallControl(
  authority: VoiceCallAuthority,
  expectedSessionId: string,
  action: VoiceCallControlAction,
  deviceId = "",
): Promise<void> {
  if (!expectedSessionId) throw new Error("voice call control is detached from its session");
  if (action === "mute") return authority.toggleMute(expectedSessionId);
  if (action === "deafen") return authority.toggleDeafen(expectedSessionId);
  if (action === "expand") return authority.setExpanded(expectedSessionId, true);
  if (action === "collapse") return authority.setExpanded(expectedSessionId, false);
  if (action === "input") {
    if (!deviceId) throw new Error("voice microphone choice is empty");
    return authority.chooseInputDevice(expectedSessionId, deviceId);
  }
  if (action === "output") {
    if (!deviceId) throw new Error("voice speaker choice is empty");
    return authority.chooseOutputDevice(expectedSessionId, deviceId);
  }
  return authority.leave(expectedSessionId);
}

/** Bind one paint. Call actions update the authority first; its event repaints the dock. */
export function bindVoiceCallDock(root: ParentNode, authority: VoiceCallAuthority, onFailure: (error: unknown) => void = () => undefined): void {
  const run = (action: () => Promise<void>): void => { void action().catch(onFailure); };
  root.querySelector<HTMLButtonElement>("[data-voice-mute]")?.addEventListener("click", (event) => {
    const control = event.currentTarget as HTMLButtonElement;
    run(() => dispatchVoiceCallControl(authority, expectedSession(control), "mute"));
  });
  root.querySelector<HTMLButtonElement>("[data-voice-deafen]")?.addEventListener("click", (event) => {
    const control = event.currentTarget as HTMLButtonElement;
    run(() => dispatchVoiceCallControl(authority, expectedSession(control), "deafen"));
  });
  root.querySelector<HTMLButtonElement>("[data-voice-expand]")?.addEventListener("click", (event) => {
    const control = event.currentTarget as HTMLButtonElement;
    run(() => dispatchVoiceCallControl(authority, expectedSession(control), control.getAttribute("aria-expanded") === "true" ? "collapse" : "expand"));
  });
  root.querySelector<HTMLButtonElement>("[data-voice-leave]")?.addEventListener("click", (event) => {
    const control = event.currentTarget as HTMLButtonElement;
    run(() => dispatchVoiceCallControl(authority, expectedSession(control), "leave"));
  });
  root.querySelector<HTMLSelectElement>("[data-voice-input]")?.addEventListener("change", (event) => {
    const control = event.currentTarget as HTMLSelectElement;
    run(() => dispatchVoiceCallControl(authority, expectedSession(control), "input", control.value));
  });
  root.querySelector<HTMLSelectElement>("[data-voice-output]")?.addEventListener("change", (event) => {
    const control = event.currentTarget as HTMLSelectElement;
    run(() => dispatchVoiceCallControl(authority, expectedSession(control), "output", control.value));
  });
}

let activeAuthority: VoiceCallAuthority | null = null;
let unsubscribe: (() => void) | null = null;

/**
 * Install the encrypted call runtime once. Re-rendering or navigating replaces
 * DOM only; it never replaces this authority or asks it to reconnect.
 */
export function installVoiceCallAuthority(authority: VoiceCallAuthority | null, repaint: () => void): void {
  if (activeAuthority === authority) return;
  unsubscribe?.();
  activeAuthority = authority;
  unsubscribe = authority?.subscribe(repaint) ?? null;
  repaint();
}

export function activeVoiceCallDockMarkup(): string {
  return activeAuthority ? voiceCallDockMarkup(activeAuthority) : "";
}

export function bindActiveVoiceCallDock(root: ParentNode, onFailure?: (error: unknown) => void): void {
  if (activeAuthority) bindVoiceCallDock(root, activeAuthority, onFailure);
}

export function activeVoiceCallSessionId(): string | null {
  return activeAuthority?.snapshot()?.sessionId ?? null;
}
