import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

function installGlobals(): void {
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn() });
  vi.stubGlobal("document", {
    querySelector: vi.fn(() => null),
    createElement: vi.fn(() => ({})),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
  });
}

describe("attended IMAP run UI", () => {
  it("main.ts attended-run UI wiring (live-path check, dry-run preview, fina", async () => {
    installGlobals();
    const { runAttendedImapUi } = await import("./main");
    const nativeInvoke = vi.fn();

    await expect(runAttendedImapUi({
      accountId: "account-1",
      selectedMessageIds: ["msg-1"],
      livePathProved: false,
      confirmFinalRun: true,
    }, nativeInvoke)).resolves.toEqual({ state: "refused", reason: "live-path-required" });
    expect(nativeInvoke).not.toHaveBeenCalled();

    nativeInvoke.mockResolvedValueOnce({ previewToken: "preview-token-0001", selectedMessageIds: ["msg-1"] });
    await expect(runAttendedImapUi({
      accountId: "account-1",
      selectedMessageIds: ["msg-1"],
      livePathProved: true,
      confirmFinalRun: false,
    }, nativeInvoke)).resolves.toMatchObject({ state: "preview", previewToken: "preview-token-0001" });
    expect(nativeInvoke).toHaveBeenCalledWith("preview_attended_imap_batch_review", {
      request: { accountId: "account-1", selectedMessageIds: ["msg-1"], dryRun: true },
    });

    nativeInvoke
      .mockResolvedValueOnce({ previewToken: "preview-token-0002", selectedMessageIds: ["msg-2"] })
      .mockResolvedValueOnce({ previewToken: "preview-token-0002", runId: "run-0002", receiptCount: 1 });
    await expect(runAttendedImapUi({
      accountId: "account-1",
      selectedMessageIds: ["msg-2"],
      livePathProved: true,
      confirmFinalRun: true,
    }, nativeInvoke)).resolves.toEqual({
      state: "completed",
      previewToken: "preview-token-0002",
      runId: "run-0002",
      receiptCount: 1,
    });
  });
});
