import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  loadHubPasswordRoleStatus,
  parseHubPasswordRoleStatus,
  removeHubAlternatePassword,
  setHubAlternatePassword,
} from "./core";

const status = {
  mainPasswordSet: true,
  stealthPasswordSet: true,
  burnPasswordSet: false,
  unlocked: true,
  stealthActionWired: false,
  burnActionWired: false,
} as const;

describe("trusted local password-role IPC", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("parses only the exact status returned by the backend", () => {
    expect(parseHubPasswordRoleStatus(status)).toEqual(status);
    expect(() => parseHubPasswordRoleStatus({ ...status, stealthActionWired: "later" })).toThrow();
    expect(() => parseHubPasswordRoleStatus({ ...status, stealthLandingToken: "secret" })).toThrow();
  });

  it("loads role status through the single read-only command", async () => {
    mocks.invoke.mockResolvedValueOnce(status);
    await expect(loadHubPasswordRoleStatus()).resolves.toEqual(status);
    expect(mocks.invoke).toHaveBeenCalledWith("get_hub_password_role_status");
  });

  it("uses distinct, bounded arguments for stealth and burn setup", async () => {
    mocks.invoke.mockResolvedValue(status);
    await expect(setHubAlternatePassword("stealth", "main-123", "quiet-456")).resolves.toEqual(status);
    await expect(setHubAlternatePassword("burn", "main-123", "destroy-789")).resolves.toEqual(status);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "set_hub_stealth_password", {
      currentMain: "main-123",
      newStealth: "quiet-456",
    });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "set_hub_burn_password", {
      currentMain: "main-123",
      newBurn: "destroy-789",
    });
  });

  it("removes only the selected alternate password role", async () => {
    mocks.invoke.mockResolvedValue(status);
    await expect(removeHubAlternatePassword("stealth", "main-123")).resolves.toEqual(status);
    await expect(removeHubAlternatePassword("burn", "main-123")).resolves.toEqual(status);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "remove_hub_stealth_password", { currentMain: "main-123" });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "remove_hub_burn_password", { currentMain: "main-123" });
  });

  it("rejects invalid or equal passwords before IPC", async () => {
    await expect(setHubAlternatePassword("stealth", "short", "quiet-456")).rejects.toThrow();
    await expect(setHubAlternatePassword("stealth", "same-123", "same-123")).rejects.toThrow();
    await expect(setHubAlternatePassword("burn", "main-123", "bad\npassword")).rejects.toThrow();
    await expect(removeHubAlternatePassword("burn", "tiny")).rejects.toThrow();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("is unavailable outside the trusted Tauri runtime", async () => {
    mocks.isTauriRuntime.mockReturnValue(false);
    await expect(loadHubPasswordRoleStatus()).rejects.toThrow();
    await expect(setHubAlternatePassword("burn", "main-123", "destroy-789")).rejects.toThrow();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
