import { describe, expect, it, vi } from "vitest";
import {
  browserServiceQaIds,
  createBrowserServiceQaShell,
  type BrowserServiceQaPort,
} from "./browser-service-qa-shell";
import type { BrowserCompanionAction, BrowserCompanionStatus } from "./services";

const firefoxStatus: BrowserCompanionStatus = {
  status: "available",
  browserId: "firefox",
  displayName: "Firefox",
  reason: "none",
  captureProtected: false,
  containment: "bestEffort",
};

function action(status: "hosted" | "focused" | "resized" | "detached"): BrowserCompanionAction {
  return {
    status,
    browserId: "firefox",
    reason: "none",
    mode: "existingBrowserCompanion",
    captureProtected: false,
    containment: "bestEffort",
  };
}

function port(overrides: Partial<BrowserServiceQaPort> = {}): BrowserServiceQaPort {
  return {
    inspectDefaultBrowser: vi.fn().mockResolvedValue(firefoxStatus),
    hostExistingFirefoxSession: vi.fn().mockResolvedValue(action("hosted")),
    focus: vi.fn().mockResolvedValue(action("focused")),
    resize: vi.fn().mockResolvedValue(action("resized")),
    detach: vi.fn().mockResolvedValue(action("detached")),
    ...overrides,
  };
}

describe("browser service QA shell", () => {
  it("hosts every allowed browser service in the existing default Firefox session", async () => {
    for (const serviceId of browserServiceQaIds) {
      const adapter = port();
      const shell = createBrowserServiceQaShell(adapter);
      const state = await shell.open(serviceId);

      expect(adapter.inspectDefaultBrowser).toHaveBeenCalledOnce();
      expect(adapter.hostExistingFirefoxSession).toHaveBeenCalledWith(serviceId);
      expect(state).toMatchObject({
        phase: "hosted",
        serviceId,
        browserId: "firefox",
        sessionOwnership: "browser",
        credentialAccess: "none",
        captureProtected: false,
        containment: "bestEffort",
        error: null,
      });
    }
  });

  it("fails closed unless the trusted default browser is Firefox", async () => {
    const adapter = port({
      inspectDefaultBrowser: vi.fn().mockResolvedValue({
        ...firefoxStatus,
        browserId: "chrome",
        displayName: "Chrome",
      }),
    });
    const state = await createBrowserServiceQaShell(adapter).open("instagram");

    expect(state).toMatchObject({ phase: "failed", error: "defaultFirefoxUnavailable" });
    expect(adapter.hostExistingFirefoxSession).not.toHaveBeenCalled();
  });

  it("rejects native and unknown scopes before touching the browser boundary", async () => {
    const adapter = port();
    const shell = createBrowserServiceQaShell(adapter);

    const state = await shell.open("telegram" as never);

    expect(state).toMatchObject({ phase: "failed", serviceId: null, error: "unsupportedService" });
    expect(adapter.inspectDefaultBrowser).not.toHaveBeenCalled();
    expect(adapter.hostExistingFirefoxSession).not.toHaveBeenCalled();
  });

  it("orchestrates focus, resize, and detach without accepting automation input", async () => {
    const adapter = port();
    const shell = createBrowserServiceQaShell(adapter);
    await shell.open("gmail");

    expect((await shell.focus()).phase).toBe("hosted");
    expect((await shell.resize()).phase).toBe("hosted");
    expect(await shell.detach()).toMatchObject({
      phase: "idle",
      serviceId: null,
      browserId: null,
      credentialAccess: "none",
    });
    expect(adapter.focus).toHaveBeenCalledOnce();
    expect(adapter.resize).toHaveBeenCalledOnce();
    expect(adapter.detach).toHaveBeenCalledOnce();
  });

  it("rejects malformed or weaker host receipts", async () => {
    const adapter = port({
      hostExistingFirefoxSession: vi.fn().mockResolvedValue({
        ...action("hosted"),
        mode: "isolatedBrowserCompanion",
      }),
    });

    const state = await createBrowserServiceQaShell(adapter).open("x");

    expect(state).toMatchObject({ phase: "failed", browserId: null, error: "hostRejected" });
  });

  it("does not overlap browser operations and publishes immutable transitions", async () => {
    let resolveStatus: ((status: BrowserCompanionStatus) => void) | undefined;
    const adapter = port({
      inspectDefaultBrowser: vi.fn().mockImplementation(() => new Promise((resolve) => {
        resolveStatus = resolve;
      })),
    });
    const shell = createBrowserServiceQaShell(adapter);
    const transitions: string[] = [];
    shell.subscribe((state) => transitions.push(state.phase));

    const opening = shell.open("messenger");
    const overlapping = await shell.open("snapchat");
    expect(overlapping).toMatchObject({ phase: "checking", serviceId: "messenger", error: "operationInProgress" });

    resolveStatus?.(firefoxStatus);
    const hosted = await opening;
    expect(hosted.phase).toBe("hosted");
    expect(Object.isFrozen(hosted)).toBe(true);
    expect(transitions).toEqual(["checking", "opening", "hosted"]);
  });
});
