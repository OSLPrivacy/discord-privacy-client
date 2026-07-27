import { invoke } from "@tauri-apps/api/core";
import {
  parseNativeDiscordOverlayPrepared,
  type NativeDiscordOverlayPrepared,
} from "./overlay-state";

export interface DiscordHeadlessQaPoll {
  openedCount: number;
  pendingViewOnceCount: number;
  acknowledgmentCount: number;
  fetched: number;
}

export type NativeVisibleRowQaTriState = "accepted" | "refused" | "not_observed";

export interface NativeVisibleRowRuntimeReceipt {
  schemaVersion: 2;
  observedAtUnixMs: number;
  buildHash: string;
  oslTargetIdentitySha256: string;
  discordTargetIdentitySha256: string;
  scopeBindingSha256: string;
  windowGeneration: number;
  rowsObserved: number;
  nativeProofSome: number;
  nativeProofNone: number;
  authenticatedOwnOutgoing: number;
  authenticatedPeerIncoming: number;
  brokerPlaintextRows: number;
  brokerRefusedRows: number;
  outcomes: {
    ownOutgoing: NativeVisibleRowQaTriState;
    peerIncoming: NativeVisibleRowQaTriState;
    peerAnchor: NativeVisibleRowQaTriState;
    zeroRows: NativeVisibleRowQaTriState;
    missingProof: NativeVisibleRowQaTriState;
    mixedScope: NativeVisibleRowQaTriState;
    differentNonSelf: NativeVisibleRowQaTriState;
    replay: NativeVisibleRowQaTriState;
    reorder: NativeVisibleRowQaTriState;
    persistence: NativeVisibleRowQaTriState;
  };
  accepted: boolean;
}

const RECEIPT_KEYS = [
  "schemaVersion",
  "observedAtUnixMs",
  "buildHash",
  "oslTargetIdentitySha256",
  "discordTargetIdentitySha256",
  "scopeBindingSha256",
  "windowGeneration",
  "rowsObserved",
  "nativeProofSome",
  "nativeProofNone",
  "authenticatedOwnOutgoing",
  "authenticatedPeerIncoming",
  "brokerPlaintextRows",
  "brokerRefusedRows",
  "outcomes",
  "accepted",
] as const;

const OUTCOME_KEYS = [
  "ownOutgoing",
  "peerIncoming",
  "peerAnchor",
  "zeroRows",
  "missingProof",
  "mixedScope",
  "differentNonSelf",
  "replay",
  "reorder",
  "persistence",
] as const;

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === keys.length
    && [...keys].sort().every((key, index) => actual[index] === key);
}

function isTriState(value: unknown): value is NativeVisibleRowQaTriState {
  return value === "accepted" || value === "refused" || value === "not_observed";
}

export function parseNativeVisibleRowRuntimeReceipt(
  value: unknown,
): NativeVisibleRowRuntimeReceipt | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (!exactKeys(record, RECEIPT_KEYS)
    || record.schemaVersion !== 2
    || !Number.isSafeInteger(record.observedAtUnixMs)
    || Number(record.observedAtUnixMs) <= 0
    || typeof record.buildHash !== "string"
    || !/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/u.test(record.buildHash)
    || !["oslTargetIdentitySha256", "discordTargetIdentitySha256", "scopeBindingSha256"]
      .every((key) => typeof record[key] === "string" && /^[0-9a-f]{64}$/u.test(String(record[key])))
    || !["windowGeneration", "rowsObserved", "nativeProofSome", "nativeProofNone",
      "authenticatedOwnOutgoing", "authenticatedPeerIncoming", "brokerPlaintextRows",
      "brokerRefusedRows"]
      .every((key) => Number.isSafeInteger(record[key]) && Number(record[key]) >= 0)
    || typeof record.accepted !== "boolean"
    || typeof record.outcomes !== "object"
    || record.outcomes === null
    || Array.isArray(record.outcomes)) return null;
  const outcomes = record.outcomes as Record<string, unknown>;
  if (!exactKeys(outcomes, OUTCOME_KEYS)
    || !OUTCOME_KEYS.every((key) => isTriState(outcomes[key]))) return null;
  const rowsObserved = Number(record.rowsObserved);
  const nativeProofSome = Number(record.nativeProofSome);
  const nativeProofNone = Number(record.nativeProofNone);
  const authenticatedOwnOutgoing = Number(record.authenticatedOwnOutgoing);
  const authenticatedPeerIncoming = Number(record.authenticatedPeerIncoming);
  const brokerPlaintextRows = Number(record.brokerPlaintextRows);
  if (Number(record.windowGeneration) <= 0
    || nativeProofSome + nativeProofNone !== rowsObserved
    || authenticatedOwnOutgoing + authenticatedPeerIncoming !== brokerPlaintextRows
    || brokerPlaintextRows > nativeProofSome
    || (record.accepted && (
      outcomes.ownOutgoing !== "accepted"
      || outcomes.peerIncoming !== "accepted"
      || outcomes.peerAnchor !== "accepted"
      || outcomes.zeroRows !== "refused"
      || outcomes.missingProof !== "refused"
      || outcomes.mixedScope !== "refused"
      || outcomes.differentNonSelf !== "refused"
      || outcomes.replay !== "refused"
      || outcomes.reorder !== "refused"
      || outcomes.persistence !== "accepted"
    ))) return null;
  return record as unknown as NativeVisibleRowRuntimeReceipt;
}

function parseDiscordHeadlessQaPoll(value: unknown): DiscordHeadlessQaPoll | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const keys = ["openedCount", "pendingViewOnceCount", "acknowledgmentCount", "fetched"] as const;
  if (Object.keys(record).length !== keys.length
    || keys.some((key) => !Number.isSafeInteger(record[key]) || Number(record[key]) < 0)) return null;
  return {
    openedCount: Number(record.openedCount),
    pendingViewOnceCount: Number(record.pendingViewOnceCount),
    acknowledgmentCount: Number(record.acknowledgmentCount),
    fetched: Number(record.fetched),
  };
}

/** Disposable compile-gated QA only; both commands accept zero renderer input. */
export async function runNativeDiscordHeadlessQa(): Promise<NativeDiscordOverlayPrepared | null> {
  if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1") return null;
  try {
    return parseNativeDiscordOverlayPrepared(await invoke<unknown>("run_native_discord_headless_qa"));
  } catch {
    return null;
  }
}

export async function pollNativeDiscordHeadlessQa(): Promise<DiscordHeadlessQaPoll | null> {
  if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1") return null;
  try {
    return parseDiscordHeadlessQaPoll(await invoke<unknown>("poll_native_discord_headless_qa"));
  } catch {
    return null;
  }
}

/** Zero-input trusted QA caller for the persisted native/broker runtime receipt. */
export async function requestNativeDiscordVisibleRowRuntimeReceipt():
Promise<NativeVisibleRowRuntimeReceipt | null> {
  if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1") return null;
  try {
    return parseNativeVisibleRowRuntimeReceipt(
      await invoke<unknown>("request_native_discord_visible_row_qa_receipt"),
    );
  } catch {
    return null;
  }
}
