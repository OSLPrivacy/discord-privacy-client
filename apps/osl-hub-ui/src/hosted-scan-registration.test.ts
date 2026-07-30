import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  openHostedSessionScan,
  parseHostedSessionScan,
  requestHostedSessionScanCommand,
} from "./hosted-session-scan";

const scopeBindingHash = "a".repeat(64);

function scan() {
  return {
    scopeBindingHash,
    generation: 7,
    rowsSeen: 3,
    rowsUnreadable: 1,
    walk: "complete",
    candidates: [{
      scanOrdinal: 1,
      shapeOrdinal: 0,
      shape: { heightPx: 42, children: 5 },
      textLen: 12,
      authoredByOperator: true,
    }],
  };
}

describe("hosted-session scan registration", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("openHostedSessionScan uses only the scan surface command with no renderer authority", async () => {
    mocks.invoke.mockResolvedValueOnce(undefined);
    await expect(openHostedSessionScan()).resolves.toBeUndefined();
    expect(mocks.invoke).toHaveBeenCalledWith("open_hosted_session_scan");
  });

  it("requestHostedSessionScanCommand returns content-free row shape metadata", async () => {
    mocks.invoke.mockResolvedValueOnce(scan());
    await expect(requestHostedSessionScanCommand()).resolves.toEqual(scan());
    expect(mocks.invoke).toHaveBeenCalledWith("request_hosted_session_scan");
  });

  it("requestHostedSessionScanCommand rejects content-bearing or widened receipts", async () => {
    expect(() => parseHostedSessionScan({ ...scan(), messageText: "delete this" })).toThrow(
      "invalid hosted session scan response",
    );
    expect(() => parseHostedSessionScan({ ...scan(), accountId: "acct-rose" })).toThrow(
      "invalid hosted session scan response",
    );
    expect(() => parseHostedSessionScan({
      ...scan(),
      candidates: [{ ...scan().candidates[0], locator: "runtime:message:7" }],
    })).toThrow("invalid hosted session scan response");
    expect(() => parseHostedSessionScan({
      ...scan(),
      candidates: [{ ...scan().candidates[0], shape: { heightPx: 42 } }],
    })).toThrow("invalid hosted session scan response");
    expect(() => parseHostedSessionScan({ ...scan(), scopeBindingHash: "not-a-hash" })).toThrow(
      "invalid hosted session scan response",
    );
  });

  it("refuses when the native command is unavailable instead of granting a local fallback", async () => {
    mocks.isTauriRuntime.mockReturnValue(false);
    await expect(openHostedSessionScan()).rejects.toThrow("hosted session scan unavailable");
    await expect(requestHostedSessionScanCommand()).rejects.toThrow("hosted session scan unavailable");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
