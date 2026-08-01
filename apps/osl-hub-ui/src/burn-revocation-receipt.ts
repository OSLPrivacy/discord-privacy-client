// Did the revocation this burn queued actually reach anyone?
//
// `queue_scope_revocations_locked` (apps/osl-hub/src/security.rs) says a burn
// whose peer notices are only QUEUED means the operator "must be shown
// `Not acknowledged` — never a success", and `HubScopeBurnResult` documents
// `revocationsQueued` as "Queued, not delivered". The producer of that answer,
// `revocation_status`, had no Tauri command, no permission and no caller, so
// the burn dialog reported every local cleanup as "Finished" regardless of
// whether a single peer had accepted the revocation.
//
// This is the projection that decides the dialog's tone. It is deliberately
// pure: it takes the burn outcome and the status read back from
// `get_hub_revocation_status`, and returns the tone plus one sentence. The
// only success it will ever return is one where nothing is outstanding.

/** Wire shape of `HubRevocationStatusDto` (apps/osl-hub/src/security.rs). */
export interface HubRevocationStatus {
  storageKey: string;
  /** `Sent request` | `Acknowledged by peer` | `Not acknowledged`. */
  status: string;
  peersPending: number;
  peersAcknowledged: number;
  claims: string[];
}

/** The part of `HubScopeBurnResult` this projection reads. */
export interface HubScopeBurnOutcome {
  storageKey: string;
  revocationsQueued: number;
  revocationQueueComplete: boolean;
}

/** `warning` renders red — see `.burn-result-message.warning` in styles.css. */
export type BurnRevocationTone = "success" | "warning";

export interface BurnRevocationReceipt {
  tone: BurnRevocationTone;
  /** True only when nothing is outstanding. Never a guess. */
  acknowledged: boolean;
  outstanding: number;
  confirmed: number;
  /** One plain sentence, appended under the local-cleanup line. */
  line: string;
}

export const REVOCATION_STATUS_SENT_REQUEST = "Sent request";
export const REVOCATION_STATUS_ACKNOWLEDGED = "Acknowledged by peer";
export const REVOCATION_STATUS_NOT_ACKNOWLEDGED = "Not acknowledged";

function boundedCount(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= 10_000;
}

function people(count: number): string {
  return count === 1 ? "1 person" : `${count} people`;
}

/**
 * The one rule this must never break: an outstanding peer is not a success.
 *
 * An unreadable status is also not a success — OSL claiming "done" because it
 * failed to check is the same lie as claiming "done" because nobody answered.
 */
export function burnRevocationReceipt(
  outcome: HubScopeBurnOutcome,
  status: HubRevocationStatus | null,
): BurnRevocationReceipt {
  if (status === null
    || status.storageKey !== outcome.storageKey
    || !boundedCount(status.peersPending)
    || !boundedCount(status.peersAcknowledged)) {
    return {
      tone: "warning",
      acknowledged: false,
      outstanding: 0,
      confirmed: 0,
      line: "Not acknowledged: OSL could not read whether anyone else's app accepted the revocation, so it is not claiming that it did.",
    };
  }

  const outstanding = status.peersPending;
  const confirmed = status.peersAcknowledged;
  const alsoConfirmed = confirmed > 0 ? ` (${confirmed} did)` : "";

  // Resolution itself was incomplete: somebody who had access could not even be
  // queued, so the outstanding count above is a floor, not the whole answer.
  if (outcome.revocationQueueComplete !== true) {
    return {
      tone: "warning",
      acknowledged: false,
      outstanding,
      confirmed,
      line: `Not acknowledged: OSL could not queue the revocation for everyone who had access${alsoConfirmed}. Treat that access as still live.`,
    };
  }

  if (outstanding > 0) {
    return {
      tone: "warning",
      acknowledged: false,
      outstanding,
      confirmed,
      line: status.status === REVOCATION_STATUS_SENT_REQUEST
        ? `Not acknowledged: OSL sent the revocation, but ${people(outstanding)} who had access ${outstanding === 1 ? "has" : "have"} not confirmed it${alsoConfirmed}. Until their app confirms, treat that access as still live.`
        : `Not acknowledged: the revocation for ${people(outstanding)} who had access has not been delivered yet${alsoConfirmed}. Until it is delivered and confirmed, treat that access as still live.`,
    };
  }

  if (confirmed > 0) {
    return {
      tone: "success",
      acknowledged: true,
      outstanding: 0,
      confirmed,
      line: `Acknowledged: ${people(confirmed)} who had access confirmed the revocation.`,
    };
  }

  // Nothing outstanding, nothing confirmed, and nothing was ever queued: the
  // conversation had no other approved device to tell. That is the only empty
  // answer allowed to read as done.
  if (outcome.revocationsQueued === 0) {
    return {
      tone: "success",
      acknowledged: true,
      outstanding: 0,
      confirmed: 0,
      line: "Nobody else's app had access to this chat, so there was no revocation to send.",
    };
  }

  return {
    tone: "warning",
    acknowledged: false,
    outstanding: 0,
    confirmed: 0,
    line: `Not acknowledged: OSL queued ${people(outcome.revocationsQueued)}'s revocation but can no longer account for it, so it is not claiming it was accepted.`,
  };
}
