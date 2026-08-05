import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { openHubSourceRepository, parseUpdateCheck } from "./updates";

describe("trusted OSL Privacy update contract", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("accepts bounded plain update metadata", () => {
    expect(parseUpdateCheck({ status: "update_available", current: "0.1.0", next: "0.2.0", notes: "Security and reliability fixes." })).toEqual({
      state: "available", current: "0.1.0", next: "0.2.0", notes: "Security and reliability fixes.",
    });
  });

  it("keeps current, unreachable, and available update checks distinguishable", () => {
    const checkedAndCurrent = parseUpdateCheck({ status: "up_to_date", current: "0.1.0" });
    const couldNotCheck = parseUpdateCheck({ status: "could_not_check" });
    const updateAvailable = parseUpdateCheck({ status: "update_available", current: "0.1.0", next: "0.2.0", notes: "Security and reliability fixes." });

    expect(checkedAndCurrent.state).toBe("upToDate");
    expect(couldNotCheck.state).toBe("couldNotCheck");
    expect(updateAvailable.state).toBe("available");
    expect(new Set([checkedAndCurrent.state, couldNotCheck.state, updateAvailable.state]).size).toBe(3);
  });

  it("rejects remote HTML fields, arbitrary URLs, and unknown states", () => {
    expect(parseUpdateCheck({ status: "update_available", current: "0.1.0", next: "0.2.0", notes: "ok", html: "<b>remote</b>" }).state).toBe("error");
    expect(parseUpdateCheck({ status: "update_available", current: "0.1.0", next: "0.2.0", notes: "ok", url: "https://evil.invalid" }).state).toBe("error");
    expect(parseUpdateCheck({ status: "install_now" }).state).toBe("error");
  });

  it("fails closed on malformed versions and oversized notes", () => {
    expect(parseUpdateCheck({ status: "up_to_date", current: "<script>" }).state).toBe("error");
    expect(parseUpdateCheck({ status: "update_available", current: "1.0.0", next: "2.0.0", notes: "x".repeat(2_001) }).state).toBe("error");
  });

  it("opens the source repository through one fixed argument-free native command", async () => {
    mocks.invoke.mockResolvedValueOnce(undefined);
    await expect(openHubSourceRepository()).resolves.toBe(true);
    expect(mocks.invoke).toHaveBeenCalledWith("open_hub_source_repository");
    expect(mocks.invoke.mock.calls[0]).toHaveLength(1);
  });
});
