import { afterEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { getSignalProtectedSendReadiness } from "./signal-qa-ipc";

afterEach(() => {
  invoke.mockReset();
  vi.unstubAllGlobals();
});

describe("Signal QA readiness IPC", () => {
  it("does not invoke a backend in a browser preview", async () => {
    vi.stubGlobal("window", {});
    await expect(getSignalProtectedSendReadiness()).resolves.toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("queries the read-only readiness command without renderer arguments", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    const receipt = { status: "accepted", reason: "none", lifecycleGeneration: 1, attestationGeneration: 2, validForMs: 2_000 };
    invoke.mockResolvedValueOnce(receipt);

    await expect(getSignalProtectedSendReadiness()).resolves.toBe(receipt);
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith("get_signal_protected_send_readiness");
  });

  it("fails closed when the readiness query rejects", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invoke.mockRejectedValueOnce(new Error("unavailable"));
    await expect(getSignalProtectedSendReadiness()).resolves.toBeNull();
  });
});
