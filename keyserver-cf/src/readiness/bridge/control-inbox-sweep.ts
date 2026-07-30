export interface BridgeControlInboxSweepResult {
  inboxRows: number;
  requestReceipts: number;
  senderStates: {
    examined: number;
    reenabled: number;
    retryable: number;
    quarantined: number;
    retired: number;
  };
}

/**
 * Artifact A must remain completely independent of migration 0031. In
 * particular, its scheduled handler cannot classify rows or name any of the
 * status columns added by that migration.
 */
export async function sweepExpiredControlInboxRows(
  _db: D1Database,
): Promise<BridgeControlInboxSweepResult> {
  return {
    inboxRows: 0,
    requestReceipts: 0,
    senderStates: {
      examined: 0,
      reenabled: 0,
      retryable: 0,
      quarantined: 0,
      retired: 0,
    },
  };
}
