import { describe, expect, it } from "vitest";

import {
  ENCLAVE_AUTHORITY_DISCLOSURE,
  ENCLAVE_BLOCK_DISCLOSURE,
  ENCLAVE_JOIN_ANSWER_DISCLOSURE,
  ENCLAVE_MEASURED_COST_DISCLOSURE,
  ENCLAVE_NO_MAXIMUM_DISCLOSURE,
  ENCLAVE_OWNERSHIP_DISCLOSURE,
  ENCLAVE_REVOKE_PENDING_DISCLOSURE,
  ENCLAVE_SECOND_OWNER_DISCLOSURE,
  ENCLAVE_STARTED_JOIN_DISCLOSURE,
  SIGNED_ENCLAVE_MEASUREMENT,
  enclaveEntryDisclosuresMarkup,
} from "./enclave-disclosures";

describe("TASK 4884 Enclave disclosures", () => {
  it("puts every available exact disclosure in the reachable Enclaves entry route", () => {
    const markup = enclaveEntryDisclosuresMarkup();
    for (const sentence of [
      "OSL has no report path and cannot ban, suspend, review or judge anyone. An enclave's owner or authorized role-holders can remove, mute, restrict or delete a message only inside that enclave.",
      "A started join can consume an invite use even if approval is never completed; the owner may need to issue a new invite.",
      "Revoking an invite does not cancel a join already awaiting approval or restore the consumed use. This release cannot remove that pending request; do not approve it, and issue a new invite if needed.",
      "Enclave ownership cannot be transferred in this release; the last owner must remain.",
      "An enclave is not created until a second owner accepts. You cannot create one alone.",
      "Enclaves have no maximum member count. Above 500 members, removing someone takes time and shows progress while OSL re-keys the enclave.",
      "Enclave join answers are encrypted to the approvers for that request. Approved or pending answers may remain on their devices; do not submit secrets.",
      "Messages from blocked accounts are still delivered to this device. They stay hidden while blocked and may appear if you unblock.",
    ]) expect(markup).toContain(sentence);
    expect(markup).toContain('data-enclave-disclosure="authority"');
    expect(markup).not.toContain("report this");
  });

  it("uses the signed threshold as a removal threshold, never a membership cap", () => {
    expect(SIGNED_ENCLAVE_MEASUREMENT.thresholdMembers).toBe(500);
    expect(ENCLAVE_NO_MAXIMUM_DISCLOSURE).toBe("Enclaves have no maximum member count. Above 500 members, removing someone takes time and shows progress while OSL re-keys the enclave.");
  });

  it("refuses to invent the required measured cost disclosure without a complete signed row", () => {
    expect(ENCLAVE_MEASURED_COST_DISCLOSURE).toBeNull();
    expect(enclaveEntryDisclosuresMarkup()).toContain('data-enclave-disclosure="measurement-unavailable"');
  });
});
