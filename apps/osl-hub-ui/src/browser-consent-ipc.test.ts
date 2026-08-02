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

    const receipt = {
      browserId,
      profile,
      account,
      scope: "history-footprint" as const,
      runId: "run-1",
      observationCount: 1,
      snapshotDeleted: true as const,
    };
    await expect(loadDetectedBrowserFootprint([receipt])).resolves.toMatchObject({ observations: expect.any(Array) });
    expect(native.invoke).toHaveBeenCalledWith("load_detected_browser_footprint", {
      consents: [{
        browserId,
        browserProfileAccount: account,
        browserProfileId: profile,
        importRunId: "run-1",
        consent: true,
      }],
    });
  });

  it("accepts the native revoke command's unit response", async () => {
    native.invoke.mockResolvedValue(undefined);

    await expect(revokeDetectedBrowserFootprint(browserId, profile, account, "run-1")).resolves.toBeUndefined();
    expect(native.invoke).toHaveBeenCalledWith("revoke_detected_browser_footprint", {
      request: {
        browserId,
        browserProfileAccount: account,
        browserProfileId: profile,
        importRunId: "run-1",
        consent: false,
      },
    });
  });
});
