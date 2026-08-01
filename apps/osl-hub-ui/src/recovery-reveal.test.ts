import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { captureProtectionEnforced, setScreenshotProtection, viewHubRecoveryPhrase } from "./adapters";

const PHRASE = "mike november oscar papa quebec romeo sierra tango uniform victor whiskey xray";

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
});

describe("T15-A3/A4 the recovery phrase can be read back later", () => {
  it("re-reads the phrase through the registered command, re-authenticating first", async () => {
    mocks.invoke.mockResolvedValue(PHRASE);

    await expect(viewHubRecoveryPhrase("correct horse battery")).resolves.toBe(PHRASE);
    expect(mocks.invoke).toHaveBeenCalledWith("view_hub_recovery_phrase", {
      current: "correct horse battery",
    });
  });

  it("fails closed and shows nothing when the backend refuses the password", async () => {
    mocks.invoke.mockRejectedValue(new Error("OSL: current password incorrect"));

    await expect(viewHubRecoveryPhrase("wrong")).resolves.toBeNull();
  });

  it("never sends an empty password to the backend", async () => {
    await expect(viewHubRecoveryPhrase("")).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("rejects a response that is not a phrase", async () => {
    mocks.invoke.mockResolvedValue({ phrase: PHRASE });

    await expect(viewHubRecoveryPhrase("correct horse battery")).resolves.toBeNull();
  });
});

describe("T15 the capture-resistance claim comes from the platform", () => {
  it("reports no enforcement when the backend says the platform has none", async () => {
    mocks.invoke.mockResolvedValue(false);

    await expect(setScreenshotProtection(true)).resolves.toBe(true);
    // The call succeeded. That is NOT the same as the window being protected,
    // and the two must not be readable as one signal.
    expect(captureProtectionEnforced()).toBe(false);
  });

  it("reports enforcement only when the backend confirms it", async () => {
    mocks.invoke.mockResolvedValue(true);
    await expect(setScreenshotProtection(true)).resolves.toBe(true);
    expect(captureProtectionEnforced()).toBe(true);

    mocks.invoke.mockRejectedValue(new Error("Windows capture resistance could not be changed"));
    await expect(setScreenshotProtection(true)).resolves.toBe(false);
    expect(captureProtectionEnforced()).toBe(false);
  });
});
