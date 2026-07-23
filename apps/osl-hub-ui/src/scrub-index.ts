import { invoke } from "@tauri-apps/api/core";
import { parsePersistedLocalPrivacyScan, type LocalMessageCandidate, type PersistedLocalPrivacyScanResult } from "./adapters";
import { isTauriRuntime } from "./preferences";

export type ScrubIndexSource = "explicit_export" | "osl_visible_data";
export type ScrubIndexPhase = "running" | "paused" | "complete";

export interface ScrubAccountSelection {
  serviceId: string;
  accountId: string;
}

export interface ScrubIndexStatus {
  importId: string;
  phase: ScrubIndexPhase;
  source: ScrubIndexSource;
  selectedAccountCount: number;
  messagesIndexed: number;
  findingsIndexed: number;
  rejectedMessages: number;
  completedChunks: number;
  nextSequence: number;
  bytesStored: number;
  maxBytes: number;
  analysisLocation: "this_device_only";
  persistedEncrypted: true;
  deletionEnabled: false;
}

export interface ScrubIndexInitializeRequest {
  selections: ScrubAccountSelection[];
  source: ScrubIndexSource;
}

export interface ScrubIndexChunkRequest {
  importId: string;
  sequence: number;
  finalChunk: boolean;
  messages: LocalMessageCandidate[];
}

const MAX_INDEX_BYTES = 50 * 1024 * 1024;
const STATUS_KEYS = [
  "importId", "phase", "source", "selectedAccountCount", "messagesIndexed",
  "findingsIndexed", "rejectedMessages", "completedChunks", "nextSequence",
  "bytesStored", "maxBytes", "analysisLocation", "persistedEncrypted", "deletionEnabled",
] as const;

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function boundedInteger(value: unknown, maximum = Number.MAX_SAFE_INTEGER): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= maximum;
}

function validImportId(value: unknown): value is string {
  return typeof value === "string" && /^[a-f0-9]{32}$/u.test(value);
}

function validSelection(value: unknown): value is ScrubAccountSelection {
  return exactRecord(value, ["serviceId", "accountId"])
    && typeof value.serviceId === "string"
    && /^[a-z0-9_-]{1,32}$/u.test(value.serviceId)
    && typeof value.accountId === "string"
    && /^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/u.test(value.accountId);
}

function validateInitializeRequest(request: ScrubIndexInitializeRequest): void {
  if (!Array.isArray(request.selections)
    || request.selections.length < 1
    || request.selections.length > 32
    || request.selections.some((selection) => !validSelection(selection))
    || new Set(request.selections.map((selection) => `${selection.serviceId}\0${selection.accountId}`)).size !== request.selections.length
    || !(["explicit_export", "osl_visible_data"] as const).includes(request.source)) {
    throw new Error("invalid Scrub initialization request");
  }
}

function validateChunkRequest(request: ScrubIndexChunkRequest): void {
  if (!validImportId(request.importId)
    || !boundedInteger(request.sequence, 4_095)
    || typeof request.finalChunk !== "boolean"
    || !Array.isArray(request.messages)
    || request.messages.length < 1
    || request.messages.length > 256) {
    throw new Error("invalid Scrub chunk request");
  }
}

export function parseScrubIndexStatus(raw: unknown): ScrubIndexStatus {
  if (!exactRecord(raw, STATUS_KEYS)
    || !validImportId(raw.importId)
    || !(["running", "paused", "complete"] as const).includes(raw.phase as ScrubIndexPhase)
    || !(["explicit_export", "osl_visible_data"] as const).includes(raw.source as ScrubIndexSource)
    || !boundedInteger(raw.selectedAccountCount, 32)
    || !boundedInteger(raw.messagesIndexed)
    || !boundedInteger(raw.findingsIndexed)
    || !boundedInteger(raw.rejectedMessages)
    || !boundedInteger(raw.completedChunks, 4_096)
    || !boundedInteger(raw.nextSequence, 4_096)
    || raw.completedChunks !== raw.nextSequence
    || !boundedInteger(raw.bytesStored, MAX_INDEX_BYTES)
    || raw.maxBytes !== MAX_INDEX_BYTES
    || raw.analysisLocation !== "this_device_only"
    || raw.persistedEncrypted !== true
    || raw.deletionEnabled !== false) {
    throw new Error("invalid Scrub index status");
  }
  return raw as unknown as ScrubIndexStatus;
}

function requireNative(): void {
  if (!isTauriRuntime()) throw new Error("Scrub indexing requires the OSL Privacy desktop app");
}

export async function initializeScrubIndex(request: ScrubIndexInitializeRequest): Promise<ScrubIndexStatus> {
  requireNative();
  validateInitializeRequest(request);
  return parseScrubIndexStatus(await invoke<unknown>("initialize_scrub_index", { request }));
}

export async function appendScrubIndexChunk(request: ScrubIndexChunkRequest): Promise<ScrubIndexStatus> {
  requireNative();
  validateChunkRequest(request);
  return parseScrubIndexStatus(await invoke<unknown>("append_scrub_index_chunk", { request }));
}

export async function getScrubIndexStatus(): Promise<ScrubIndexStatus | null> {
  requireNative();
  const raw = await invoke<unknown>("get_scrub_index_status");
  return raw === null ? null : parseScrubIndexStatus(raw);
}

export async function getScrubIndexScan(importId: string): Promise<PersistedLocalPrivacyScanResult> {
  requireNative();
  if (!validImportId(importId)) throw new Error("invalid Scrub import identifier");
  const result = parsePersistedLocalPrivacyScan(await invoke<unknown>("get_scrub_index_scan", { importId }));
  if (!result) throw new Error("invalid persisted Scrub scan");
  return result;
}

export async function pauseScrubIndex(importId: string): Promise<ScrubIndexStatus> {
  requireNative();
  if (!validImportId(importId)) throw new Error("invalid Scrub import identifier");
  return parseScrubIndexStatus(await invoke<unknown>("pause_scrub_index", { importId }));
}

export async function resumeScrubIndex(importId: string): Promise<ScrubIndexStatus> {
  requireNative();
  if (!validImportId(importId)) throw new Error("invalid Scrub import identifier");
  return parseScrubIndexStatus(await invoke<unknown>("resume_scrub_index", { importId }));
}

export async function cancelScrubIndex(importId: string): Promise<void> {
  requireNative();
  if (!validImportId(importId)) throw new Error("invalid Scrub import identifier");
  await invoke("cancel_scrub_index", { importId });
}
