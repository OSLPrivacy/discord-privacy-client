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

describe("autoscrub attended IMAP authorization", () => {
  it("Wire review-list confirmation to arm authorize_attended_imap_batch_rev", async () => {
    installGlobals();
    const { authorizeAutoscrubReviewList } = await import("./main");
    const nativeInvoke = vi.fn();

    await expect(authorizeAutoscrubReviewList({
      accountId: "account-1",
      selectedMessageIds: ["msg-1"],
      operatorConfirmed: false,
    }, nativeInvoke)).resolves.toEqual({ state: "refused", reason: "confirmation-required" });
    expect(nativeInvoke).not.toHaveBeenCalled();

    nativeInvoke.mockResolvedValueOnce({ authorizationId: "auth-token-00000001" });
    await expect(authorizeAutoscrubReviewList({
      accountId: "account-1",
      selectedMessageIds: ["msg-1", "msg-2"],
      operatorConfirmed: true,
    }, nativeInvoke)).resolves.toEqual({
      state: "armed",
      authorizationId: "auth-token-00000001",
      selectedMessageIds: ["msg-1", "msg-2"],
    });
    expect(nativeInvoke).toHaveBeenCalledWith("authorize_attended_imap_batch_review", {
      request: { accountId: "account-1", selectedMessageIds: ["msg-1", "msg-2"] },
    });

    nativeInvoke.mockClear();
    await expect(authorizeAutoscrubReviewList({
      accountId: "account-1",
      selectedMessageIds: ["msg-1", "msg-1"],
      operatorConfirmed: true,
    }, nativeInvoke)).resolves.toEqual({ state: "refused", reason: "invalid-selection" });
    expect(nativeInvoke).not.toHaveBeenCalled();
  });
});
