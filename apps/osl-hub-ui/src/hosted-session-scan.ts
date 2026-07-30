import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";

export type HostedSessionScanWalk = "complete" | "truncated";

export interface HostedSessionScanRowShape {
  heightPx: number;
  children: number;
}

export interface HostedSessionScanCandidate {
  scanOrdinal: number;
  shapeOrdinal: number;
  shape: HostedSessionScanRowShape;
  textLen: number;
  authoredByOperator: boolean;
}

export interface HostedSessionScan {
  scopeBindingHash: string;
  generation: number;
  rowsSeen: number;
  rowsUnreadable: number;
  walk: HostedSessionScanWalk;
  candidates: HostedSessionScanCandidate[];
}

export async function openHostedSessionScan(): Promise<void> {
  if (!isTauriRuntime()) throw new Error("hosted session scan unavailable");
  const raw = await invoke<unknown>("open_hosted_session_scan");
  if (raw !== undefined && raw !== null) {
    throw new Error("invalid hosted session scan open response");
  }
}

export async function requestHostedSessionScanCommand(): Promise<HostedSessionScan> {
  if (!isTauriRuntime()) throw new Error("hosted session scan unavailable");
  return parseHostedSessionScan(await invoke<unknown>("request_hosted_session_scan"));
}

export function parseHostedSessionScan(raw: unknown): HostedSessionScan {
  if (!isExactRecord(raw, [
    "scopeBindingHash",
    "generation",
    "rowsSeen",
    "rowsUnreadable",
    "walk",
    "candidates",
  ])) {
    throw new Error("invalid hosted session scan response");
  }
  if (!isSha256(raw.scopeBindingHash)
    || !isSafeNonNegativeInteger(raw.generation)
    || raw.generation < 1
    || !isSafeNonNegativeInteger(raw.rowsSeen)
    || !isSafeNonNegativeInteger(raw.rowsUnreadable)
    || !["complete", "truncated"].includes(String(raw.walk))
    || !Array.isArray(raw.candidates)
    || raw.candidates.length > raw.rowsSeen) {
    throw new Error("invalid hosted session scan response");
  }
  const candidates = raw.candidates.map(parseHostedSessionScanCandidate);
  return { ...raw, candidates } as HostedSessionScan;
}

function parseHostedSessionScanCandidate(raw: unknown): HostedSessionScanCandidate {
  if (!isExactRecord(raw, [
    "scanOrdinal",
    "shapeOrdinal",
    "shape",
    "textLen",
    "authoredByOperator",
  ])) {
    throw new Error("invalid hosted session scan response");
  }
  if (!isSafeNonNegativeInteger(raw.scanOrdinal)
    || !isSafeNonNegativeInteger(raw.shapeOrdinal)
    || !isSafeNonNegativeInteger(raw.textLen)
    || typeof raw.authoredByOperator !== "boolean") {
    throw new Error("invalid hosted session scan response");
  }
  const shape = parseHostedSessionScanRowShape(raw.shape);
  return { ...raw, shape } as HostedSessionScanCandidate;
}

function parseHostedSessionScanRowShape(raw: unknown): HostedSessionScanRowShape {
  if (!isExactRecord(raw, ["heightPx", "children"])
    || !isSafeNonNegativeInteger(raw.heightPx)
    || !isSafeNonNegativeInteger(raw.children)
    || raw.heightPx < 1
    || raw.children > 10_000) {
    throw new Error("invalid hosted session scan response");
  }
  return {
    heightPx: raw.heightPx,
    children: raw.children,
  };
}

function isExactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return false;
  const actual = Object.keys(value);
  return actual.length === keys.length && keys.every((key) => Object.prototype.hasOwnProperty.call(value, key));
}

function isSafeNonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) >= 0;
}

function isSha256(value: unknown): value is string {
  return typeof value === "string" && /^[a-f0-9]{64}$/u.test(value);
}
