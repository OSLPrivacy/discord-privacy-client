import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  burnRevocationReceipt,
  REVOCATION_STATUS_ACKNOWLEDGED,
  REVOCATION_STATUS_NOT_ACKNOWLEDGED,
  REVOCATION_STATUS_SENT_REQUEST,
  type HubRevocationStatus,
  type HubScopeBurnOutcome,
} from "./burn-revocation-receipt";
import { parseHubRevocationStatus, parseHubScopeBurnOutcome } from "./adapters";

const STORAGE_KEY = "dm:peer-conversation-1";

function burned(overrides: Partial<HubScopeBurnOutcome> = {}): HubScopeBurnOutcome {
  return { storageKey: STORAGE_KEY, revocationsQueued: 2, revocationQueueComplete: true, ...overrides };
}

function status(overrides: Partial<HubRevocationStatus> = {}): HubRevocationStatus {
  return {
    storageKey: STORAGE_KEY,
    status: REVOCATION_STATUS_NOT_ACKNOWLEDGED,
    peersPending: 2,
    peersAcknowledged: 0,
    claims: ["one", "two", "three"],
    ...overrides,
  };
}

/** Every non-success state must read as a refusal, not as a softer success. */
function expectRefusal(receipt: ReturnType<typeof burnRevocationReceipt>): void {
  expect(receipt.tone).toBe("warning");
  expect(receipt.acknowledged).toBe(false);
  expect(receipt.line).toMatch(/not acknowledged|could not/i);
  expect(receipt.line).not.toMatch(/\bfinished\b|\bdone\b|\bcomplete\b|\bsuccess/i);
}

describe("burn revocation acknowledgement", () => {
  it("reports a queued-but-undelivered revocation as not acknowledged", () => {
    const receipt = burnRevocationReceipt(burned(), status());
    expectRefusal(receipt);
    expect(receipt.outstanding).toBe(2);
    expect(receipt.confirmed).toBe(0);
    expect(receipt.line).toContain("2 people");
    expect(receipt.line).toContain("still live");
  });

  it("reports a sent-but-unconfirmed revocation as not acknowledged", () => {
    const receipt = burnRevocationReceipt(burned(), status({ status: REVOCATION_STATUS_SENT_REQUEST }));
    expectRefusal(receipt);
    expect(receipt.line).toContain("OSL sent the revocation");
    expect(receipt.outstanding).toBe(2);
  });

  it("still refuses when only some of the people who had access confirmed", () => {
    const receipt = burnRevocationReceipt(
      burned(),
      status({ status: REVOCATION_STATUS_SENT_REQUEST, peersPending: 1, peersAcknowledged: 1 }),
    );
    expectRefusal(receipt);
    expect(receipt.outstanding).toBe(1);
    expect(receipt.confirmed).toBe(1);
    expect(receipt.line).toContain("1 person");
  });

  it("reads as acknowledged only once nothing is outstanding", () => {
    const receipt = burnRevocationReceipt(
      burned(),
      status({ status: REVOCATION_STATUS_ACKNOWLEDGED, peersPending: 0, peersAcknowledged: 2 }),
    );
    expect(receipt.tone).toBe("success");
    expect(receipt.acknowledged).toBe(true);
    expect(receipt.outstanding).toBe(0);
    expect(receipt.confirmed).toBe(2);
    expect(receipt.line).toContain("2 people who had access confirmed");
  });

  it("refuses when the queue itself was incomplete, even with nothing pending", () => {
    // A peer OSL could not address never reaches the outbox at all, so
    // `peersPending` is silent about them. `revocationQueueComplete` is not.
    const receipt = burnRevocationReceipt(
      burned({ revocationQueueComplete: false, revocationsQueued: 0 }),
      status({ peersPending: 0, peersAcknowledged: 0 }),
    );
    expectRefusal(receipt);
  });

  it("refuses when the status could not be read at all", () => {
    expectRefusal(burnRevocationReceipt(burned(), null));
  });

  it("refuses a status that answers about a different conversation", () => {
    expectRefusal(burnRevocationReceipt(burned(), status({ storageKey: "dm:someone-else" })));
  });

  it("allows a plain success only when there was nobody to tell", () => {
    const receipt = burnRevocationReceipt(
      burned({ revocationsQueued: 0 }),
      status({ peersPending: 0, peersAcknowledged: 0 }),
    );
    expect(receipt.tone).toBe("success");
    expect(receipt.acknowledged).toBe(true);
    expect(receipt.line).toContain("no revocation to send");
  });

  it("refuses when nothing is pending but the notices it queued are unaccounted for", () => {
    const receipt = burnRevocationReceipt(
      burned({ revocationsQueued: 2 }),
      status({ peersPending: 0, peersAcknowledged: 0 }),
    );
    expectRefusal(receipt);
  });
});

describe("revocation wire parsing", () => {
  it("accepts the exact HubRevocationStatusDto shape and rejects anything else", () => {
    const wire = {
      storageKey: STORAGE_KEY,
      status: REVOCATION_STATUS_NOT_ACKNOWLEDGED,
      peersPending: 2,
      peersAcknowledged: 0,
      claims: ["a", "b", "c"],
    };
    expect(parseHubRevocationStatus(wire)).toEqual(wire);
    expect(parseHubRevocationStatus({ ...wire, status: "Deleted" })).toBeNull();
    expect(parseHubRevocationStatus({ ...wire, peersPending: -1 })).toBeNull();
    expect(parseHubRevocationStatus({ ...wire, extra: 1 })).toBeNull();
    expect(parseHubRevocationStatus(null)).toBeNull();
  });

  it("keeps the burn fields the acknowledgement read-back needs", () => {
    const wire = {
      storageKey: STORAGE_KEY,
      rowsDestroyed: 3,
      revocationsQueued: 2,
      revocationQueueComplete: false,
      claims: ["a"],
    };
    expect(parseHubScopeBurnOutcome(wire)).toEqual({
      storageKey: STORAGE_KEY,
      revocationsQueued: 2,
      revocationQueueComplete: false,
    });
    expect(parseHubScopeBurnOutcome({ ...wire, revocationQueueComplete: "yes" })).toBeNull();
    expect(parseHubScopeBurnOutcome({ ...wire, storageKey: "" })).toBeNull();
  });
});

describe("the outstanding line is styled, and styled red", () => {
  // The app ships CSP `style-src 'self'`: a runtime <style> element and an
  // inline style= attribute are both dropped, so the only place this line can
  // get its refusal colour is styles.css itself.
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  it("gives .burn-revocation-line the danger rule and only overrides it when acknowledged", () => {
    const block = styles.slice(styles.indexOf(".burn-revocation-line {"));
    expect(block.length).toBeGreaterThan(0);
    expect(block.slice(0, block.indexOf("}"))).toContain("border-left: 3px solid var(--danger)");
    expect(styles).toContain(".burn-revocation-line.acknowledged { border-left-color: var(--brand);");
  });
});
