import { describe, expect, it } from "vitest";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const overlay = fs.readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");
const styles = fs.readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const native = fs.readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const nativeOverlay = fs.readFileSync(new URL("../../osl-hub/src/native_discord_overlay.rs", import.meta.url), "utf8");
const hubCapability = fs.readFileSync(new URL("../../osl-hub/capabilities/hub.json", import.meta.url), "utf8");

function body(startNeedle: string, endNeedle: string): string {
  const start = source.indexOf(startNeedle);
  const end = source.indexOf(endNeedle, start + startNeedle.length);
  expect(start).toBeGreaterThan(-1);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("Discord QA header strip", () => {
  it("keeps the production header branch unchanged and isolates the QA strip", () => {
    const controls = body(
      "function nativeDiscordHeaderControls()",
      "function trustedHeader()",
    );
    const production = controls.slice(
      controls.indexOf("if (!discordQaShell)"),
      controls.indexOf("const context ="),
    );
    expect(production).toContain('data-open-burn="chat"');
    expect(production).toContain('id="native-discord-covertext"');
    expect(production).toContain('id="native-discord-ai-covertext"');
    expect(production).not.toContain("discord-qa-control");
    expect(controls).toContain('data-open-burn="account"');
    expect(controls).toContain('data-open-burn="app"');
    expect(controls).toContain('aria-label="Account Burn"');
    expect(controls).toContain('aria-label="Discord Burn"');
    expect(controls).toContain('aria-label="Chat Burn"');
    expect(controls).not.toContain("<span>Account Burn</span>");
    expect(controls).not.toContain("<span>Discord Burn</span>");
    expect(controls).not.toContain("<span>Chat Burn</span>");
    expect(controls).toContain('id="discord-qa-whitelist-add"');
    expect(controls).toContain('id="discord-qa-whitelist-remove"');
    expect(controls).toContain('id="discord-qa-transcript-visibility"');
    expect(controls).toContain('id="discord-qa-toggle-composer"');
    expect(styles).toContain(
      ".discord-qa-icon-control.composer.unlocked { color: #ff626e;",
    );
    expect(styles).toContain(
      ".discord-qa-icon-control.composer.locked { color: #5b8cff;",
    );
    expect(styles).toContain("left: 50%");
    expect(styles).toContain("transform: translate(-50%, -50%)");
  });

  it("keeps the protected surface frameless and tethered to the borrowed app", () => {
    expect(nativeOverlay).toContain(".decorations(false)");
    expect(nativeOverlay).toContain(".set_decorations(false)");
    expect(nativeOverlay).toContain(".set_shadow(false)");
    expect(nativeOverlay).toContain(
      "super::window_border::suppress_accent_border(window.as_ref())",
    );
    expect(nativeOverlay).toContain(".parent(&main)");
    expect(nativeOverlay).toContain("start_guard(");
  });

  it("routes destructive controls through the existing confirmation dialog", () => {
    expect(source).toContain('data-open-burn="account"');
    expect(source).toContain('data-open-burn="app"');
    expect(source).toContain('data-open-burn="chat"');
    expect(source).toContain('burnDialogOpen = true');
    expect(source).toContain('input.value !== burnConfirmationPhrase(burnScope)');
  });

  it("changes only an exact verified peer scope and fails closed", () => {
    const permission = body(
      "async function setDiscordQaWhitelistPermission",
      "async function toggleDiscordQaTranscriptVisibility",
    );
    expect(permission).toContain("activeVerifiedDiscordQaPeer()");
    expect(permission).toContain("active.context.contextToken");
    expect(permission).toContain("active.person.personId");
    expect(permission).toContain("enabled,\n    false,");
    expect(permission).toContain("Whitelist change failed closed");
  });

  it("keeps transcript visibility independent from composer lock state", () => {
    const controls = body(
      "function nativeDiscordHeaderControls()",
      "function trustedHeader()",
    );
    const visibility = body(
      "async function toggleDiscordQaTranscriptVisibility",
      "async function toggleDiscordQaComposer",
    );
    expect(controls).toContain('${!verifiedPeer || visibilityBusy ? "disabled" : ""}');
    expect(controls).not.toContain('${!nativeDiscordProtectionActive || !verifiedPeer || visibilityBusy ? "disabled" : ""}');
    expect(visibility).toContain("saveActiveContextSecurity(");
    expect(visibility).toContain("VITE_OSL_DISCORD_QA_SHELL");
    expect(visibility).toContain("peerProtectedSheet.decryptDisplayEnabled = requested");
    expect(visibility).toContain("peerProtectedSheet.decryptDisplayEnabled = previous");
    expect(visibility).toContain("await saveActiveContextSecurity(");
    // Display is independent of encryption: the eye turns on the display
    // surface's own presence, never on the lock.
    expect(visibility).toContain("&& transcriptSurfaceLive");
    expect(visibility).toContain("const transcriptSurfaceLive = nativeDiscordOverlaySurfacePresent");
    expect(visibility).toContain('emitTo(');
    expect(visibility).toContain('"native-discord-overlay"');
    expect(visibility).toContain("PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT");
    expect(visibility).toContain(
      "PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT,\n    ).then",
    );
    expect(visibility).not.toContain("setNativeDiscordProtectedOverlayOpenForQa(");
    expect(visibility).not.toContain("setNativeDiscordProtectedOverlayOpen(active.context.contextToken, false)");
    expect(visibility).not.toContain("resetLocalProtectedSheet(false)");
    expect(native).toContain('if caller.label() != "main"');
    expect(overlay).toContain("if (discordQaShell)");
    expect(overlay).toContain('window.addEventListener("focus"');
    expect(overlay).toContain("listen<boolean>(PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT");
    expect(overlay).toContain("applyDecryptDisplayVisibility(payload)");
    expect(overlay).toContain("void refreshProtectedDisplayVisibility()");
    expect(overlay).toContain("const state = await getNativeDiscordOverlayState()");
    expect(overlay).toContain("applyDecryptDisplayVisibility(decryptDisplayEnabled)");
    expect(source).toContain("listen<void>(NATIVE_DISCORD_OVERLAY_CLOSED_EVENT");
    expect(source).toContain("nativeDiscordProtectionActive = false");
    expect(nativeOverlay).toContain('app.emit_to("main", OVERLAY_CLOSED_EVENT, ())');
    expect(hubCapability).toContain('"core:event:allow-listen"');
  });

  it("opens and closes the actual native protected composer", () => {
    const toggle = body(
      "async function toggleDiscordQaComposer",
      "async function openSoleVerifiedDiscordQaOverlay",
    );
    expect(toggle).toContain("await toggleLocalProtectedSheet()");
    expect(toggle).toContain("await openDiscordQaComposer()");
    const close = body(
      "async function toggleLocalProtectedSheet",
      "async function openNativeDiscordProtection",
    );
    expect(close).toContain("if (protectedSheetCloseBusy) return");
    expect(close).toContain("protectedSheetCloseBusy = true");
    expect(close).toContain("finally {\n        protectedSheetCloseBusy = false;");
    expect(source).toContain("nativeDiscordProtectionActive = false");
  });
});
