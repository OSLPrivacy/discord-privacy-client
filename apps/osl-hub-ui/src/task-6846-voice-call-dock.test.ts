import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import { oslPrimaryDestinationValues, oslSettingsDestination } from "./state";
import {
  activeVoiceCallDockMarkup,
  dispatchVoiceCallControl,
  formatVoiceCallElapsed,
  installVoiceCallAuthority,
  voiceCallDockMarkup,
  type VoiceCallAuthority,
  type VoiceCallSnapshot,
} from "./voice-call-dock";

class AuthoritativeEncryptedCall implements VoiceCallAuthority {
  current: VoiceCallSnapshot | null = {
    sessionId: "task6846-live-call",
    revision: 1,
    roomLabel: "Engineering voice",
    connection: "connected",
    elapsedMs: 125_000,
    participants: ["A", "B", "C"].map((name) => ({
      accountId: `task6846-account-${name}`,
      displayName: `Person ${name}`,
      verification: "verified" as const,
      speaking: name === "B",
    })),
    muted: false,
    deafened: false,
    expanded: false,
    inputDevices: [
      { id: "mic-1", label: "Built-in microphone", available: true },
      { id: "mic-2", label: "USB microphone", available: true },
    ],
    outputDevices: [
      { id: "speaker-1", label: "Built-in speaker", available: true },
      { id: "speaker-2", label: "USB speaker", available: true },
    ],
    selectedInputDeviceId: "mic-1",
    selectedOutputDeviceId: "speaker-1",
  };
  listeners = new Set<() => void>();
  calls: string[] = [];
  captureActive = true;
  keysLive = true;

  snapshot(): VoiceCallSnapshot | null { return this.current; }
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  private update(expected: string, patch: Partial<VoiceCallSnapshot>, call: string): void {
    if (!this.current || expected !== this.current.sessionId) throw new Error("panel detached from encrypted call");
    this.calls.push(call);
    this.current = { ...this.current, ...patch, revision: this.current.revision + 1 };
    for (const listener of this.listeners) listener();
  }
  async toggleMute(expected: string): Promise<void> {
    this.update(expected, { muted: !this.current!.muted }, "mute");
    this.captureActive = !this.current!.muted;
  }
  async toggleDeafen(expected: string): Promise<void> {
    this.update(expected, { deafened: !this.current!.deafened }, "deafen");
  }
  async chooseInputDevice(expected: string, id: string): Promise<void> {
    this.update(expected, { selectedInputDeviceId: id }, `input:${id}`);
    this.captureActive = true;
  }
  async chooseOutputDevice(expected: string, id: string): Promise<void> {
    this.update(expected, { selectedOutputDeviceId: id }, `output:${id}`);
  }
  async setExpanded(expected: string, expanded: boolean): Promise<void> {
    this.update(expected, { expanded }, expanded ? "expand" : "collapse");
  }
  async leave(expected: string): Promise<void> {
    this.update(expected, {}, "leave");
    this.captureActive = false;
    this.keysLive = false;
    this.current = null;
    for (const listener of this.listeners) listener();
  }

  loseSelectedDevice(): void {
    this.current = {
      ...this.current!,
      revision: this.current!.revision + 1,
      connection: "device-lost",
      inputDevices: this.current!.inputDevices.map((device) => device.id === "mic-2" ? { ...device, available: false } : device),
      selectedInputDeviceId: null,
    };
    this.captureActive = false;
  }

  reconnect(): void {
    this.current = { ...this.current!, revision: this.current!.revision + 1, connection: "connected" };
  }
}

describe("TASK 6846 persistent voice-call dock", () => {
  it("projects one authoritative encrypted session across every primary surface and remount", () => {
    const call = new AuthoritativeEncryptedCall();
    const repaint = vi.fn();
    installVoiceCallAuthority(call, repaint);
    const routes = [...oslPrimaryDestinationValues, oslSettingsDestination];
    const panels = routes.map(() => activeVoiceCallDockMarkup());
    expect(routes).toEqual(["home", "inbox", "people", "privacy", "activity", "connections", "settings"]);
    expect(panels).toHaveLength(7);
    for (const panel of panels) {
      expect(panel).toContain('data-session-id="task6846-live-call"');
      expect(panel).toContain("3 verified participants");
      expect(panel).toContain("Connected · encrypted");
      expect(panel).toContain("2:05");
    }
    expect(call.calls).toEqual([]);
    installVoiceCallAuthority(call, repaint);
    expect(call.listeners.size).toBe(1);
    expect(call.calls).toEqual([]);
    installVoiceCallAuthority(null, repaint);
  });

  it("sends every dock control to the session authority and follows device loss, reconnect and leave", async () => {
    const call = new AuthoritativeEncryptedCall();
    const id = call.current!.sessionId;
    await dispatchVoiceCallControl(call, id, "mute");
    expect(call.current!.muted).toBe(true);
    expect(call.captureActive).toBe(false);
    await dispatchVoiceCallControl(call, id, "mute");
    await dispatchVoiceCallControl(call, id, "deafen");
    await dispatchVoiceCallControl(call, id, "input", "mic-2");
    await dispatchVoiceCallControl(call, id, "output", "speaker-2");
    await dispatchVoiceCallControl(call, id, "expand");
    expect(voiceCallDockMarkup(call)).toContain('data-verified-participant="task6846-account-C"');
    expect(voiceCallDockMarkup(call)).toContain('<select data-voice-input');

    call.loseSelectedDevice();
    const lost = voiceCallDockMarkup(call);
    expect(lost).toContain('data-connection="device-lost"');
    expect(lost).toContain("Audio device lost");
    expect(lost).not.toContain('value="mic-2" selected');
    await dispatchVoiceCallControl(call, id, "input", "mic-1");
    call.reconnect();
    expect(voiceCallDockMarkup(call)).toContain("Connected · encrypted");

    await dispatchVoiceCallControl(call, id, "leave");
    expect(call.captureActive).toBe(false);
    expect(call.keysLive).toBe(false);
    expect(voiceCallDockMarkup(call)).toBe("");
    expect(call.calls).toEqual(["mute", "mute", "deafen", "input:mic-2", "output:speaker-2", "expand", "input:mic-1", "leave"]);
  });

  it("fails red for a stale panel, unverified roster, unavailable selection and detached shell", async () => {
    const call = new AuthoritativeEncryptedCall();
    await expect(dispatchVoiceCallControl(call, "detached-copy", "mute")).rejects.toThrow(/detached/);
    call.current = { ...call.current!, participants: [{ ...call.current!.participants[0], verification: "unverified" as "verified" }] };
    expect(() => voiceCallDockMarkup(call)).toThrow(/unverified participant/);
    call.current = { ...new AuthoritativeEncryptedCall().current!, selectedInputDeviceId: "missing" };
    expect(() => voiceCallDockMarkup(call)).toThrow(/unavailable input device/);

    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    expect(main).toContain("${activeVoiceCallDockMarkup()}");
    expect(main).toContain("bindActiveVoiceCallDock(document");
    const routeHandler = main.slice(main.indexOf('document.querySelectorAll<HTMLButtonElement>("[data-route]")'), main.indexOf("// A `[data-service]` click binding"));
    expect(routeHandler).not.toMatch(/installVoiceCallAuthority|toggleMute|toggleDeafen|reconnect\(|\.leave\(/);
  });

  it("formats the call-owned elapsed value without starting a second clock", () => {
    expect(formatVoiceCallElapsed(0)).toBe("0:00");
    expect(formatVoiceCallElapsed(125_999)).toBe("2:05");
    expect(formatVoiceCallElapsed(3_725_000)).toBe("1:02:05");
  });
});
