import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  boundedProtectedDraft,
  overlayExpiryDelayMs,
  parseNativeDiscordOverlayAcknowledgment,
  parseNativeDiscordOverlayOpened,
  parseNativeDiscordOverlayOpenedBatch,
  parseNativeDiscordOverlayPrepared,
  parseNativeDiscordOverlayState,
  utf8Length,
} from "./overlay-state";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

describe("trusted composer overlay", () => {
  it("preserves exact Unicode and multiline drafts without a visible hard cap", () => {
    const draft = `🙂 first\n\nsecond\n${"x".repeat(48_000)}`;
    expect(boundedProtectedDraft(draft)).toBe(draft);
    expect(utf8Length(boundedProtectedDraft(draft))).toBe(utf8Length(draft));
  });

  it("has a separate local capability with only three narrow commands", () => {
    const composer = JSON.parse(readRelative("../../osl-hub/capabilities/composer-overlay.json")) as {
      local: boolean;
      webviews: string[];
      permissions: unknown[];
    };
    const overlay = JSON.parse(readRelative("../../osl-hub/capabilities/native-discord-overlay.json")) as {
      local: boolean;
      webviews: string[];
      permissions: unknown[];
      remote?: unknown;
      windows?: unknown;
    };
    const hub = JSON.parse(readRelative("../../osl-hub/capabilities/hub.json")) as {
      local: boolean;
      webviews: string[];
      permissions: string[];
      remote?: unknown;
    };
    expect(composer.local).toBe(true);
    expect(composer.webviews).toEqual(["composer-overlay"]);
    expect(composer.permissions).toEqual([]);
    expect(overlay.local).toBe(true);
    expect(overlay.webviews).toEqual(["native-discord-overlay"]);
    expect(overlay.permissions).toEqual([
      "allow-get-native-discord-overlay-state",
      "allow-prepare-native-discord-overlay-text",
      "allow-send-native-discord-overlay-carrier",
      "allow-open-native-discord-overlay-text",
      "allow-reveal-native-discord-overlay-view-once",
      "allow-select-native-discord-overlay-attachment",
      "allow-list-native-discord-overlay-attachments",
      "allow-open-native-discord-overlay-attachment",
      "allow-burn-native-discord-overlay-chat",
      "allow-set-native-discord-overlay-security",
    ]);
    expect(overlay).not.toHaveProperty("remote");
    expect(overlay).not.toHaveProperty("windows");
    expect(hub.local).toBe(true);
    expect(hub.webviews).toEqual(["main"]);
    expect(hub.permissions).toEqual(expect.arrayContaining([
      "allow-activate-native-manual-peer-context",
      "allow-set-native-discord-protected-overlay-open",
    ]));
    expect(hub).not.toHaveProperty("remote");
    expect(overlay.permissions).not.toEqual(expect.arrayContaining([
      "allow-prepare-encrypted-text",
      "allow-decrypt-hub-capsule",
    ]));
  });

  it("ships as a dedicated local entry with no networking, storage, events, or context token", () => {
    const vite = readRelative("../vite.config.ts");
    const source = readRelative("./overlay.ts");
    const adapter = readRelative("./native-overlay-adapter.ts");
    const native = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    expect(vite).toContain('overlay: fileURLToPath(new URL("./overlay.html"');
    const invoked = [...adapter.matchAll(/invoke<unknown>\("([^"]+)"/g)].map((match) => match[1]);
    expect(invoked).toEqual([
      "get_native_discord_overlay_state",
      "prepare_native_discord_overlay_text",
      "send_native_discord_overlay_carrier",
      "open_native_discord_overlay_text",
      "reveal_native_discord_overlay_view_once",
      "burn_native_discord_overlay_chat",
      "set_native_discord_overlay_security",
      "select_native_discord_overlay_attachment",
      "list_native_discord_overlay_attachments",
      "open_native_discord_overlay_attachment",
      "select_osl_chat_attachment",
      "list_osl_chat_attachments",
      "open_osl_chat_attachment",
    ]);
    expect(source).not.toMatch(/\binvoke\s*\(/);
    expect(source).not.toMatch(/\bfetch\s*\(/);
    expect(source).not.toMatch(/localStorage|sessionStorage|indexedDB/);
    expect(`${source}\n${adapter}`).not.toMatch(/contextToken|BroadcastChannel|\blisten\s*\(/);
    expect(native).toContain('pub(crate) const OVERLAY_LABEL: &str = "native-discord-overlay"');
    expect(native).toContain("WebviewUrl::App(PathBuf::from(OVERLAY_ASSET))");
    expect(native).toContain(".transparent(true)");
    expect(native).toContain(".set_position(");
    expect(native).toContain(".set_size(");
    expect(native).toContain("window.show()");
    expect(native).toContain("OverlayPhase::Guarding");
    expect(native).toContain("overlay_state.mark_ready(epoch, &host)?");
    expect(source).toContain("window.setTimeout(() => void initializeOverlay(), overlayInitRetryMs)");
    expect(source).toContain("Math.min(overlayInitRetryMs * 2, 1_000)");
  });

  it("uses a bounded protected transcript and composer with configuration controls removed from view", () => {
    const html = readRelative("../overlay.html");
    const css = readRelative("./overlay.css");
    expect(html).toContain('class="trust-mark"');
    expect(html).toContain("This composer belongs to OSL, not Discord");
    expect(html).toContain('id="protected-view-once"');
    expect(html).toContain('id="protected-ttl"');
    expect(html).toContain('id="protected-decrypt-display"');
    expect(html).toContain('id="current-expiry"');
    expect(html).toContain("Single Enter · Experimental");
    expect(html).toContain('id="osl-message-list"');
    expect(html).toContain('id="burn-protected-chat"');
    expect(html).toContain(">Burn</button>");
    expect(html).toContain('id="covertext-mode"');
    expect(html).toContain(">Covertext</button>");
    expect(html).not.toContain('id="ai-covertext-mode"');
    expect(html).toContain('class="overlay-runtime-controls" hidden');
    expect(html).toContain("Discord stays separate");
    expect(html).not.toContain("OSL checks visible accessibility labels, bounds, and an empty composer");
    expect(html).toContain("local OSL decoy line");
    expect(html).toContain("no cloud AI is used");
    expect(html).not.toMatch(/Discord (?:Sent|Delivered|Read)/i);
    expect(css).toContain("a central protected transcript and bottom composer");
    expect(css).toContain("grid-template-rows: minmax(0, 1fr) auto");
    expect(css).toContain("border: 1px solid rgba(73, 214, 255, .42)");
    expect(css).toContain("background: #313338");
    expect(css).not.toContain("#osl-message-list li:last-child");
  });

  it("keeps Covertext optional without putting AI controls in the composer", () => {
    const source = readRelative("./overlay.ts");
    const html = readRelative("../overlay.html");
    expect(source).toContain("let coverTextEnabled = true");
    expect(source).toContain("discordMarkerAvailable && coverTextEnabled");
    expect(source).toContain("Covertext off · private messages travel through OSL only.");
    expect(source).not.toContain('requireElement<HTMLButtonElement>("#ai-covertext-mode")');
    expect(html).not.toContain('id="ai-covertext-mode"');
  });

  it("strictly parses state and preserves multiline plaintext receipts", () => {
    const state = { active: true, friendLabel: "Test friend", scopeApproved: true, ttlSeconds: 3_600, decryptDisplayEnabled: true, viewOnceEnabled: true, attachmentsEnabled: true, discordMarkerAvailable: false, covertextEnabled: true };
    const prepared = { messageId: "msg-0123456789abcdef", expiresAt: 1_787_000_000, personToPersonE2ee: true, viewOnce: true, deliveredToOslInbox: true };
    const opened = { plaintext: "first\n\nthird", contextVerified: true, personToPersonE2ee: true, viewOnceConsumed: true, expiresAt: 1_787_000_000 };
    const acknowledgment = { messageId: prepared.messageId, status: "opened", acknowledgedAt: 1_786_999_900 };
    expect(parseNativeDiscordOverlayState(state)).toEqual(state);
    expect(parseNativeDiscordOverlayPrepared(prepared)).toEqual(prepared);
    expect(parseNativeDiscordOverlayOpened(opened)).toEqual(opened);
    expect(parseNativeDiscordOverlayAcknowledgment(acknowledgment)).toEqual(acknowledgment);
    const pendingViewOnce = { messageId: "peer-0123456789abcdef0123456789abcdef", expiresAt: prepared.expiresAt, personToPersonE2ee: true };
    expect(parseNativeDiscordOverlayOpenedBatch({ messages: [opened], pendingViewOnce: [pendingViewOnce], acknowledgments: [acknowledgment], fetched: 2 })).toEqual({ messages: [opened], pendingViewOnce: [pendingViewOnce], acknowledgments: [acknowledgment], fetched: 2 });
    expect(parseNativeDiscordOverlayState({ ...state, scopeApproved: false })).toBeNull();
    const { discordMarkerAvailable: _marker, ...stateWithoutMarkerAvailability } = state;
    expect(parseNativeDiscordOverlayState(stateWithoutMarkerAvailability)).toBeNull();
    expect(parseNativeDiscordOverlayState({ ...state, attachmentsEnabled: false })?.attachmentsEnabled).toBe(false);
    expect(parseNativeDiscordOverlayState({ ...state, discordMarkerAvailable: true })?.discordMarkerAvailable).toBe(true);
    expect(parseNativeDiscordOverlayPrepared({ ...prepared, deliveredToOslInbox: false })).toBeNull();
    expect(parseNativeDiscordOverlayOpened({ ...opened, plaintext: "🙂".repeat(262_145) })).toBeNull();
    expect(parseNativeDiscordOverlayOpened({ ...opened, expiresAt: 0 })).toBeNull();
    expect(parseNativeDiscordOverlayAcknowledgment({ ...acknowledgment, status: "read" })).toBeNull();
    expect(parseNativeDiscordOverlayOpenedBatch({ messages: Array.from({ length: 65 }, () => opened), pendingViewOnce: [], acknowledgments: [], fetched: 64 })).toBeNull();
    expect(parseNativeDiscordOverlayOpenedBatch({ messages: [], pendingViewOnce: [{ ...pendingViewOnce, messageId: "../message" }], acknowledgments: [], fetched: 1 })).toBeNull();
  });

  it("keeps relay-only delivery independent of unavailable Discord marker placement", () => {
    const source = readRelative("./overlay.ts");
    expect(source).toContain("placementMode.disabled = sendBusy || !overlayReady || !discordMarkerAvailable");
    expect(source).toContain("if (discordMarkerAvailable && coverTextEnabled) {");
    expect(source).toContain("await sendNativeDiscordOverlayCarrier(requestedPlacement, charsPerSecond, measuredCarrierLayout())");
    expect(source).toContain('padding: "shapeMatched"');
    expect(source).toContain("Sent privately through OSL only. No Discord marker was attempted.");
    expect(source).toContain("Ready for OSL-only messages. Discord marker placement is unavailable.");
    expect(source).toContain('markerSent ? " · Discord marked" : " · OSL only"');
  });

  it("bounds ephemeral plaintext lifetime without retaining it in browser storage", () => {
    const source = readRelative("./overlay.ts");
    expect(overlayExpiryDelayMs(1_700_000_010, 1_700_000_000_000)).toBe(10_000);
    expect(overlayExpiryDelayMs(1_700_000_000, 1_700_000_001_000)).toBe(0);
    expect(overlayExpiryDelayMs(1_800_000_000, 1_700_000_000_000)).toBe(604_800_000);
    expect(source).toContain("removeViewOnceBubbles();");
    expect(source).toContain('window.addEventListener("blur", removeViewOnceBubbles)');
    expect(source).toContain('item.textContent = ""');
    expect(source).not.toMatch(/localStorage|sessionStorage|indexedDB/);
  });

  it("hides and reveals still-live received plaintext without reopening it", () => {
    const source = readRelative("./overlay.ts");
    const append = source.slice(source.indexOf("function appendBubble"), source.indexOf("function applyAcknowledgment"));
    const visibility = source.slice(source.indexOf("function applyDecryptDisplayVisibility"), source.indexOf("function clearMessageBubbles"));
    const save = source.slice(source.indexOf("async function saveSecurity"), source.indexOf('ttl.addEventListener("change"'));

    expect(append).toContain('direction === "incoming" && !viewOnceMessage');
    expect(append).toContain("receivedPlaintextBubbles.add(item)");
    expect(append).toContain("body.hidden = !decryptDisplayEnabled");
    expect(visibility).toContain("body.hidden = !visible");
    expect(visibility).not.toContain('body.textContent = ""');
    expect(save).toContain("applyDecryptDisplayVisibility(decryptDisplayEnabled)");
    expect(save).toContain("applyDecryptDisplayVisibility(false)");
    expect(save.indexOf("applyDecryptDisplayVisibility(false)")).toBeLessThan(
      save.indexOf("await setNativeDiscordOverlaySecurity"),
    );
    expect(save).toContain("applyDecryptDisplayVisibility(previousDecrypt)");
    expect(save.indexOf("applyDecryptDisplayVisibility(decryptDisplayEnabled)")).toBeLessThan(
      save.indexOf("if (decryptDisplayEnabled) scheduleReceivePoll(0)"),
    );
    expect(save).not.toContain("clearMessageBubbles()");
  });

  it("destroys view-once plaintext on display-off while preserving destructive cleanup", () => {
    const source = readRelative("./overlay.ts");
    const visibility = source.slice(source.indexOf("function applyDecryptDisplayVisibility"), source.indexOf("function clearMessageBubbles"));
    const reveal = source.slice(source.indexOf("function appendPendingViewOnce"), source.indexOf("function scheduleReceivePoll"));
    const burn = source.slice(source.indexOf('burnChat.addEventListener("click"'), source.indexOf("function clearGestureTimer"));

    expect(visibility).toContain("if (!visible) removeViewOnceBubbles()");
    expect(reveal).toContain("reveal.disabled = !decryptDisplayEnabled");
    expect(reveal).toContain("if (receiveBusy || !decryptDisplayEnabled) return");
    expect(source).toContain('window.addEventListener("blur", removeViewOnceBubbles)');
    expect(source).toContain('window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(expiresAt, Date.now()))');
    expect(burn).toContain("clearMessageBubbles()");
    expect(source).toContain('item.textContent = ""');
  });

  it("does not fetch received plaintext while display is off", () => {
    const source = readRelative("./overlay.ts");
    const schedule = source.slice(source.indexOf("function scheduleReceivePoll"), source.indexOf("async function pollReceived"));
    const poll = source.slice(source.indexOf("async function pollReceived"), source.indexOf('document.addEventListener("visibilitychange"'));
    expect(schedule).toContain("!decryptDisplayEnabled");
    expect(poll).toContain("!decryptDisplayEnabled");
    expect(source).not.toMatch(/localStorage|sessionStorage|indexedDB/);
  });

  it("keeps view-once text pending until an explicit reveal gesture", () => {
    const source = readRelative("./overlay.ts");
    const adapter = readRelative("./native-overlay-adapter.ts");
    expect(source).toContain('body.textContent = "View-once message"');
    expect(source).toContain('reveal.textContent = "Reveal once"');
    expect(source).toContain("await revealNativeDiscordOverlayViewOnce(message.messageId)");
    expect(source).toContain("for (const message of batch.pendingViewOnce) appendPendingViewOnce(message)");
    expect(adapter).toContain('invoke<unknown>("reveal_native_discord_overlay_view_once", { messageId })');
  });

  it("burns only the Rust-held OSL scope and never claims Discord or recipient deletion", () => {
    const adapter = readRelative("./native-overlay-adapter.ts");
    const native = readRelative("../../osl-hub/src/main.rs");
    const permission = readRelative("../../osl-hub/permissions/hub.toml");
    expect(adapter).toContain('invoke<unknown>("burn_native_discord_overlay_chat")');
    expect(adapter).not.toMatch(/contextToken|personId|accountId/);
    expect(native).toContain("burn_manual_peer_scope");
    expect(native).toContain("burn_local_protected_context");
    expect(native).toContain("discord_history_deleted: false");
    expect(native).toContain("recipient_copies_deleted: false");
    expect(permission).toContain('identifier = "allow-burn-native-discord-overlay-chat"');
    expect(permission).toContain("It cannot touch Discord history, profiles, logins, or recipient copies.");
  });

  it("keeps attachments unusable unless native state confirms Pro", () => {
    const source = readRelative("./overlay.ts");
    const nativeMain = readRelative("../../osl-hub/src/main.rs");
    expect(source).toContain("chooseAttachment.hidden = !attachmentsEnabled");
    expect(source).toContain("attachmentBusy || !overlayReady || !attachmentsEnabled");
    expect(nativeMain).toContain("ipc::tier_gate::is_paid_equivalent(&core.osl)");
    expect(nativeMain).toContain("require_active_pro_entitlement(&app.state::<HubCoreState>())?");
  });
});
