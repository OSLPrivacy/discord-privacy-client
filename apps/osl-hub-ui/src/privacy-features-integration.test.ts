import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const ui = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const view = readFileSync(new URL("./osl-chats-view.ts", import.meta.url), "utf8");
const native = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const gate = readFileSync(new URL("../../osl-hub/src/startup_gate.rs", import.meta.url), "utf8");

describe("privacy feature integration pins", () => {
  it("persists all five controls per active identity and mirrors them in Settings", () => {
    expect(ui).toContain('const privacyFeaturesStorageKey = "osl-privacy-features-v1"');
    expect(ui).toContain("activeOwnerStorageKey(privacyFeaturesStorageKey)");
    for (const id of ["disable-previews", "ip-grabber-protection", "external-default-browser", "auto-lock", "clear-clipboard"]) {
      expect(ui).toContain(`privacyToggle("${id}"`);
    }
  });

  it("suppresses chat and compose previews before markup is produced", () => {
    expect(view).toContain("if (disableLinkPreviews)");
    expect(view).toContain('class="osl-chat-external-link"');
    expect(view).toContain("linkSurface(message.body, disableLinkPreviews)");
    expect(view).toContain("linkSurface(model.draft, model.disableLinkPreviews === true, true)");
  });

  it("checks the denylist at click time before the native browser command", () => {
    const guard = ui.slice(ui.indexOf("async function openCheckedExternalLink"), ui.indexOf("function scheduleCopiedMessageClear"));
    expect(guard).toContain('setupPrivacyChoices.has("ip-grabber-protection")');
    expect(guard).toContain("checkExternalLink");
    expect(guard.indexOf("checkExternalLink")).toBeLessThan(guard.indexOf("openExternalLinkInDefaultBrowser"));
    expect(native).toContain('matches!(parsed.scheme(), "http" | "https")');
    expect(native).toContain("parsed.username().is_empty()");
  });

  it("locks native account state and clears the live renderer plaintext state", () => {
    expect(ui).toContain("Date.now() - lastTrustedActivityAt >= autoLockIdleMilliseconds");
    expect(ui).toContain("clearUnlockedRendererState()");
    expect(gate).toContain("ipc::main_password::set_file_storage_key(None)");
    expect(native).toContain("startup_gate::lock_session(&app.state::<HubCoreState>())");
  });

  it("clears only an unchanged protected clipboard", () => {
    expect(ui).toContain("scheduleProtectedClipboardClear(protectedClipboardClearSeconds)");
    expect(native).toContain("GetClipboardSequenceNumber");
    expect(native).toContain("!= expected_sequence");
    expect(native).toContain("EmptyClipboard()");
  });
});
