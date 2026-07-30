import { describe, expect, it, vi } from "vitest";
import {
  requestHostedDeleteCapability,
  scanHostedSession,
  validateDeleteCapabilityInput,
  validateOwnSessionProofInput,
  type DeleteCapabilityInput,
  type HostedSessionScanPort,
  type OwnSessionProofInput,
  type OwnerIdl,
  type SessionKind,
} from "./scrub-hosted-session-scan";

const ownerId: OwnerIdl = "owner-main";
const sessionKind: SessionKind = "browser_account";
const scopeFingerprint = "a".repeat(64);
const findingFingerprint = "b".repeat(64);

const ownerProof: OwnSessionProofInput = {
  ownerId,
  serviceId: "gmail",
  accountId: "mail-main",
  sessionKind,
  authEpoch: 5,
  proofNonce: "c".repeat(64),
  issuedAtUnixMs: 1_000,
  expiresAtUnixMs: 2_000,
};

const deleteInput: DeleteCapabilityInput = {
  ownerProof,
  scopeFingerprint,
  findingFingerprint,
  mode: "manual_jump",
  attended: true,
  unattended: false,
};

function port(overrides: Partial<HostedSessionScanPort> = {}): HostedSessionScanPort {
  return {
    scan: vi.fn().mockResolvedValue({
      ownerId: ownerProof.ownerId,
      serviceId: ownerProof.serviceId,
      accountId: ownerProof.accountId,
      sessionKind: ownerProof.sessionKind,
      authEpoch: ownerProof.authEpoch,
      scopeFingerprint,
      messagesScanned: 12,
      deleteCapability: "attended_only",
    }),
    requestDeleteCapability: vi.fn().mockResolvedValue("attended_only"),
    ...overrides,
  };
}

describe("scrub hosted session scan", () => {
  it("Define scrub-hosted-session-scan.ts typed renderer scan-port types", async () => {
    const adapter = port();
    const receipt = await scanHostedSession(adapter, {
      ownerProof,
      scopeFingerprint,
      maxMessages: 100,
    });

    expect(receipt).toMatchObject({
      ownerId: "owner-main",
      serviceId: "gmail",
      accountId: "mail-main",
      sessionKind: "browser_account",
      authEpoch: 5,
      deleteCapability: "attended_only",
    });
    expect(adapter.scan).toHaveBeenCalledOnce();

    const mismatched = port({
      scan: vi.fn().mockResolvedValue({ ...receipt, authEpoch: 4 }),
    });
    await expect(scanHostedSession(mismatched, {
      ownerProof,
      scopeFingerprint,
      maxMessages: 100,
    })).rejects.toThrow("invalid hosted session scan receipt");
  });

  it("Define SessionKind/OwnSessionProofInput/DeleteCapabilityInput/OwnerIdl", async () => {
    expect(validateOwnSessionProofInput(ownerProof)).toBe(true);
    expect(validateOwnSessionProofInput({ ...ownerProof, authEpoch: 0 })).toBe(false);
    expect(validateOwnSessionProofInput({ ...ownerProof, sessionKind: "copied_session" })).toBe(false);
    expect(validateDeleteCapabilityInput(deleteInput)).toBe(true);
    expect(validateDeleteCapabilityInput({ ...deleteInput, unattended: true })).toBe(false);
    expect(validateDeleteCapabilityInput({ ...deleteInput, attended: false })).toBe(false);

    const adapter = port();
    await expect(requestHostedDeleteCapability(adapter, deleteInput)).resolves.toBe("attended_only");
    expect(adapter.requestDeleteCapability).toHaveBeenCalledWith(deleteInput);
    await expect(requestHostedDeleteCapability(adapter, {
      ...deleteInput,
      findingFingerprint: "d".repeat(63),
    })).rejects.toThrow("invalid hosted session delete capability request");
  });
});
