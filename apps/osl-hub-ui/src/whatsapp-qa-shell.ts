import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";

export type WhatsAppQaStatus = "hosted" | "resized" | "focused" | "detached" | "failed";
export type WhatsAppQaReason = "none" | "platformUnsupported" | "appNotInstalled" | "existingSessionUnavailable" | "existingSessionAmbiguous" | "windowIdentityChanged" | "ownerWindowUnavailable" | "windowOperationRejected" | "notHosted";

export interface WhatsAppQaReceipt {
  provider: "whatsapp";
  status: WhatsAppQaStatus;
  reason: WhatsAppQaReason;
  mode: "none" | "existingNativeCompanion";
  captureProtected: false;
}

export interface WhatsAppQaState {
  phase: "idle" | "opening" | "open" | "failed";
  reason: WhatsAppQaReason | null;
  browserFallbackAllowed: false;
  installAllowed: false;
  credentialsAccepted: false;
  sessionMode: "existingSession";
}

export interface WhatsAppQaDependencies {
  claim(): Promise<WhatsAppQaReceipt>;
  resize(): Promise<WhatsAppQaReceipt>;
  focus(): Promise<WhatsAppQaReceipt>;
  detach(): Promise<WhatsAppQaReceipt>;
}

const reasons: readonly WhatsAppQaReason[] = ["none", "platformUnsupported", "appNotInstalled", "existingSessionUnavailable", "existingSessionAmbiguous", "windowIdentityChanged", "ownerWindowUnavailable", "windowOperationRejected", "notHosted"];
const statuses: readonly WhatsAppQaStatus[] = ["hosted", "resized", "focused", "detached", "failed"];

function exactRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseWhatsAppQaReceipt(raw: unknown): WhatsAppQaReceipt {
  if (!exactRecord(raw) || Object.keys(raw).sort().join(",") !== "captureProtected,mode,provider,reason,status"
    || raw.provider !== "whatsapp" || !statuses.includes(raw.status as WhatsAppQaStatus)
    || !reasons.includes(raw.reason as WhatsAppQaReason) || raw.captureProtected !== false
    || !["none", "existingNativeCompanion"].includes(String(raw.mode))) throw new Error("invalid WhatsApp QA receipt");
  const success = raw.status !== "failed";
  if ((success && (raw.reason !== "none" || raw.mode !== "existingNativeCompanion"))
    || (!success && raw.mode !== "none")) throw new Error("invalid WhatsApp QA receipt");
  return raw as unknown as WhatsAppQaReceipt;
}

const invokeReceipt = async (command: string): Promise<WhatsAppQaReceipt> => {
  if (!isTauriRuntime()) throw new Error("WhatsApp Desktop QA is unavailable outside OSL");
  return parseWhatsAppQaReceipt(await invoke<unknown>(command));
};

const defaults: WhatsAppQaDependencies = {
  claim: () => invokeReceipt("claim_whatsapp_qa_window"),
  resize: () => invokeReceipt("resize_whatsapp_qa_window"),
  focus: () => invokeReceipt("focus_whatsapp_qa_window"),
  detach: () => invokeReceipt("detach_whatsapp_qa_window"),
};

const initial = (): WhatsAppQaState => ({ phase: "idle", reason: null, browserFallbackAllowed: false, installAllowed: false, credentialsAccepted: false, sessionMode: "existingSession" });

export function createWhatsAppQaShell(deps: WhatsAppQaDependencies = defaults) {
  let state = initial();
  let pending = false;
  const snapshot = (): WhatsAppQaState => ({ ...state });
  const accept = (receipt: WhatsAppQaReceipt, status: WhatsAppQaStatus): boolean => receipt.provider === "whatsapp" && receipt.status === status && receipt.reason === "none" && receipt.mode === "existingNativeCompanion" && receipt.captureProtected === false;
  const fail = (reason: WhatsAppQaReason | null): WhatsAppQaState => (state = { ...initial(), phase: "failed", reason }, snapshot());
  return {
    state: snapshot,
    async open(): Promise<WhatsAppQaState> {
      if (pending || state.phase === "open") return fail("windowOperationRejected");
      pending = true; state = { ...initial(), phase: "opening" };
      try { const receipt = await deps.claim(); return accept(receipt, "hosted") ? (state = { ...initial(), phase: "open" }, snapshot()) : fail(receipt.reason); }
      catch { return fail(null); } finally { pending = false; }
    },
    async resize(): Promise<WhatsAppQaState> { if (state.phase !== "open" || pending) return fail("notHosted"); try { const receipt = await deps.resize(); return accept(receipt, "resized") ? snapshot() : fail(receipt.reason); } catch { return fail(null); } },
    async focus(): Promise<WhatsAppQaState> { if (state.phase !== "open" || pending) return fail("notHosted"); try { const receipt = await deps.focus(); return accept(receipt, "focused") ? snapshot() : fail(receipt.reason); } catch { return fail(null); } },
    async close(): Promise<WhatsAppQaState> { if (state.phase !== "open" || pending) return fail("notHosted"); try { const receipt = await deps.detach(); return accept(receipt, "detached") ? (state = initial(), snapshot()) : fail(receipt.reason); } catch { return fail(null); } },
  };
}
