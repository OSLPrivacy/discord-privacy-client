import type { ScrubReviewRow } from "./scrub-review-list";

export interface ScrubDeleteRequest {
  dedupKey: string;
  serviceId: string;
  accountId: string;
  logicalHost: string;
  confirmed: true;
}

export interface ScrubDeleteReceipt {
  dedupKey: string;
  requested: true;
  verifiedDeleted: false;
  manualReviewRequired: true;
}

export interface ScrubDeletionEngine {
  executeDeletion(request: ScrubDeleteRequest): Promise<ScrubDeleteReceipt>;
}

export interface AttendedImapRunResult {
  attempted: number;
  receipts: ScrubDeleteReceipt[];
}

export async function runAttendedImapDeletionBatch(
  rows: readonly ScrubReviewRow[],
  confirmedDedupKeys: ReadonlySet<string>,
  engine: ScrubDeletionEngine,
): Promise<AttendedImapRunResult> {
  const receipts: ScrubDeleteReceipt[] = [];
  for (const row of rows) {
    if (!confirmedDedupKeys.has(row.dedupKey)) continue;
    const receipt = await engine.executeDeletion({
      dedupKey: row.dedupKey,
      serviceId: row.serviceId,
      accountId: row.accountId,
      logicalHost: row.logicalHost,
      confirmed: true,
    });
    if (receipt.dedupKey !== row.dedupKey
      || receipt.requested !== true
      || receipt.verifiedDeleted !== false
      || receipt.manualReviewRequired !== true) {
      throw new Error("invalid attended deletion receipt");
    }
    receipts.push(receipt);
  }
  return { attempted: receipts.length, receipts };
}
