import { beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: () => true }));

import {
  grantBrowserProfileConsent,
  loadDetectedBrowserFootprint,
  revokeDetectedBrowserFootprint,
  scanConsentedBrowserProfile,
} from "./services";

const browserId = "chrome" as const;
const profile = "Default";
const account = "history-footprint";
const grantId = "a".repeat(64);

beforeEach(() => native.invoke.mockReset());

describe("browser consent IPC contract", () => {
  it("sends the native consent grant request envelope", async () => {
    native.invoke.mockResolvedValue({ browserId, profile, grantId, expiresAtUnixMs: Date.now() + 30_000 });

    await grantBrowserProfileConsent(browserId, profile);

    expect(native.invoke).toHaveBeenCalledWith("grant_browser_profile_consent", {
      request: { browserId, profile },
    });
  });

  it("accepts the receipt actually returned by the native scan command", async () => {
    native.invoke.mockResolvedValue({
      browserId,
      profile,
      account,
      scope: "history-footprint",
      runId: "run-1",
      observationCount: 1,
      snapshotDeleted: true,
    });

    await expect(scanConsentedBrowserProfile(browserId, profile, grantId)).resolves.toMatchObject({
      browserId,
      profile,
      scope: "history-footprint",
    });
  });

  it("sends the native scan request envelope", async () => {
    native.invoke.mockResolvedValue({
      browserId,
      profile,
      account,
      scope: "history-footprint",
      runId: "run-1",
      observationCount: 1,
      snapshotDeleted: true,
    });

    await scanConsentedBrowserProfile(browserId, profile, grantId);

    expect(native.invoke).toHaveBeenCalledWith("scan_consented_browser_profile", {
      request: { browserId, profile },
      grantId,
    });
  });

  it("accepts the observation list actually returned by the native hydration command", async () => {
    native.invoke.mockResolvedValue([{ browserId, browserProfileAccount: account, browserProfileId: profile, importRunId: "run-1", observedAtUnixMs: 1 }]);

    await expect(loadDetectedBrowserFootprint([{
      schemaVersion: 1,
      authority: "native-owner-bound-encrypted-store",
      ownerBindingSha256: "b".repeat(64),
      browserId,
      profile,
      account,
      scope: "browser-profile-import",
      runId: "run-1",
      buildSha256: "c".repeat(64),
      generation: 1,
      persistedCount: 1,
      immediateRereadCount: 1,
      observationsSha256: "d".repeat(64),
      sealedSha256: "e".repeat(64),
      rollbackStatus: "unproven",
    }])).resolves.toMatchObject({ observations: expect.any(Array) });
  });

  it("accepts the native revoke command's unit response", async () => {
    native.invoke.mockResolvedValue(undefined);

    await expect(revokeDetectedBrowserFootprint(browserId, profile, account)).resolves.toBeUndefined();
  });
});
