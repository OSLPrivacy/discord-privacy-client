/**
 * Truthful, shared copy for the Enclaves route.  Keep these as complete
 * sentences: action views import this module rather than maintaining slightly
 * different versions of an authority or retention claim.
 */
export const ENCLAVE_AUTHORITY_DISCLOSURE = "OSL has no report path and cannot ban, suspend, review or judge anyone. An enclave's owner or authorized role-holders can remove, mute, restrict or delete a message only inside that enclave.";
export const ENCLAVE_STARTED_JOIN_DISCLOSURE = "A started join can consume an invite use even if approval is never completed; the owner may need to issue a new invite.";
export const ENCLAVE_REVOKE_PENDING_DISCLOSURE = "Revoking an invite does not cancel a join already awaiting approval or restore the consumed use. This release cannot remove that pending request; do not approve it, and issue a new invite if needed.";
export const ENCLAVE_OWNERSHIP_DISCLOSURE = "Enclave ownership cannot be transferred in this release; the last owner must remain.";
export const ENCLAVE_SECOND_OWNER_DISCLOSURE = "An enclave is not created until a second owner accepts. You cannot create one alone.";
export const ENCLAVE_NO_MAXIMUM_DISCLOSURE = "Enclaves have no maximum member count. Above 500 members, removing someone takes time and shows progress while OSL re-keys the enclave.";
export const ENCLAVE_JOIN_ANSWER_DISCLOSURE = "Enclave join answers are encrypted to the approvers for that request. Approved or pending answers may remain on their devices; do not submit secrets.";
export const ENCLAVE_BLOCK_DISCLOSURE = "Messages from blocked accounts are still delivered to this device. They stay hidden while blocked and may appear if you unblock.";

/** The only signed 4864/4865 fields available to this UI build. */
export const SIGNED_ENCLAVE_MEASUREMENT = Object.freeze({ thresholdMembers: 500 });

/**
 * 4864/4865 do not publish D/X/Y/Z/W or immutable raw bytes/timestamps in
 * this checkout.  Do not manufacture a plausible cost sentence: the release
 * bar explicitly requires the disclosure to remain open in that case.
 */
export const ENCLAVE_MEASURED_COST_DISCLOSURE: string | null = null;

function disclosure(id: string, sentence: string): string {
  return `<p class="enclave-disclosure" data-enclave-disclosure="${id}">${sentence}</p>`;
}

/**
 * The entry route is intentionally explicit about the disclosures applicable
 * before a person can choose creation or joining.  Each action surface should
 * reuse the matching literal above beside its actionable control.
 */
export function enclaveEntryDisclosuresMarkup(): string {
  const unavailableMeasurement = ENCLAVE_MEASURED_COST_DISCLOSURE === null
    ? '<p class="enclave-disclosure enclave-disclosure--unavailable" data-enclave-disclosure="measurement-unavailable">Measured join/removal cost is not shown because this build has no signed devices, data, and timing row.</p>'
    : disclosure("measured-cost", ENCLAVE_MEASURED_COST_DISCLOSURE);

  return `<section class="enclave-disclosures" aria-label="Enclave limits and authority">${disclosure("authority", ENCLAVE_AUTHORITY_DISCLOSURE)}${disclosure("second-owner", ENCLAVE_SECOND_OWNER_DISCLOSURE)}${disclosure("ownership", ENCLAVE_OWNERSHIP_DISCLOSURE)}${disclosure("no-maximum", ENCLAVE_NO_MAXIMUM_DISCLOSURE)}${disclosure("started-join", ENCLAVE_STARTED_JOIN_DISCLOSURE)}${disclosure("revoke-pending", ENCLAVE_REVOKE_PENDING_DISCLOSURE)}${disclosure("join-answer-retention", ENCLAVE_JOIN_ANSWER_DISCLOSURE)}${disclosure("blocking", ENCLAVE_BLOCK_DISCLOSURE)}${unavailableMeasurement}</section>`;
}
