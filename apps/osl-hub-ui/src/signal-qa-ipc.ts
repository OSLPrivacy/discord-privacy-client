import { invoke } from "@tauri-apps/api/core";
import type { NativeApp } from "./services";
import type { SignalQaNativeReceipt, SignalQaShellDependencies } from "./signal-qa-shell";
import { isSignalQaTauriRuntime } from "./signal-qa-runtime";

const nativeReasons = new Set([
  "none", "platformUnsupported", "secondaryInstanceUnverified", "existingSessionNotFound",
  "existingSessionAmbiguous", "appNotInstalled", "launchFailed", "windowNotFound",
  "windowIdentityChanged", "ownerWindowUnavailable", "hostWindowUnavailable",
  "windowOperationRejected", "notHosted",
]);

function signalOnlyCatalog(raw: unknown): NativeApp[] {
  if (!Array.isArray(raw)) throw new Error("invalid native app catalog");
  const signal = raw.find((candidate) => candidate && typeof candidate === "object" && (candidate as { id?: unknown }).id === "signal") as Record<string, unknown> | undefined;
  if (!signal || signal.displayName !== "Signal" || !["installed", "installable", "unavailable"].includes(String(signal.availability))) return [];
  if (typeof signal.isolatedProfileAvailable !== "boolean" || typeof signal.supportsOverlay !== "boolean") throw new Error("invalid Signal catalog entry");
  return [{
    id: "signal", displayName: "Signal", availability: signal.availability as NativeApp["availability"],
    supportStatus: "comingSoon", protectedMode: "unavailable",
    isolatedProfileAvailable: signal.isolatedProfileAvailable, supportsOverlay: signal.supportsOverlay,
  }];
}

function signalReceipt(raw: unknown, expectedStatus: SignalQaNativeReceipt["status"] | "hosted"): SignalQaNativeReceipt {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw new Error("invalid Signal window receipt");
  const candidate = raw as Record<string, unknown>;
  if (candidate.id !== "signal" || candidate.status !== expectedStatus || candidate.reason !== "none"
    || candidate.mode !== "existingNativeCompanion" || candidate.captureProtected !== false
    || !nativeReasons.has(String(candidate.reason))) throw new Error("invalid Signal window receipt");
  return {
    id: "signal",
    status: expectedStatus === "hosted" ? "existingSession" : expectedStatus,
    reason: "none",
    mode: "existingNativeCompanion",
    captureProtected: false,
  };
}

export const signalQaNativeDependencies: SignalQaShellDependencies = {
  async listNativeApps() {
    if (!isSignalQaTauriRuntime()) return [];
    return signalOnlyCatalog(await invoke<unknown>("list_native_apps"));
  },
  async hostNativeAppWindow(appId, mode) {
    if (!isSignalQaTauriRuntime() || appId !== "signal" || mode !== "existingSession") throw new Error("Signal host unavailable");
    return signalReceipt(await invoke<unknown>("host_native_app_window", {
      appId,
      discordSessionMode: "existingSession",
      discordTakeover: "borrowExisting",
    }), "hosted");
  },
  async focusNativeAppWindow() {
    if (!isSignalQaTauriRuntime()) throw new Error("Signal focus unavailable");
    return signalReceipt(await invoke<unknown>("focus_native_app_window"), "focused");
  },
  async detachNativeAppWindow() {
    if (!isSignalQaTauriRuntime()) throw new Error("Signal detach unavailable");
    return signalReceipt(await invoke<unknown>("detach_native_app_window"), "detached");
  },
};

/** Read-only readiness query. Renderer input can never create an attestation. */
export async function getSignalProtectedSendReadiness(): Promise<unknown> {
  if (!isSignalQaTauriRuntime()) return null;
  try {
    return await invoke<unknown>("get_signal_protected_send_readiness");
  } catch {
    return null;
  }
}
