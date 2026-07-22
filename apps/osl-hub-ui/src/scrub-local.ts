import type { LocalMessageCandidate, PersistedLocalPrivacyScanResult } from "./adapters";
import {
  appendScrubIndexChunk,
  cancelScrubIndex,
  getScrubIndexScan,
  getScrubIndexStatus,
  initializeScrubIndex,
  type ScrubIndexStatus,
} from "./scrub-index";
import { validateCoverageReceipt, type ScrubCoverageReceipt } from "./scrub-plan";

const SCRUB_CHUNK_MESSAGES = 256;

export interface PersistedLocalScrubScan {
  scan: PersistedLocalPrivacyScanResult;
  status: ScrubIndexStatus;
  receipt: ScrubCoverageReceipt;
}

export interface LocalScrubIndexAdapters {
  getStatus: typeof getScrubIndexStatus;
  initialize: typeof initializeScrubIndex;
  cancel: typeof cancelScrubIndex;
  append: typeof appendScrubIndexChunk;
  readScan: typeof getScrubIndexScan;
}

const defaultAdapters: LocalScrubIndexAdapters = {
  getStatus: getScrubIndexStatus,
  initialize: initializeScrubIndex,
  cancel: cancelScrubIndex,
  append: appendScrubIndexChunk,
  readScan: getScrubIndexScan,
};

export function localImportCoverageReceipt(
  messages: readonly LocalMessageCandidate[],
  scan: PersistedLocalPrivacyScanResult,
): ScrubCoverageReceipt {
  const timestamps = messages.flatMap(({ createdAtUnixMs }) => createdAtUnixMs === null ? [] : [createdAtUnixMs]);
  const gaps = ["The provider did not attest that this manual export is complete."];
  if (timestamps.length !== messages.length) gaps.push("Some exported messages did not include a reachable timestamp.");
  const receipt: ScrubCoverageReceipt = {
    messagesScanned: scan.messagesScanned,
    oldestReachableAtUnixMs: timestamps.length ? Math.min(...timestamps) : null,
    newestReachableAtUnixMs: timestamps.length ? Math.max(...timestamps) : null,
    providerReportedComplete: false,
    gaps,
    textChecked: true,
    imagesChecked: scan.imagesChecked,
    videosChecked: scan.videosChecked,
    attachmentsScanned: scan.attachmentsScanned,
    attachmentTypesScanned: [...scan.attachmentTypesScanned],
    uninspectedAttachments: [...scan.uninspectedAttachments],
  };
  if (!validateCoverageReceipt(receipt)) throw new Error("invalid Scrub coverage receipt");
  return receipt;
}

export async function persistLocalScrubExport(
  messages: LocalMessageCandidate[],
  adapters: LocalScrubIndexAdapters = defaultAdapters,
): Promise<PersistedLocalScrubScan> {
  if (!messages.length) throw new Error("Scrub requires at least one local message");
  const initialization = {
    selections: [{ serviceId: "local_import", accountId: "manual-export" }],
    source: "explicit_export" as const,
  };
  let status = await adapters.getStatus();
  if (status && (status.source !== "explicit_export" || status.selectedAccountCount !== 1)) {
    throw new Error("A different Scrub import is already in progress");
  }
  // A status row does not authenticate which export file produced it. Never
  // resume a different file into an old sequence: cancel the exact prior import
  // and start a new random import id for this explicit user-selected export.
  if (status) await adapters.cancel(status.importId);
  status = await adapters.initialize(initialization);
  const chunks = Array.from(
    { length: Math.ceil(messages.length / SCRUB_CHUNK_MESSAGES) },
    (_, sequence) => messages.slice(sequence * SCRUB_CHUNK_MESSAGES, (sequence + 1) * SCRUB_CHUNK_MESSAGES),
  );
  for (let sequence = 0; sequence < chunks.length; sequence += 1) {
    status = await adapters.append({
      importId: status.importId,
      sequence,
      finalChunk: sequence === chunks.length - 1,
      messages: chunks[sequence],
    });
  }
  const persistedStatus = await adapters.getStatus();
  if (!persistedStatus
    || persistedStatus.importId !== status.importId
    || persistedStatus.phase !== "complete"
    || !persistedStatus.persistedEncrypted
    || persistedStatus.deletionEnabled) {
    throw new Error("Scrub index did not finish safely");
  }
  const scan = await adapters.readScan(persistedStatus.importId);
  if (scan.messagesScanned !== persistedStatus.messagesIndexed
    || scan.findings.length !== persistedStatus.findingsIndexed
    || scan.messagesRejected !== persistedStatus.rejectedMessages) {
    throw new Error("Persisted Scrub scan does not match its encrypted index status");
  }
  return {
    scan,
    status: persistedStatus,
    receipt: localImportCoverageReceipt(messages, scan),
  };
}
