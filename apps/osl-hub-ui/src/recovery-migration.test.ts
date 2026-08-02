import { describe, expect, it, vi } from "vitest";
import {
  addLegacyPhraseWrap,
  chooseLegacyFreshStart,
  legacyMarkerRecoveryRefused,
  legacyRecoveryMigrationMarkup,
  type RecoveryMigrationDependencies,
} from "./recovery-migration";

describe("T15-T9 legacy recovery-marker migration", () => {
  it("refuses phrase-only recovery, offers exactly the safe repair or an explicit fresh start, and preserves peer_map during repair", async () => {
    const peerMap = new Uint8Array([0x7b, 0x22, 0x70, 0x65, 0x65, 0x72, 0x22, 0x3a, 0x31, 0x7d]);
    const before = peerMap.slice();
    const dependencies: RecoveryMigrationDependencies = {
      addPhraseWrap: vi.fn(async (current) => {
        expect(current).toBe("current-password");
        // Models the native rotate/write operation: repairing the marker is
        // not a reset and may not alter encrypted state bytes.
        expect(peerMap).toEqual(before);
      }),
      freshStart: vi.fn(),
    };
    const refusal = legacyRecoveryMigrationMarkup({ kind: "needs-current-password", phraseVerified: true });

    expect(legacyMarkerRecoveryRefused(new Error("OSL: cannot complete recovery — this account's encrypted state"))).toBe(true);
    expect(refusal).toContain("Re-enter your CURRENT password now so we can add the wrap");
    expect(refusal).toContain("Start over and lose your burn list");
    expect(refusal).not.toContain("Reset password");

    await expect(addLegacyPhraseWrap("", dependencies)).resolves.toEqual({ kind: "needs-current-password", phraseVerified: true });
    expect(dependencies.addPhraseWrap).not.toHaveBeenCalled();
    await expect(addLegacyPhraseWrap("current-password", dependencies)).resolves.toEqual({ kind: "recoverable" });
    expect(peerMap).toEqual(before);
    expect(dependencies.freshStart).not.toHaveBeenCalled();
  });

  it("keeps Fresh Start an explicit destructive choice", async () => {
    const dependencies: RecoveryMigrationDependencies = { addPhraseWrap: vi.fn(), freshStart: vi.fn() };
    await expect(chooseLegacyFreshStart(dependencies)).resolves.toEqual({ kind: "fresh-start" });
    expect(dependencies.freshStart).toHaveBeenCalledOnce();
    expect(dependencies.addPhraseWrap).not.toHaveBeenCalled();
  });
});
