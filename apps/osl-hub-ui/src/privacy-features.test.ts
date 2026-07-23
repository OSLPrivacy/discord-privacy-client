import { describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: () => true }));

import {
  checkExternalLink,
  externalHttpUrls,
  isKnownLinkGrabberHostname,
  openExternalLinkInDefaultBrowser,
  scheduleProtectedClipboardClear,
} from "./privacy-features";

describe("privacy feature enforcement", () => {
  it("blocks exact and subdomain forms of the local link-grabber denylist", () => {
    expect(isKnownLinkGrabberHostname("grabify.link")).toBe(true);
    expect(isKnownLinkGrabberHostname("track.iplogger.org")).toBe(true);
    expect(isKnownLinkGrabberHostname("example.org")).toBe(false);
    expect(checkExternalLink("https://track.iplogger.org/a", true)).toEqual({
      allowed: false,
      reason: "knownLinkGrabber",
    });
  });

  it("accepts only bounded credential-free HTTP links", () => {
    expect(checkExternalLink("javascript:alert(1)", false).allowed).toBe(false);
    expect(checkExternalLink("https://user:secret@example.org/", false).allowed).toBe(false);
    expect(checkExternalLink("https://example.org/path", true)).toMatchObject({
      allowed: true,
      hostname: "example.org",
    });
  });

  it("extracts a bounded de-duplicated set without trailing punctuation", () => {
    const body = `${"https://example.org/a ".repeat(20)}https://other.example/test).`;
    expect(externalHttpUrls(body)).toEqual(["https://example.org/a", "https://other.example/test"]);
  });

  it("uses only the native default-browser command after local validation", async () => {
    invoke.mockResolvedValue(undefined);
    await expect(openExternalLinkInDefaultBrowser("https://example.org/")).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("open_external_link_in_default_browser", { url: "https://example.org/" });
    invoke.mockClear();
    await expect(openExternalLinkInDefaultBrowser("file:///secret")).resolves.toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("bounds clipboard-clear timeouts before invoking native code", async () => {
    invoke.mockResolvedValue(undefined);
    await expect(scheduleProtectedClipboardClear(30)).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("schedule_protected_clipboard_clear", { timeoutSeconds: 30 });
    invoke.mockClear();
    await expect(scheduleProtectedClipboardClear(1)).resolves.toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });
});
