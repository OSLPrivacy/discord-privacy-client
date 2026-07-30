import { readFileSync } from "node:fs";
import { Buffer } from "node:buffer";
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
  parseNativeSurfaceCapture,
  utf8Length,
} from "./overlay-state";
import { shouldPollDiscordOverlay } from "./discord-qa-receive-policy";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

function nativeSurfaceFixture() {
  const widthPx = 320;
  const heightPx = 24;
  const byteLength = 54 + widthPx * heightPx * 4;
  const bytes = new Uint8Array(byteLength);
  const view = new DataView(bytes.buffer);
  bytes[0] = 0x42;
  bytes[1] = 0x4d;
  view.setUint32(2, byteLength, true);
  view.setUint32(10, 54, true);
  view.setUint32(14, 40, true);
  view.setUint32(18, widthPx, true);
  view.setUint32(22, heightPx, true);
  view.setUint16(26, 1, true);
  view.setUint16(28, 32, true);
  view.setUint32(34, widthPx * heightPx * 4, true);
  return {
    version: "osl-native-surface-capture-v1",
    imageDataUrl: `data:image/bmp;base64,${Buffer.from(bytes).toString("base64")}`,
    widthPx,
    heightPx,
    inputLeftPx: 40,
    inputTopPx: 2,
    inputWidthPx: 240,
    inputHeightPx: 20,
    inputBackground: "#383a40",
    textLeftPx: 44,
    textTopPx: 4,
    textWidthPx: 236,
    textHeightPx: 16,
    fontFamily: "gg sans",
    fontSizePx: 16,
    fontWeight: 400,
    lineHeightPx: 16,
  };
}

describe("trusted composer overlay", () => {
  it("fully covers the native composer and mirrors its conversation placeholder", () => {
    const source = readRelative("./overlay.ts");
    const styles = readRelative("./overlay.css");
    const markup = readRelative("../overlay.html");
    expect(markup).toContain('placeholder="Message"');
    expect(source).toContain('draft.placeholder = state.nativeSurface ? " " : exactDiscordPlaceholder');
    expect(styles).toContain('.draft-field textarea:placeholder-shown');
    expect(source).toContain('sendMode.value = "single"');
    expect(source).toContain('sendGesture.setMode("single")');
    expect(source).toContain("event.isTrusted || discordQaShell");
    expect(styles).toContain(':root[data-discord-qa-shell="true"] .composer-box');
    expect(styles).toContain("position: absolute;");
    expect(styles).toContain("inset: 0;");
    expect(styles).toContain("max-width: none;");
    // Was `background: var(--osl-composer-bg)`. That variable held Discord's
    // DEFAULT dark-theme composer colour -- both as a literal in this sheet and
    // as a theme-pack constant re-derived by discordVisualCssVariables() -- so
    // on a near-black custom Discord theme this rule painted a visibly lighter
    // slab inside the real message box. The fill is now measured or absent.
    expect(styles).toContain("background: var(--osl-composer-fill);");
    expect(styles).toContain(':root[data-discord-qa-shell="true"] .send-action');
  });

  it("restores DOM draft focus when the native overlay reclaims its window", () => {
    const source = readRelative("./overlay.ts");
    expect(source).toContain('const OVERLAY_REFOCUS_EVENT = "osl://native-discord-overlay-refocus"');
    expect(source).toContain("draft.focus({ preventScroll: true })");
  });

  it("puts the caret in the protected draft as soon as a raised lock is readable", () => {
    // The defect this pins: the native guard gives this WINDOW input focus once,
    // on first open (`active_focus_overlay`), and nothing ever gave the trusted
    // textarea DOM focus. The composer was on screen, above Discord and
    // foreground, with `document.activeElement` still <body> -- so the operator
    // had to click before typing, over a rectangle that is also Discord's real
    // message box, where a missed click plus Enter sends plaintext.
    const source = readRelative("./overlay.ts");
    const native = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    // The window half of the handshake still belongs to the guard, and is still
    // there: this renderer only adds the caret.
    //
    // Now gated on `!band_surrendered` as well. This used to pin the bare
    // `if active_overlay_requires_focus_acquisition() {`, and that needle pinned
    // stale behaviour: with the composer band surrendered the keyboard belongs to
    // Discord's own message box, and taking the foreground there would type the
    // operator's next keystroke into a surface that is deliberately not a message
    // box at all. Both halves of the caret handshake are gated on the same fact --
    // this guard natively, and `caretGrantedForEngagement` in the renderer.
    expect(native).toContain("if active_overlay_requires_focus_acquisition() && !band_surrendered {");
    // Also stale: this pinned `if let Err(error) = active_focus_overlay(&window) {`
    // from when a refused foreground was fatal and tore the session down. The
    // outcome is now recorded and reported instead -- which is what raises the
    // composer-unreachable warning in the hub -- so the acquisition is a value, not
    // an early return.
    expect(native).toContain("let focus_acquired = active_focus_overlay(&window).is_ok();");
    expect(source).toContain("function focusEngagedProtectedDraft(): void {");
    // Three gates, all of them refusals, and the grant is recorded before the
    // focus so it can never run twice for one raise.
    expect(source).toMatch(
      /function focusEngagedProtectedDraft\(\): void \{\s*if \(!lockEngaged \|\| !overlayReady \|\| caretGrantedForEngagement\) return;\s*caretGrantedForEngagement = true;\s*draft\.focus\(\{ preventScroll: true \}\);/u,
    );
    // A recovered draft is text the operator already wrote: typing continues it.
    expect(source).toMatch(
      /const caret = draft\.value\.length;\s*try \{\s*draft\.setSelectionRange\(caret, caret\);/u,
    );
    // Granted from the readable-session path, after `overlayReady` is true and
    // after every control has been enabled -- never before there is something
    // to type into.
    expect(source).toMatch(
      /overlayReady = true;[\s\S]*?setBusy\(false\);[\s\S]*?focusEngagedProtectedDraft\(\);\s*\} catch \{/u,
    );
    // And from the lock state itself, which is the only signal that says the
    // operator asked for OSL to own the message box.
    expect(source).toMatch(
      /function applyLockEngaged\(engaged: boolean\): void \{[\s\S]*?if \(!engaged\) caretGrantedForEngagement = false;\s*else focusEngagedProtectedDraft\(\);/u,
    );
  });

  it("never moves focus except as a direct result of the operator raising the lock", () => {
    const source = readRelative("./overlay.ts");
    // DOM-only, by construction: nothing on this path may raise, show, move or
    // foreground a window, so it cannot yank an operator working elsewhere.
    const grantStart = source.indexOf("function focusEngagedProtectedDraft(): void {");
    expect(grantStart).toBeGreaterThan(-1);
    const grant = source.slice(grantStart, source.indexOf("\n}\n", grantStart));
    expect(grant).not.toMatch(/window\.focus\(|setFocus|show\(|setPosition|setAlwaysOnTop|invoke\(/u);
    // The caret is granted once per raise. Everything that re-announces an
    // already-raised lock -- a restored composer re-emitting the session event,
    // every re-measure edge re-applying the same lock state, the initialisation
    // backoff -- hits the `caretGrantedForEngagement` refusal above instead of
    // stealing focus from whatever control the operator is using.
    const cleared = [...source.matchAll(/(?<!let )caretGrantedForEngagement = false;/gu)];
    expect(cleared).toHaveLength(6);
    // Each clearing site is a state in which there is no raised, readable lock
    // to hold the caret: the lock came down, the session was discarded, the
    // session could not be verified at all, or the native window surrendered the
    // composer band so there is no composer on screen to hold one.
    expect(source).toMatch(/if \(!engaged\) caretGrantedForEngagement = false;/u);
    expect(source).toMatch(
      /oslComposerBandSurrendered = String\(payload\);[\s\S]{0,400}?if \(payload\) caretGrantedForEngagement = false;/u,
    );
    expect(source).toMatch(
      /function discardProtectedSession\(\): void \{[\s\S]*?caretGrantedForEngagement = false;/u,
    );
    expect(source).toMatch(
      /clearDecodedTranscript\(\);[\s\S]{0,1400}?caretGrantedForEngagement = false;\s*return;/u,
    );
    expect(source).toMatch(
      /status\.textContent = "Verifying protected Discord…";[\s\S]{0,200}?caretGrantedForEngagement = false;\s*scheduleOverlayInit\(\);/u,
    );
  });

  it("applies QA transcript visibility directly from the trusted header event", () => {
    const source = readRelative("./overlay.ts");
    const styles = readRelative("./overlay.css");
    const markup = readRelative("../overlay.html");
    const native = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    expect(markup).toContain('id="osl-message-list" class="transcript-mount"');
    expect(markup).toMatch(/id="osl-message-list"[^>]*\shidden/u);
    expect(source).toContain("listen<boolean>(PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT");
    expect(source).toContain("applyDecryptDisplayVisibility(payload)");
    // The eye is the only control over display, so it takes OSL's layer off
    // Discord entirely. That is safe now precisely because no opaque shield sits
    // behind the message list any more: the shield is tied to the eye and clipped
    // to the rows OSL paints, so with the eye off there is nothing behind this
    // layer but Discord itself.
    expect(source).toContain("messageList.hidden = !visible");
    expect(native).toContain("fn show_capture_shield(shield: &tauri::WebviewWindow, shielded: bool)");
    expect(native).toContain("let shielded = !painted_rows.is_empty();");
    expect(source).toMatch(
      /applyDecryptDisplayVisibility\(payload\);[\s\S]*?if \(payload\)[\s\S]*?scheduleReceivePoll\(0\)[\s\S]*?receiveTimer = undefined/u,
    );
    expect(styles).not.toMatch(
      /:root\[data-discord-qa-shell="true"\]\s+\.transcript-mount\s*\{\s*display:\s*none/u,
    );
    // Per-row in every build, not only in the QA shell. The QA-scoped copies of
    // these rules are gone: the shipping build painted one panel over the whole
    // message band because they were gated.
    expect(styles).toMatch(
      /\n\.transcript-mount\s*\{[^}]*background:\s*transparent;[^}]*pointer-events:\s*none;/su,
    );
    expect(styles).toMatch(
      /\n\.osl-discord-transcript__row--carrier-bound\s*\{[^}]*position:\s*absolute;[^}]*pointer-events:\s*auto;/su,
    );
    expect(styles).not.toContain(
      ':root[data-discord-qa-shell="true"] .osl-discord-transcript__row--carrier-bound',
    );
    expect(source).not.toContain("if (!discordQaShell) return;");
    expect(source).toContain("clearCarrierRowGeometry(item)");
    expect(source).toContain("applyCarrierRowGeometry(item, binding)");
    expect(styles).toContain("grid-template-rows: minmax(0, 1fr) auto");
    expect(styles).toContain(':root[data-discord-qa-shell="true"][data-native-composer-capture="true"] #write-pane');
    expect(styles).toContain("aspect-ratio: var(--osl-native-composer-aspect-ratio, auto)");
    expect(source).toContain("`${capture.widthPx} / ${capture.heightPx}`");
    expect(source).not.toContain('`${capture.heightPx}px`');
    // One shape for both builds; the QA fork of the geometry is gone.
    expect(native).not.toContain('#[cfg(feature = "discord-qa-shell")]\nfn active_overlay_rect_with_composer');
    expect(native).toContain("protected_overlay_rect(discord, composer, painted_top)");
  });

  it("keeps the eye's decrypted display untouched when the readable session drops out", () => {
    // The lock is encryption only. Losing a readable protected session -- because
    // the lock went down, or the backend has nothing left to paint -- is not
    // evidence the eye was turned off, and must never reset it. Only the eye
    // toggle itself may change decryptDisplayEnabled.
    const source = readRelative("./overlay.ts");
    const fnStart = source.indexOf("async function refreshProtectedDisplayVisibility(): Promise<void> {");
    expect(fnStart).toBeGreaterThan(-1);
    const guardStart = source.indexOf("if (!state || !state.active) {", fnStart);
    expect(guardStart).toBeGreaterThan(fnStart);
    const guardEnd = source.indexOf("return;", guardStart);
    expect(guardEnd).toBeGreaterThan(guardStart);
    const guardBody = source.slice(guardStart, guardEnd);
    expect(guardBody).not.toContain("decryptDisplayEnabled");
    expect(guardBody).toContain("applyNativeSurfaceCapture();");
    expect(guardBody).toContain("applyVerifiedCarrierRows([]);");
  });

  it("treats the eye as a stored per-scope policy that a discarded protected session does not reset", () => {
    // A lock toggle (or any other reason a session is discarded) must not silently
    // close the operator's decrypted display. The eye is restored from the exact
    // in-memory value it already held, never forced off.
    const source = readRelative("./overlay.ts");
    const fnStart = source.indexOf("function discardProtectedSession(): void {");
    expect(fnStart).toBeGreaterThan(-1);
    const fnEnd = source.indexOf("\n}\n", fnStart);
    expect(fnEnd).toBeGreaterThan(fnStart);
    const body = source.slice(fnStart, fnEnd);
    expect(body).not.toContain("decryptDisplayEnabled = false");
    expect(body).not.toContain("decryptDisplayEnabled = true");
    expect(body).toContain("applyDecryptDisplayVisibility(decryptDisplayEnabled);");
  });

  it("preserves exact Unicode and multiline drafts without a visible hard cap", () => {
    const draft = `🙂 first\n\nsecond\n${"x".repeat(48_000)}`;
    expect(boundedProtectedDraft(draft)).toBe(draft);
    expect(utf8Length(boundedProtectedDraft(draft))).toBe(utf8Length(draft));
  });

  it("has separate local capabilities with only the required narrow permissions", () => {
    const composer = JSON.parse(readRelative("../../osl-hub/capabilities/composer-overlay.json")) as {
      local: boolean;
      webviews: string[];
      permissions: unknown[];
    };
    const whatsapp = JSON.parse(readRelative("../../osl-hub/capabilities/whatsapp-composer-overlay.json")) as {
      local: boolean;
      webviews: string[];
      permissions: unknown[];
      remote?: unknown;
      windows?: unknown;
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
    expect(whatsapp.local).toBe(true);
    expect(whatsapp.webviews).toEqual(["whatsapp-composer-overlay"]);
    expect(whatsapp.permissions).toEqual(["allow-prepare-whatsapp-qa-protected-text"]);
    expect(whatsapp).not.toHaveProperty("remote");
    expect(whatsapp).not.toHaveProperty("windows");
    expect(overlay.local).toBe(true);
    expect(overlay.webviews).toEqual(["native-discord-overlay"]);
    expect(overlay.permissions).toEqual([
      "core:event:allow-listen",
      "allow-get-native-discord-overlay-state",
      "allow-prepare-native-discord-overlay-text",
      "allow-send-native-discord-qa-probe",
      "allow-send-native-discord-qa-atomic-text",
      "allow-record-native-discord-qa-send-stage",
      "allow-send-native-discord-overlay-carrier",
      "allow-open-native-discord-overlay-text",
      // The eye's own bounded transcript read. Its absence here is what kept the
      // feature dark: Tauri refused every invoke at the ACL boundary before the
      // command body ran, so the command could not even report that it had been
      // refused. This list is exhaustive by design, and an exhaustive list only
      // fails one way -- "too many" -- so it never caught the missing grant. The
      // other direction is now enforced natively, in
      // `every_command_the_protected_renderer_invokes_is_granted_to_its_webview`.
      "allow-rehydrate-native-discord-overlay-history",
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
      "core:event:allow-emit-to",
      "allow-activate-native-manual-peer-context",
      "allow-set-native-discord-protected-overlay-open",
    ]));
    expect(hub).not.toHaveProperty("remote");
    expect(overlay.permissions).not.toEqual(expect.arrayContaining([
      "allow-prepare-encrypted-text",
      "allow-decrypt-hub-capsule",
    ]));
  });

  it("ships as a dedicated local entry with no networking, storage, or context token", () => {
    const vite = readRelative("../vite.config.ts");
    const source = readRelative("./overlay.ts");
    const adapter = readRelative("./native-overlay-adapter.ts");
    const native = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    expect(vite).toContain('overlay: fileURLToPath(new URL("./overlay.html"');
    const invoked = [...adapter.matchAll(/invoke<unknown>\("([^"]+)"/g)].map((match) => match[1]);
    expect(invoked).toEqual([
      "get_native_discord_overlay_state",
      "send_native_discord_qa_probe",
      "prepare_native_discord_overlay_text",
      "send_native_discord_overlay_carrier",
      "send_native_discord_qa_atomic_text",
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
    expect(`${source}\n${adapter}`).not.toMatch(/contextToken|BroadcastChannel/);
    expect(source).toContain(
      'listen<boolean>(PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT, ({ payload }) => {',
    );
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
    expect(source).toContain('import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1"');
    expect(source).toContain("QA backend rejection ·");
  });

  it("uses a bounded protected transcript and composer with configuration controls removed from view", () => {
    const html = readRelative("../overlay.html");
    const css = readRelative("./overlay.css");
    const source = readRelative("./overlay.ts");
    const native = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    expect(html).toContain('class="trust-mark"');
    expect(html).toContain("This composer belongs to OSL, not Discord");
    expect(html).toContain('id="protected-view-once"');
    expect(html).toContain('id="protected-ttl"');
    expect(html).toContain('id="protected-decrypt-display"');
    expect(html).toContain('id="current-expiry"');
    expect(html).toContain('id="protected-send-mode" aria-label="Send behavior"');
    expect(html).toContain('<option value="button">Manual</option>');
    expect(html).toContain('<option value="double">Double Enter</option>');
    expect(html).toContain('<option value="single">Experimental Single Enter</option>');
    expect(html).not.toContain(">Button</option>");
    expect(html).not.toContain("Single Enter · Experimental");
    expect(html).toContain('id="osl-message-list"');
    expect(html).toContain('class="native-composer-backdrop"');
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
    expect(css).toContain("grid-template-rows: minmax(0, 1fr) minmax(40px, var(--osl-composer-min-height))");
    expect(css).toContain("border: 0;");
    // The cyan ring is the "protection is on" indicator, not decoration: it is
    // drawn only by OSL's own protected window, so its presence is how the
    // operator tells OSL's composer from Discord's real message box sitting
    // pixels underneath it -- and typing into the wrong one sends plaintext. It
    // was briefly removed to make the surface seamless and had to be restored,
    // so these assertions exist to stop that happening again. (It no longer
    // doubles as a lock-engaged cue via `#write-pane { display: none }` on
    // `data-osl-lock-engaged="false"`; that rule is banned, and the only rule
    // that may hide the composer is keyed on the native band surrender.)
    expect(css).toContain("one-pixel cyan boundary");
    expect(css).toContain(".composer-box::after");
    expect(css).toContain("border: 1px solid rgba(73, 214, 255, .55)");
    expect(css).toContain("pointer-events: none");
    expect(css).toContain("z-index: 3");
    expect(css).toContain(".composer-box:focus-within::after");
    expect(css).toContain("border-color: rgba(73, 214, 255, .72)");
    expect(css).toMatch(/@media \(forced-colors: active\)[\s\S]*\.composer-box \{ border: 1px solid CanvasText; \}/u);
    expect(css).toContain(':root[data-native-composer-capture="true"] .draft-field textarea');
    expect(css).not.toContain("padding-top: 3px");
    // Was pinned to `var(--osl-native-edit-font-family, inherit)`. The contract
    // changed because that declaration was the bug: the variable holds the ONE
    // family name UIA measured on Discord's input -- "gg sans", a webfont inside
    // Discord's renderer that this process cannot resolve -- with no fallback
    // behind it, so WebView2 dropped to its default serif. The measured family
    // still leads; what is new is that something sane has to follow it.
    expect(css).toContain(
      'font-family: var(--osl-native-edit-font-family, "gg sans"), var(--osl-discord-font-stack);',
    );
    expect(css).not.toContain("font-family: var(--osl-native-edit-font-family, inherit)");
    expect(css).toContain("font-size: var(--osl-native-edit-font-size, 14px)");
    expect(css).toContain("line-height: var(--osl-native-edit-line-height");
    expect(css).not.toContain("outline: 1px solid rgba(73, 214, 255, .55)");
    expect(css).not.toContain("box-shadow: inset 0 0 0 1px rgba(73, 214, 255, .14)");
    expect(css).toContain(':root[data-native-composer-capture="true"] .native-composer-backdrop');
    expect(css).toContain("background-image: var(--osl-native-composer-image, none)");
    expect(css).toContain("left: var(--osl-native-edit-left)");
    expect(css).toContain("top: var(--osl-native-edit-top)");
    expect(css).toContain("width: var(--osl-native-edit-width)");
    expect(css).toContain("height: var(--osl-native-edit-height)");
    expect(css).toContain("--osl-native-edit-layer: var(--osl-composer-fill);");
    expect(css).toContain("--osl-composer-fill: var(--osl-native-edit-background, transparent);");
    expect(css).toContain(".overlay-shell {");
    expect(css).toContain("background: transparent");
    // Load-bearing, not cosmetic: the green eye reveals Discord's flagtext by
    // hiding OSL's own transcript layer, so this surface must stay see-through
    // wherever OSL is not painting. Painting it opaque -- for instance to cover
    // a white band -- would make hiding the layer reveal the paint instead of
    // Discord, and silently break the eye. A white band is a native per-pixel
    // alpha defect and is repaired natively.
    expect(css).toMatch(/\.overlay-shell\s*\{[^}]*background:\s*transparent;/su);
    expect(css).not.toMatch(
      /\.overlay-shell\s*\{[^}]*background:\s*var\(--osl-overlay-bg\)/su,
    );
    expect(native).toContain("fn enforce_transparent_protected_composer(");
    expect(native).toContain("DwmEnableBlurBehindWindow(");
    expect(css).toContain(".trust-mark {\n  display: none;");
    expect(css).toContain(':root[data-discord-qa-shell="true"] #write-pane');
    expect(css).toContain("height: 100%");
    expect(css).not.toContain("padding: 0 14px 14px");
    expect(css).not.toContain("min-height: var(--osl-composer-min-height)");
    expect(source).toContain("applyDiscordVisualRecipe(state.visualRecipe)");
    expect(source).toContain("applyNativeSurfaceCapture(state.nativeSurface)");
    expect(source).toContain('root.dataset.nativeComposerCapture = "true"');
    expect(source).toContain('root.style.setProperty("--osl-native-composer-image"');
    expect(source).toContain('root.style.setProperty("--osl-native-edit-left"');
    expect(source).toContain("percent(capture.textLeftPx, capture.widthPx)");
    expect(source).toContain("percent(capture.textTopPx, capture.heightPx)");
    expect(source).toContain('"--osl-native-edit-font-family"');
    expect(source).toContain("delete root.dataset.nativeComposerCapture");
    expect(source).toContain('draft.placeholder = state.nativeSurface ? " " : exactDiscordPlaceholder');
    expect(css).not.toContain("#osl-message-list li:last-child");
  });

  it("explains the transparent shell without citing the removed cyan boundary", () => {
    const css = readRelative("./overlay.css");
    const native = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    const shellNote = css.slice(0, css.indexOf(".overlay-shell {"));
    // The ring is the lock-engaged indicator and is present, so this note names
    // it and points at the rule that documents why it must not be deleted.
    expect(shellNote).toContain("The only persistent OSL cue is the composer's one-pixel cyan boundary");
    expect(shellNote).toContain("composer-box::after");
    // The warning around it is load-bearing and must survive that correction:
    // an opaque shell is never the fix for a white artifact on this surface.
    expect(shellNote).toContain("This background MUST stay transparent");
    expect(shellNote).toContain("lost per-pixel alpha");
    expect(shellNote).toContain("enforce_transparent_protected_composer");
    // With the ring removed, the composer's rounded corners are the only
    // alpha-0 pixels left on a composer-sized window, which is why the same
    // native defect now reads as white corner slivers rather than a band.
    expect(shellNote).toContain("border-radius");
    expect(css).toMatch(/\.composer-box\s*\{[^}]*border-radius:\s*8px;/su);
    // The native repair has to survive the composer's other reveal. The QA
    // carrier stack shows this window with SWP_SHOWWINDOW instead of Tauri's
    // show, so it never reaches `reveal_protected_pair` and has to restore the
    // alpha its own post-show frame strip can clear.
    const carrierStack = native.slice(native.indexOf("fn active_ensure_carrier_stack("));
    const carrier = carrierStack.slice(0, carrierStack.indexOf("\n}\n"));
    expect(carrier).toContain("SWP_SHOWWINDOW");
    expect(carrier.indexOf("enforce_native_frameless_overlay(overlay)")).toBeLessThan(
      carrier.indexOf("enforce_transparent_protected_composer(overlay)"),
    );
  });

  it("never lets a measured Discord font family fall through to a serif", () => {
    const css = readRelative("./overlay.css");
    // Every surface that paints text against Discord's own text takes its family
    // from the same chain, so the composer and the carrier rows cannot drift
    // apart from each other or from the shell.
    const stack = css.match(/--osl-discord-font-stack:([^;]*);/su)?.[1] ?? "";
    expect(stack).not.toBe("");
    // Discord's family first for fidelity, the bundled Inter as the first
    // fallback that can actually resolve in this process, a generic sans last.
    expect(stack.indexOf('"gg sans"')).toBeLessThan(stack.indexOf('"Inter Variable"'));
    expect(stack.trimEnd().endsWith("sans-serif")).toBe(true);
    expect(stack).not.toMatch(/\bserif\b(?<!sans-serif)/u);
    expect(css).toContain("font-family: var(--osl-discord-font-stack);");

    // Both measured-family consumers end in that chain. Each variable carries a
    // single family name with no fallback of its own, so a bare `var(...)` or a
    // lone generic is the defect, not a style preference.
    const measured = [...css.matchAll(/font-family:\s*var\(--osl-(?:native-edit|carrier)-font-family[^;]*;/gu)];
    expect(measured).toHaveLength(2);
    for (const [declaration] of measured) {
      expect(declaration).toContain("var(--osl-discord-font-stack)");
      // A CSS-wide keyword is only legal as the entire value, so `inherit` in a
      // list makes the whole declaration invalid at computed-value time the
      // moment the measured variable is absent -- which it is whenever the
      // native capture came back without all four font measurements.
      expect(declaration).not.toContain("inherit");
    }

    // Synthesis stays off: a faux-bold is a smeared wrong face rendered inches
    // from Discord's real text, and Inter Variable has a true weight axis.
    expect(css).toContain("font-synthesis: none;");
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
    // EXPECTED SHAPE CHANGED. A received message now carries the correlation
    // handle the renderer needs to paint it over the Discord row it belongs to:
    // an id, and the public cover of that row. Before this, an inbound message
    // was anonymous and could only be appended in key-server inbox order.
    const opened = { messageId: "peer-fedcba98765432100123456789abcdef", coverPointer: "ordinary looking cover prose", plaintext: "first\n\nthird", contextVerified: true, personToPersonE2ee: true, viewOnceConsumed: true, expiresAt: 1_787_000_000 };
    const acknowledgment = { messageId: prepared.messageId, status: "opened", acknowledgedAt: 1_786_999_900 };
    expect(parseNativeDiscordOverlayState(state)).toEqual(state);
    expect(parseNativeDiscordOverlayPrepared(prepared)).toEqual(prepared);
    expect(parseNativeDiscordOverlayOpened(opened)).toEqual(opened);
    expect(parseNativeDiscordOverlayAcknowledgment(acknowledgment)).toEqual(acknowledgment);
    // A reassembled multi-row message has no single carrier row, so the cover key
    // is absent rather than null -- both arities parse, and nothing in between.
    const { coverPointer: _cover, ...openedWithoutCover } = opened;
    expect(parseNativeDiscordOverlayOpened(openedWithoutCover)).toEqual(openedWithoutCover);
    expect(parseNativeDiscordOverlayOpened({ ...opened, coverPointer: null })).toBeNull();
    expect(parseNativeDiscordOverlayOpened({ ...opened, coverPointer: "two\nlines" })).toBeNull();
    // The handle is held to the id shape the rest of this path already uses.
    expect(parseNativeDiscordOverlayOpened({ ...opened, messageId: "not-a-peer-id" })).toBeNull();
    const pendingViewOnce = { messageId: "peer-0123456789abcdef0123456789abcdef", expiresAt: prepared.expiresAt, personToPersonE2ee: true };
    const batch = { messages: [opened], pendingViewOnce: [pendingViewOnce], acknowledgments: [acknowledgment], fetched: 2, decryptDisplayEnabled: true, deferredRows: 0 };
    expect(parseNativeDiscordOverlayOpenedBatch(batch)).toEqual(batch);
    // A batch that cannot say whether opening was switched on, or how many rows it
    // deferred, is the ambiguous shape this parser now refuses: those two states
    // were previously indistinguishable from an empty inbox.
    const { decryptDisplayEnabled: _display, ...batchWithoutDisplay } = batch;
    expect(parseNativeDiscordOverlayOpenedBatch(batchWithoutDisplay)).toBeNull();
    const { deferredRows: _deferred, ...batchWithoutDeferred } = batch;
    expect(parseNativeDiscordOverlayOpenedBatch(batchWithoutDeferred)).toBeNull();
    expect(parseNativeDiscordOverlayOpenedBatch({ ...batch, decryptDisplayEnabled: false })?.decryptDisplayEnabled).toBe(false);
    expect(parseNativeDiscordOverlayOpenedBatch({ ...batch, deferredRows: 3 })?.deferredRows).toBe(3);
    expect(parseNativeDiscordOverlayOpenedBatch({ ...batch, deferredRows: -1 })).toBeNull();
    expect(parseNativeDiscordOverlayState({ ...state, scopeApproved: false })).toBeNull();
    const { discordMarkerAvailable: _marker, ...stateWithoutMarkerAvailability } = state;
    expect(parseNativeDiscordOverlayState(stateWithoutMarkerAvailability)).toBeNull();
    expect(parseNativeDiscordOverlayState({ ...state, attachmentsEnabled: false })?.attachmentsEnabled).toBe(false);
    expect(parseNativeDiscordOverlayState({ ...state, discordMarkerAvailable: true })?.discordMarkerAvailable).toBe(true);
    const visualRecipe = {
      version: "osl-discord-visual-recipe-v1",
      theme: "light",
      density: "compact",
      zoom: 1,
      dpiScale: 1,
      messageColumnWidthPx: 640,
      composerWidthPx: 640,
      composerMinHeightPx: 48,
      lineHeightPx: 18,
      averageGraphemeWidthPx: 7,
      highContrast: false,
      reducedMotion: false,
    };
    expect(parseNativeDiscordOverlayState({ ...state, visualRecipe })?.visualRecipe).toEqual(visualRecipe);
    expect(parseNativeDiscordOverlayState({ ...state, visualRecipe: { ...visualRecipe, messageText: "secret" } })).toBeNull();
    const nativeSurface = nativeSurfaceFixture();
    expect(parseNativeSurfaceCapture(nativeSurface)).toEqual(nativeSurface);
    expect(parseNativeDiscordOverlayState({ ...state, nativeSurface })?.nativeSurface).toEqual(nativeSurface);
    expect(parseNativeDiscordOverlayState({ ...state, nativeSurface: null })).toEqual(state);
    const visibleCarrierRows = [{
      messageId: prepared.messageId,
      nativeLocatorSha256: "1".repeat(64),
      carrierSha256: "2".repeat(64),
      leftPx: 160,
      topPx: 320,
      widthPx: 480,
      heightPx: 36,
      backgroundColor: "rgb(49 51 56)",
      foregroundColor: "rgb(219 222 225)",
      fontFamily: "gg sans",
      fontSizePx: 16,
      fontWeight: 400,
      lineHeightPx: 20,
      letterSpacingPx: 0,
      zoom: 1,
      density: 1.25,
    }];
    expect(parseNativeDiscordOverlayState({ ...state, visibleCarrierRows })?.visibleCarrierRows)
      .toEqual(visibleCarrierRows);
    expect(parseNativeDiscordOverlayState({
      ...state,
      visibleCarrierRows: [{ ...visibleCarrierRows[0], plaintext: "secret" }],
    })).toBeNull();
    expect(parseNativeSurfaceCapture({ ...nativeSurface, inputWidthPx: 281 })).toBeNull();
    expect(parseNativeSurfaceCapture({ ...nativeSurface, fontWeight: null })).toBeNull();
    expect(parseNativeSurfaceCapture({
      ...nativeSurface,
      fontFamily: null,
      fontSizePx: null,
      fontWeight: null,
      lineHeightPx: null,
    })).toEqual({
      ...nativeSurface,
      fontFamily: null,
      fontSizePx: null,
      fontWeight: null,
      lineHeightPx: null,
    });
    expect(parseNativeSurfaceCapture({ ...nativeSurface, imageDataUrl: nativeSurface.imageDataUrl.replace("Qk", "AA") })).toBeNull();
    expect(parseNativeSurfaceCapture({ ...nativeSurface, messageText: "secret" })).toBeNull();
    expect(parseNativeDiscordOverlayPrepared({ ...prepared, deliveredToOslInbox: false })).toBeNull();
    expect(parseNativeDiscordOverlayOpened({ ...opened, plaintext: "🙂".repeat(262_145) })).toBeNull();
    expect(parseNativeDiscordOverlayOpened({ ...opened, expiresAt: 0 })).toBeNull();
    expect(parseNativeDiscordOverlayAcknowledgment({ ...acknowledgment, status: "read" })).toBeNull();
    expect(parseNativeDiscordOverlayOpenedBatch({ messages: Array.from({ length: 65 }, () => opened), pendingViewOnce: [], acknowledgments: [], fetched: 64 })).toBeNull();
    expect(parseNativeDiscordOverlayOpenedBatch({ messages: [], pendingViewOnce: [{ ...pendingViewOnce, messageId: "../message" }], acknowledgments: [], fetched: 1 })).toBeNull();
  });

  it("commits QA plaintext before its Discord flag and renders the exact row immediately", () => {
    const source = readRelative("./overlay.ts");
    const native = readRelative("../../osl-hub/src/main.rs");
    expect(source).toContain("placementMode.disabled = sendBusy || !overlayReady || !discordMarkerAvailable");
    expect(source).toContain("if (!discordMarkerAvailable || !coverTextEnabled)");
    expect(source).toContain("await sendNativeDiscordQaAtomicText(");
    expect(source).toContain("result = atomic.prepared;");
    expect(source).toContain("immediateCarrierRow = atomic.visibleCarrierRow;");
    expect(source).toContain("applyVerifiedCarrierRows(verifiedCarrierRows);");
    expect(source.indexOf("await sendNativeDiscordQaAtomicText(")).toBeLessThan(
      source.indexOf("result = atomic.prepared;"),
    );
    expect(source).toContain("if (result && discordMarkerAvailable && coverTextEnabled)");
    expect(source).toContain("const carrier = await captureC4NativeReceipt(");
    expect(source).toMatch(
      /sendNativeDiscordOverlayCarrier\(\s*requestedPlacement,\s*charsPerSecond,\s*measuredCarrierLayout\(\),\s*\)/u,
    );
    expect(source).toContain('padding: "shapeMatched"');
    expect(source).toContain("Sent privately through OSL only. No Discord marker was attempted.");
    expect(source).toContain("Ready for OSL-only messages. Discord marker placement is unavailable.");
    expect(source).toContain('markerSent ? " · Discord marked" : " · OSL only"');
    const atomicStart = native.indexOf("async fn send_native_discord_qa_atomic_text");
    const atomicEnd = native.indexOf("async fn send_native_discord_qa_probe", atomicStart);
    const atomic = native.slice(atomicStart, atomicEnd);
    // What goes to Discord is the encrypted message's own wordbank flagtext, so
    // the plan's cover text has to be resolved -- and refused when there is none
    // -- before a single keystroke is placed. Deliberately not asserting an
    // `ok_or_else`: a refused flagtext reports WHICH reason refused it back to
    // this renderer, which a plain error mapping cannot express.
    expect(atomic).toContain("plan.cover_text()");
    expect(atomic).not.toContain("LocalCoverState::free_cover");
    expect(atomic.indexOf("plan.cover_text()")).toBeLessThan(
      atomic.indexOf("composer.place_carrier("),
    );
    expect(atomic.indexOf("broker::prepare_native_discord_overlay_text(")).toBeLessThan(
      atomic.indexOf("composer.place_carrier("),
    );
    expect(atomic).toContain("native_discord_carrier_row_dtos(");
    expect(atomic).toContain(".find(|row| row.message_id == prepared.message_id)");
  });

  it("gates the deterministic P2P probe to the disposable native QA build", () => {
    const native = readRelative("../../osl-hub/src/main.rs");
    const nativeOverlay = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    const adapter = readRelative("./native-overlay-adapter.ts");
    const overlay = readRelative("./overlay.ts");
    const permissions = readRelative("../../osl-hub/permissions/hub.toml");

    expect(native).toMatch(
      /#\[cfg\(feature = "discord-qa-shell"\)\]\s*#\[tauri::command\]\s*async fn send_native_discord_qa_probe/u,
    );
    expect(native).toContain('const QA_PROBE_PLAINTEXT: &str = "OSL Discord QA probe";');
    expect(native).toContain("wait_for_registered_transport(");
    expect(native).toContain("discord_qa_inbound_receipt::record_outbound(");
    expect(native).toContain("discord_qa_inbound_receipt::record_poll(");
    expect(native).toContain("record_overlay_open_stage(");
    expect(nativeOverlay).toContain(
      '#[cfg(feature = "discord-qa-shell")]\nfn active_overlay_requires_focus_acquisition() -> bool {\n    desktop_has_foreground_window()',
    );
    expect(nativeOverlay).toContain(
      '#[cfg(not(feature = "discord-qa-shell"))]\nfn active_overlay_requires_focus_acquisition() -> bool {\n    true',
    );
    // And with the composer band surrendered nobody asks for the foreground at
    // all: that band is Discord's message box, so the keyboard is Discord's. (The
    // bare `if active_overlay_requires_focus_acquisition() {` this used to pin is
    // stale -- see the caret test above.)
    expect(nativeOverlay).toContain("if active_overlay_requires_focus_acquisition() && !band_surrendered {");
    const qaSend = native.slice(
      native.indexOf("async fn send_native_discord_qa_probe"),
      native.indexOf("async fn open_native_discord_overlay_text"),
    );
    const securitySettings = native.slice(
      native.indexOf("async fn set_native_discord_overlay_security"),
      native.indexOf("async fn prepare_native_discord_overlay_text"),
    );
    const userComposerSend = native.slice(
      native.indexOf("async fn prepare_native_discord_overlay_text"),
      native.indexOf("async fn send_native_discord_qa_probe"),
    );
    const atomicComposerSend = native.slice(
      native.indexOf("async fn send_native_discord_qa_atomic_text"),
      native.indexOf("async fn send_native_discord_qa_probe"),
    );
    expect(qaSend).toContain("wait_for_registered_transport(");
    expect(qaSend.indexOf("wait_for_registered_transport"))
      .toBeLessThan(qaSend.indexOf("session.transition.lock().await"));
    expect(qaSend.indexOf("session.transition.lock().await"))
      .toBeLessThan(qaSend.indexOf("require_overlay_context_snapshot"));
    expect(qaSend.indexOf("require_overlay_context_snapshot"))
      .toBeLessThan(qaSend.indexOf("prepare_native_discord_overlay_text"));
    expect(qaSend).toContain("record_send_stage(");
    expect(qaSend).toContain("registration.terminal_state");
    expect(securitySettings).not.toContain("wait_for_registered_transport(");
    expect(userComposerSend).toContain("wait_for_registered_transport(");
    expect(userComposerSend.indexOf("wait_for_registered_transport"))
      .toBeLessThan(userComposerSend.indexOf("session.transition.lock().await"));
    expect(atomicComposerSend).toContain("composer.place_carrier(");
    expect(atomicComposerSend).toContain("DiscordCarrierStatus::Sent");
    expect(atomicComposerSend.indexOf("broker::prepare_native_discord_overlay_text("))
      .toBeLessThan(atomicComposerSend.indexOf("composer.place_carrier("));
    const composerOpen = native.slice(
      native.indexOf("async fn set_native_discord_protected_overlay_open"),
      native.indexOf("fn get_native_discord_overlay_state"),
    );
    expect(composerOpen).toContain("wait_for_registered_transport(");
    expect(native).toContain("caller.label() != native_discord_overlay::OVERLAY_LABEL");
    expect(native).toContain('#[cfg(feature = "discord-qa-shell")]\n            send_native_discord_qa_probe,');
    expect(native).toContain('#[cfg(feature = "discord-qa-shell")]\n            send_native_discord_qa_atomic_text,');
    expect(permissions).toContain('commands.allow = ["send_native_discord_qa_probe"]');
    expect(permissions).toContain('commands.allow = ["send_native_discord_qa_atomic_text"]');
    expect(adapter).toContain('if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1") return null;');
    expect(adapter).toContain('invoke<unknown>("send_native_discord_qa_probe")');
    expect(overlay).toContain('const discordQaShell = import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1"');
    expect(overlay).toContain("if (discordQaShell)");
    expect(overlay).toContain('event.key !== "F12"');
    expect(overlay).not.toMatch(/sendNativeDiscordQaProbe\([^)]/u);
  });

  it("keeps hidden-RDP receive polling QA-only without bypassing protection state", () => {
    expect(shouldPollDiscordOverlay({
      overlayReady: true,
      decryptDisplayEnabled: true,
      documentHidden: true,
      discordQaShell: false,
    })).toBe(false);
    expect(shouldPollDiscordOverlay({
      overlayReady: true,
      decryptDisplayEnabled: true,
      documentHidden: true,
      discordQaShell: true,
    })).toBe(true);
    expect(shouldPollDiscordOverlay({
      overlayReady: false,
      decryptDisplayEnabled: true,
      documentHidden: true,
      discordQaShell: true,
    })).toBe(false);
    expect(shouldPollDiscordOverlay({
      overlayReady: true,
      decryptDisplayEnabled: false,
      documentHidden: true,
      discordQaShell: true,
    })).toBe(false);

    const source = readRelative("./overlay.ts");
    expect(source).toContain("if (!discordQaShell) {");
    expect(source).toContain("removeViewOnceBubbles();");
    expect(source).toContain("scheduleReceivePoll(0);");
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
    // The eye takes OSL's whole layer off Discord, which is now correct in the
    // shipping build too: nothing opaque sits behind it, so what is revealed is
    // Discord's own rows -- real history, other people's messages, scrollback.
    expect(visibility).toContain("messageList.hidden = !visible");
    expect(visibility).toContain('row.plaintext = messagePlaintext.get(row.key) ?? ""');
    expect(visibility).toContain("row.plaintextHidden = !visible");
    expect(visibility).not.toContain('body.textContent = ""');
    expect(save).toContain("applyDecryptDisplayVisibility(decryptDisplayEnabled)");
    expect(save).toContain("applyDecryptDisplayVisibility(false)");
    expect(save.indexOf("applyDecryptDisplayVisibility(false)")).toBeLessThan(
      save.indexOf("await setNativeDiscordOverlaySecurity"),
    );
    expect(save).toContain("applyDecryptDisplayVisibility(previousDecrypt)");
    // EXPECTED VALUE CHANGED: the eye-on branch used to be the single statement
    // `if (decryptDisplayEnabled) scheduleReceivePoll(0)`. Switching the eye on
    // now also claims one bounded transcript read -- the edge that puts the
    // operator's decryptable history on screen -- so the branch is a block. The
    // ordering guarantee this test exists for is unchanged and still asserted:
    // visibility is applied synchronously BEFORE anything is allowed to poll.
    expect(save).toContain("if (decryptDisplayEnabled) {\n    scheduleReceivePoll(0);");
    expect(save.indexOf("applyDecryptDisplayVisibility(decryptDisplayEnabled)")).toBeLessThan(
      save.indexOf("if (decryptDisplayEnabled) {"),
    );
    expect(save).not.toContain("clearMessageBubbles()");
  });

  it("paints per row over Discord and never over a row it cannot place", () => {
    const source = readRelative("./overlay.ts");
    const styles = readRelative("./overlay.css");
    // EXPECTED VALUE CHANGED: the painting itself moved out of
    // `applyVerifiedCarrierRows` into `paintBoundRows`, because the just-sent
    // carrier rows are no longer the only source of rows to paint. The contract
    // this test exists for is unchanged -- clear everything, then paint only
    // what the backend proved -- and is now asserted against that function.
    const bind = source.slice(source.indexOf("function paintBoundRows"), source.indexOf("function decodedRowPresentation"));

    // Every rendered row starts hidden and only becomes visible once the backend
    // has proven where its exact Discord row is on screen. Ordinary chat,
    // undecodable rows and history OSL cannot place are therefore Discord's own
    // rows, untouched.
    expect(bind).toContain("clearCarrierRowGeometry(item)");
    expect(bind).toContain("applyCarrierRowGeometry(item, binding)");
    expect(bind).not.toContain("discordQaShell");
    // The eye gates the whole layer, so no row can be painted with it off. The
    // lock is deliberately absent: it is encryption only.
    expect(bind).toContain("if (!decryptDisplayEnabled) return;");
    expect(bind).not.toContain("lockEngaged");
    expect(source).toContain("decryptDisplayEnabled ? bindings ?? [] : []");
    // No full-height panel: this layer paints nothing of its own and lets every
    // click through to Discord.
    expect(styles).toMatch(
      /\n\.transcript-mount\s*\{[^}]*background:\s*transparent;[^}]*pointer-events:\s*none;/su,
    );
    expect(styles).toMatch(
      /\n\.osl-discord-transcript__row\s*\{[^}]*pointer-events:\s*none;/su,
    );
  });

  it("keys the eye to every decodable Discord row, not to the messages this client sent", () => {
    const source = readRelative("./overlay.ts");
    const rowProjection = readRelative("./discord-row-attribution.ts");
    const paint = source.slice(source.indexOf("function paintBoundRows"), source.indexOf("function decodedRowPresentation"));
    const apply = source.slice(source.indexOf("function applyDecodedTranscript"), source.indexOf("function clearDecodedTranscript"));

    // The defect this fixes: the eye was keyed to `outgoingBubbles`, the
    // messages this client happened to send during this session. A row received
    // last week is perfectly decodable and was simply never in that map, so the
    // eye could never work on history. Decodable rows are now painted FIRST and
    // by their own binding; `outgoingBubbles` survives only as the QA shell's
    // just-sent carrier path, which does not exist in a shipping build.
    expect(paint.indexOf("decodedRowBindings")).toBeLessThan(paint.indexOf("outgoingBubbles"));
    expect(apply).not.toContain("outgoingBubbles");

    // A row is painted only when OSL can decrypt it, place it, and carry the
    // backend's complete native-row/crypto agreement. Any missing half means
    // OSL owns no pixel there and Discord's own row shows through.
    expect(apply).toContain("const visible = projectNativeDiscordVisibleRow(row);");
    expect(apply).toContain("if (visible === null) continue;");
    const projection = readRelative("./discord-row-attribution.ts");
    expect(projection).toContain(
      "if (plaintext === null || orientation === null || attribution === null || row === null)",
    );
    expect(projection).toContain("if (attribution.orientation !== orientation) return null;");
    expect(apply).toContain(
      'author: visible.author === "self" ? localIdentity : verifiedFriendIdentity',
    );
    expect(apply).toContain("const key = visible.key");
    expect(projection).toContain("key: `decoded-${attribution.nativeLocatorSha256}`");

    // Rebuilt wholesale from one read, never accumulated: a row kept from an
    // earlier read is decrypted text sitting over whatever Discord has since
    // scrolled into its place.
    expect(apply).toContain("decodedRows.length = 0;");
    expect(apply).toContain("decodedRowBindings.clear();");

    // The decrypted text goes with the row, in the same statement that drops it.
    const clear = source.slice(source.indexOf("function clearDecodedTranscript"), source.indexOf("async function refreshVerifiedCarrierRows"));
    expect(clear).toContain("messagePlaintext.delete(key)");

    // The row's own public cover is never rendered by OSL. With the eye on OSL
    // shows the message; with it off OSL shows nothing and Discord's row already
    // says the cover itself.
    expect(apply).not.toContain("row.flagtext");
  });

  it("refreshes the eye on edges only and never on a timer", () => {
    const source = readRelative("./overlay.ts");
    const schedule = source.slice(source.indexOf("function scheduleTranscriptRehydrate"), source.indexOf("async function runTranscriptRehydrate"));
    const run = source.slice(source.indexOf("async function runTranscriptRehydrate"), source.indexOf("function transcriptTimestamp"));

    // THE trap. A previous implementation re-resolved Discord's accessibility
    // tree once per poll and froze this app for 19,207 ms. Nothing in this
    // renderer may ever schedule a repeating read.
    expect(source).not.toContain("setInterval");
    // The coalescer never polls: the scheduler's setTimeout calls
    // runTranscriptRehydrate once, and completion can only replay one pending
    // edge that arrived while the read was in flight.
    expect(schedule.match(/setTimeout/gu)?.length).toBe(1);
    // A completed read may replay exactly one edge that arrived mid-read, but it
    // may not restart an already scheduled floor retry or coalesced read. That
    // keeps completion chain-free while preserving the row-moved edge that often
    // arrives during the first read of a newly grown overlay window.
    const settled = run.slice(run.indexOf("const result = await rehydrateNativeDiscordOverlayHistory"));
    expect(settled.match(/scheduleTranscriptRehydrate\(\)/gu)?.length).toBe(1);
    expect(settled).toContain("if (rehydratePending) {");
    expect(settled).toContain("if (rehydrateTimer === undefined) scheduleTranscriptRehydrate();");
    // And the watchdog is a per-request timeout, not a poll: armed by a read that
    // started, cleared by that read finishing, never re-arming itself.
    const watchdog = run.slice(run.indexOf("const watchdog = window.setTimeout"), run.indexOf("const result = await"));
    expect(watchdog).toContain("abandoned = true;");
    expect(watchdog).toContain("rehydrateBusy = false;");
    expect(watchdog).toContain("scheduleTranscriptRehydrate();");
    expect(watchdog.match(/setTimeout/gu)?.length).toBe(1);
    expect(run).toContain("window.clearTimeout(watchdog);");
    // Its budget must exceed every bound the backend leg has, or a slow-but-alive
    // read would be abandoned and re-asked in a loop.
    expect(source).toContain("const REHYDRATE_IN_FLIGHT_BUDGET_MS = 6_000;");
    // An abandoned read never paints: its geometry is old enough that the
    // watchdog gave up on it, and painting it would be decrypted text over
    // whatever Discord has since scrolled into place.
    expect(run).toContain("if (abandoned) return;");
    expect(run).toMatch(/if \(!abandoned\) \{\s*rehydrateBusy = false;/u);

    // A pending read is always replaced, never queued behind itself.
    expect(schedule).toContain("if (rehydrateTimer !== undefined) window.clearTimeout(rehydrateTimer);");
    // Nothing is claimed while there is nothing to paint: no session, or eye off.
    expect(schedule).toContain("if (!overlayReady || !decryptDisplayEnabled) return;");
    // Concurrent reads are remembered, not stacked, so an edge during a read
    // cannot start a second bounded accessibility walk on top of the first.
    expect(run).toContain("if (rehydrateBusy");
    expect(run).toContain("rehydratePending = true;");

    // A refused edge is replaced exactly once, and the replacement may not arm
    // another -- by the time it runs the backend's floor has elapsed.
    expect(run).toContain("if (rehydrateReplacementArmed");
    expect(run).toContain("rehydrateReplacementArmed = true;");

    // The receive poll is itself a timer, so it must only raise an edge when the
    // backend actually reported something new -- otherwise the eye would read
    // Discord's accessibility tree once per receive tick.
    const poll = source.slice(source.indexOf("async function pollReceived"), source.indexOf('document.addEventListener("visibilitychange"'));
    expect(poll).toContain("if (batch.messages.length > 0 || batch.pendingViewOnce.length > 0");
    expect(poll).toContain("scheduleTranscriptRehydrate();");

    // The scroll edge observes and never consumes the gesture, and no window is
    // hidden, moved or restyled to make any of this work.
    expect(source).toContain('window.addEventListener("wheel", () => scheduleTranscriptRehydrate(), { capture: true, passive: true });');
  });

  it("never lets one unreadable state read disarm the eye for the rest of the session", () => {
    const source = readRelative("./overlay.ts");
    const refresh = source.slice(
      source.indexOf("async function refreshProtectedDisplayVisibility"),
      source.indexOf("ttl.addEventListener"),
    );
    expect(refresh).not.toBe("");

    // THE defect. This function cancels the eye's armed read when the state
    // command answers nothing -- and that command shares ONE non-blocking native
    // accessibility gate with the eye's own read, so any other in-flight
    // accessibility operation is enough to make it answer nothing. Cancelling on
    // that answer disarms the eye with no error and nothing to repaint it: every
    // edge that would have asked again is the edge that was just cancelled.
    expect(refresh).toContain("cancelTranscriptRehydrate();");
    expect(refresh).toContain("if (!state) scheduleTranscriptRehydrate();");

    // The two answers must stay distinguishable. A session the backend positively
    // reports as over stays cancelled; an unreadable one is asked again.
    expect(refresh).toContain("if (!state || !state.active) {");
    // Whatever was painted still comes down in both: OSL may not keep decrypted
    // text over rows it can no longer place.
    expect(refresh).toContain("clearDecodedTranscript();");
    // Through the same coalescer as every other edge -- no new timer here.
    const unreadable = refresh.slice(0, refresh.indexOf("applyLockEngaged"));
    expect(unreadable).not.toContain("setTimeout");
    expect(unreadable).not.toContain("setInterval");
  });

  it("realigns the painted rows on the native rows-moved edge, through the existing bound and nothing else", () => {
    const source = readRelative("./overlay.ts");

    // The renderer cannot see Discord move: OSL owns pixels only where OSL draws,
    // and Discord is another process. This is the announcement that closes it.
    expect(source).toContain('const NATIVE_DISCORD_ROWS_MOVED_EVENT = "osl://native-discord-rows-moved";');

    const listener = source.slice(
      source.indexOf("void listen<void>(NATIVE_DISCORD_ROWS_MOVED_EVENT"),
      source.indexOf("// The WebView is retained so the lock toggle can show it instantly."),
    );
    expect(listener).not.toBe("");

    // Routed through the established coalescer, which already carries the 800 ms
    // trailing collapse, the backend's own floor behind it and the refusal to
    // start a second read while one is in flight. A second scheduler in front of
    // it would pace nothing and hide the bound that matters.
    expect(listener).toContain("scheduleTranscriptRehydrate();");
    // No new debounce, no timer, no polling: an unpaced per-tick accessibility
    // re-resolve is what froze this app for 19,207 ms.
    expect(listener).not.toContain("setTimeout");
    expect(listener).not.toContain("setInterval");
    expect(listener).not.toContain("invoke");
    // One handler, one call.
    expect(listener.match(/scheduleTranscriptRehydrate\(/gu)?.length).toBe(1);

    // The known limitation stays a limitation on purpose: a pure wheel inside
    // Discord's own window moves nothing, so nothing reports it, and the renderer
    // does not go looking with a timer.
    expect(source).not.toContain("setInterval");

    // The read this edge drives is nobody's click, so a backend that starts
    // refusing it would otherwise be completely silent -- the eye would just keep
    // painting the previous rows. It fails closed exactly as before AND journals
    // the reason. See ./backend-failure.ts.
    const read = source.slice(
      source.indexOf("async function rehydrateNativeDiscordOverlayHistory"),
      source.indexOf("function syncTranscript()"),
    );
    expect(read).toContain('checkedBackendResponse(\n      "rehydrate_native_discord_overlay_history",');
    expect(read).toContain('recordBackendFailure("rehydrate_native_discord_overlay_history", error, [scope]);');
    expect(read).not.toMatch(/\}\s*catch\s*\{/u);
    // Fail-closed is unchanged: a refusal still answers null and paints nothing new.
    expect(read).toContain("return null;");
  });

  it("refuses a transcript read it cannot vouch for rather than painting somewhere wrong", () => {
    const source = readRelative("./overlay.ts");
    const rect = source.slice(source.indexOf("function parseDecodedDiscordRowRect"), source.indexOf("function parseRehydratedDiscordRow"));
    const transcriptParse = source.slice(source.indexOf("function parseRehydratedDiscordTranscript"), source.indexOf("async function rehydrateNativeDiscordOverlayHistory"));
    const run = source.slice(source.indexOf("async function runTranscriptRehydrate"), source.indexOf("function transcriptTimestamp"));

    // Exact keys, finite numbers, and the same bounds the just-sent carrier
    // binding is held to. A rectangle OSL cannot vouch for must never become
    // decrypted text painted over the wrong Discord row.
    expect(rect).toContain('exactKeys(value, ["leftPx", "topPx", "widthPx", "heightPx"])');
    expect(rect).toContain("rect.leftPx < 0 || rect.topPx < 0 || rect.widthPx < 1 || rect.heightPx < 12");
    // A throttle answer must be honest: a read that happened cannot also claim
    // time left on the floor.
    expect(transcriptParse).toContain("record.read === true && record.retryAfterMs !== 0");
    // A refused, failed or malformed read leaves the previous paint exactly as
    // it was, because OSL has no newer fact to paint.
    expect(run).toContain("if (!result) return;");
  });

  it("never renders cover prose of its own", () => {
    const source = readRelative("./overlay.ts");
    // With the eye off OSL paints nothing at all, so Discord's own row already
    // shows exactly what Discord has. There is nothing for this renderer to
    // reproduce and nothing it could get wrong, so the whole cover-prose store,
    // its unknown-cover notice, and the mode chooser are gone.
    expect(source).not.toContain("messageFlagtext");
    expect(source).not.toContain("UNKNOWN_FLAGTEXT_NOTICE");
    expect(source).not.toContain("displayedRowText");
    expect(source).not.toContain("result.flagtext");
  });

  it("keeps the plaintext out of every serialised payload and out of the hidden DOM", () => {
    const source = readRelative("./overlay.ts");
    expect(source).toContain("const messagePlaintext = new Map<string, string>();");
    // Nothing persists it, and no payload builder mentions plaintext beyond the
    // send command that has always taken it.
    expect(source).not.toMatch(/localStorage|sessionStorage|indexedDB/);
    expect(source.match(/plaintext(?=[,)])/gu)?.length).toBeGreaterThan(0);
    expect(source).not.toContain("dataset.plaintext");
    expect(source).not.toContain("setAttribute(\"data-plaintext\"");
    // It goes with the row, in the same statement as the row itself.
    expect(source).toContain("messagePlaintext.delete(key)");
  });

  it("leaves Discord's own history on screen instead of explaining a black band", () => {
    const source = readRelative("./overlay.ts");
    // The notice existed only because the opaque shield spanned the whole message
    // band, so an unrehydrated transcript read as a broken app. The shield is now
    // tied to the eye and clipped to the rows OSL paints, so the operator's real
    // history is simply visible and there is nothing to apologise for.
    expect(source).not.toContain("UNREHYDRATED_NOTICE_KEY");
    expect(source).not.toContain("showUnrehydratedNotice");
    expect(source).not.toContain("OSL is covering Discord's messages while protection is on");
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
    expect(schedule).toContain("shouldPollDiscordOverlay({");
    expect(schedule).toContain("decryptDisplayEnabled,");
    expect(poll).toContain("shouldPollDiscordOverlay({");
    expect(poll).toContain("decryptDisplayEnabled,");
    expect(shouldPollDiscordOverlay({
      overlayReady: true,
      decryptDisplayEnabled: false,
      documentHidden: false,
      discordQaShell: false,
    })).toBe(false);
    expect(source).not.toMatch(/localStorage|sessionStorage|indexedDB/);
  });

  it("names every received message and never swallows a receive failure", () => {
    const source = readRelative("./overlay.ts");
    const pollStart = source.indexOf("async function pollReceived(): Promise<void> {");
    expect(pollStart).toBeGreaterThan(-1);
    const poll = source.slice(pollStart, source.indexOf('document.addEventListener("visibilitychange"', pollStart));

    // The receive loop's own error used to vanish into a bare `catch {}` that only
    // doubled the poll interval, so the one failure the backend did return was
    // invisible. It now goes through the same journal as every other invoke.
    expect(poll).not.toMatch(/\}\s*catch\s*\{/u);
    expect(poll).toContain('recordBackendFailure("open_native_discord_overlay_text", error)');

    // A received message is addressable, so the backend surfacing one twice
    // cannot append a second bubble for the same text.
    expect(poll).toContain("if (incomingBubbles.has(message.messageId)) continue;");
    expect(poll).toContain("incomingBubbles.set(message.messageId, item);");
    // ...and the handle is dropped with the bubble that carried it.
    const remove = source.slice(source.indexOf("function removeBubble"), source.indexOf("function clearMessageBubbles"));
    expect(remove).toContain("incomingBubbles.delete(messageId)");
    expect(source).toContain("incomingBubbles.clear();");

    // The two states an empty batch used to hide are both reported, and neither
    // one short-circuits the receipts that keep flowing in both of them.
    expect(poll).toContain("if (!batch.decryptDisplayEnabled) {");
    expect(poll).toContain("if (batch.deferredRows > 0) {");
    expect(poll).toContain("|| batch.deferredRows > 0 || attachments.length > 0 ? 2_000");
    expect(poll).not.toMatch(/if \(!batch\.decryptDisplayEnabled\) \{[\s\S]{0,400}?\breturn\b/u);
    // Status text stays a fixed sentence plus a count -- never a fragment of what
    // arrived.
    expect(poll).toContain('status.textContent = "OSL could not reach the protected message store. Retrying.";');
    expect(poll).toContain('status.textContent = "Decrypted text is off for this conversation.";');
    expect(poll).not.toMatch(/status\.textContent = `[^`]*\$\{(?:message|opened)\.plaintext/u);
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

  it("anchors the protected composer to the measured native rectangle at the window bottom", () => {
    const css = readRelative("./overlay.css");
    // The transcript is hidden until the eye is on. Without explicit rows the
    // composer auto-places into the transcript row and paints as a strip at the
    // top of this deliberately tall window instead of over Discord's composer.
    expect(css).toMatch(/\.transcript-mount\s*\{[^}]*grid-row:\s*1;/su);
    expect(css).toMatch(/#write-pane\s*\{[^}]*grid-row:\s*2;/su);
    expect(css).toMatch(
      /:root\[data-discord-qa-shell="true"\]\s+#write-pane\s*\{[^}]*grid-row:\s*2;/su,
    );
    // The composer row is the exact measured rectangle, not a nominal minimum,
    // and the recipe width must never re-centre or shrink it.
    expect(css).toMatch(
      /:root\[data-native-composer-capture="true"\]\s+\.overlay-shell\s*\{[^}]*grid-template-rows:\s*minmax\(0, 1fr\) auto;/su,
    );
    expect(css).toMatch(
      /:root\[data-native-composer-capture="true"\]\s+#write-pane\s*\{[^}]*aspect-ratio:\s*var\(--osl-native-composer-aspect-ratio, auto\);/su,
    );
    expect(css).toMatch(
      /:root\[data-native-composer-capture="true"\]\s+\.composer-box\s*\{[^}]*max-width:\s*none;[^}]*margin-inline:\s*0;/su,
    );
    // The sampled native background must win over the QA fill -- and now does
    // so by construction rather than by a tie-breaking rule. Both composer-box
    // rules resolve the same `--osl-composer-fill` token, so the QA rule can no
    // longer restate a theme constant and beat the captured rule on source
    // order at equal specificity. The extra QA-only rule that existed purely to
    // tie the sampled colour back is therefore gone, and must not return.
    expect(css).not.toContain(
      ':root[data-discord-qa-shell="true"][data-native-composer-capture="true"] .composer-box',
    );
    expect(css).toMatch(
      /:root\[data-discord-qa-shell="true"\]\s+\.composer-box\s*\{[^}]*background:\s*var\(--osl-composer-fill\);/su,
    );
    // ...and there is exactly one inset one-pixel cyan outline.
    expect(css.match(/border:\s*1px solid rgba\(73, 214, 255, \.55\)/gu)).toHaveLength(1);
  });

  it("stops drawing a composer once the native window has vacated the composer band", () => {
    // THE PRODUCT MODEL. The lock is encryption only; the eye is the only control
    // over display. With the lock off and the eye on, the native window covers
    // Discord's transcript rows and vacates the composer band entirely, so
    // Discord's real message box is uncovered and taking keystrokes -- and OSL
    // must not paint its own composer into a window that no longer sits over one.
    // Until this rule existed OSL drew its composer box over the transcript there.
    const css = readRelative("./overlay.css");
    const source = readRelative("./overlay.ts");
    const native = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    expect(source).toContain(
      'const NATIVE_DISCORD_COMPOSER_BAND_SURRENDERED_EVENT = "osl://native-discord-composer-band-surrendered";',
    );
    expect(native).toContain(
      'const OVERLAY_COMPOSER_BAND_EVENT: &str = "osl://native-discord-composer-band-surrendered";',
    );
    // Emitted to OVERLAY_LABEL, so this renderer is the one that must listen.
    expect(native).toContain("OVERLAY_LABEL,\n                            OVERLAY_COMPOSER_BAND_EVENT,");
    expect(source).toMatch(
      /void listen<boolean>\(NATIVE_DISCORD_COMPOSER_BAND_SURRENDERED_EVENT, \(\{ payload \}\) => \{[\s\S]*?if \(typeof payload !== "boolean"\) return;\s*document\.documentElement\.dataset\.oslComposerBandSurrendered = String\(payload\);/u,
    );
    // A composer that is off screen holds no caret, exactly as the lock coming
    // down does not: the next raise is a fresh engagement and gets its own grant.
    expect(source).toMatch(
      /document\.documentElement\.dataset\.oslComposerBandSurrendered = String\(payload\);[\s\S]*?if \(payload\) caretGrantedForEngagement = false;/u,
    );
    expect(css).toMatch(
      /:root\[data-osl-composer-band-surrendered="true"\]\s+#write-pane\s*\{\s*display:\s*none;\s*\}/su,
    );
    // The vacated track goes to the transcript. Hiding the composer removes it
    // from the grid but not its track, and the base track has a 40px floor that
    // does not collapse -- so `.transcript-mount` would be shorter than the band
    // and, being `overflow: hidden` with absolutely positioned rows inside it,
    // would clip the bottom rows rather than rescale them.
    expect(css).toMatch(
      /:root\[data-osl-composer-band-surrendered="true"\]\s+\.transcript-mount\s*\{\s*grid-row:\s*1 \/ -1;\s*\}/su,
    );
    expect(css).toMatch(/\.transcript-mount\s*\{[^}]*overflow:\s*hidden;/su);
    // Keyed on the NATIVE fact and never on the lock. The banned
    // `data-osl-lock-engaged="false"` rule is the same state on this path and would
    // look equivalent; it is not, because it hid the composer while OSL's window
    // still covered Discord's message box, leaving an invisible surface that
    // swallowed every click aimed at the real box. A renderer that ships ahead of
    // its native half must keep drawing its composer rather than recreate that.
    // Read over the declarations only: the note below quotes the banned rule
    // verbatim in order to name it, and that citation must not read as the rule.
    const rules = css.replace(/\/\*[\s\S]*?\*\//gu, "");
    expect(rules).not.toContain("data-osl-lock-engaged");
    expect(source).not.toContain("dataset.oslComposerBandSurrendered = String(lockEngaged)");
    expect(source).not.toContain("dataset.oslComposerBandSurrendered = String(!lockEngaged)");
    const note = css.slice(0, css.indexOf(':root[data-osl-composer-band-surrendered="true"]'));
    expect(note).toContain("The lock may NEVER take the composer off screen.");
    expect(note).toContain("invisible surface that swallowed every");
    expect(note).toContain("vacated the composer band entirely");
    expect(note).toContain("Keying it on the lock is what would let");
    // The cyan ring is unaffected: it is still the engaged-state cue, and it is
    // still the only way the operator tells OSL's composer from Discord's.
    expect(css).toContain(".composer-box::after");
    expect(css).toContain("border: 1px solid rgba(73, 214, 255, .55)");
    expect(css).not.toMatch(/composer-band-surrendered[^{]*\.composer-box::after/u);
    // And no highlighted band behind the composer, which the owner has asked to
    // have removed twice.
    expect(css).not.toMatch(
      /:root\[data-osl-composer-band-surrendered="true"\][^{]*\{[^}]*background:/su,
    );
  });

  it("retains one warm protected WebView and resets it at every session boundary", () => {
    const native = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    const nativeMain = readRelative("../../osl-hub/src/main.rs");
    const source = readRelative("./overlay.ts");
    expect(native).toContain('const OVERLAY_SESSION_EVENT: &str = "osl://native-discord-overlay-session"');
    expect(native).toContain("pub(crate) fn prewarm(app: &tauri::AppHandle) -> Result<(), String>");
    expect(nativeMain).toContain("native_discord_overlay::prewarm(&prewarm_app)");
    // One window per label in every path. Tauri only registers a label after the
    // native window exists, so two callers that each pass their own existence
    // check both create a real HWND and the second silently orphans the first.
    // The pre-warm and the session path therefore share one construction gate.
    const nativeCore = native.split("#[cfg(test)]\nmod tests {")[0];
    expect(nativeCore).toContain("static PROTECTED_WINDOW_BUILD_LOCK: Mutex<()> = Mutex::new(());");
    expect((nativeCore.match(/WebviewWindowBuilder::new\(/gu) ?? []).length).toBe(2);
    expect((nativeCore.match(/ensure_retained_protected_window\(/gu) ?? []).length).toBe(5);
    // The page-load hook fires on both Started and Finished, so the pre-warm
    // worker must be gated to a single one of them.
    expect(nativeMain).toContain("matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)");
    // The lock toggle must hide the retained pair, never destroy and rebuild it.
    const hideStart = native.indexOf("fn hide_window(app: &tauri::AppHandle) {");
    const hideEnd = native.indexOf("\n/// Destroy the retained pair", hideStart);
    expect(hideStart).toBeGreaterThan(-1);
    expect(hideEnd).toBeGreaterThan(hideStart);
    expect(native.slice(hideStart, hideEnd)).not.toContain(".close()");
    // Nothing from an ended session may survive into the next one.
    expect(native).toContain("let _ = app.emit_to(OVERLAY_LABEL, OVERLAY_SESSION_EVENT, false);");
    expect(native).toContain("let _ = app.emit_to(OVERLAY_LABEL, OVERLAY_SESSION_EVENT, true);");
    expect(source).toContain("function discardProtectedSession(): void");
    expect(source).toContain('draft.value = "";');
    expect(source).toContain("void listen<boolean>(OVERLAY_SESSION_EVENT");
    // The post-show frameless enforcement stays exactly where it was measured,
    // now stated once in the single helper that both reveal sites go through.
    const restoreStart = native.indexOf("let composer_restored = composer_temporarily_hidden;");
    expect(restoreStart).toBeGreaterThan(-1);
    const restore = native.slice(restoreStart, restoreStart + 1_400);
    expect(restore).toContain("reveal_protected_pair(");
    const revealStart = native.indexOf("fn reveal_protected_pair(");
    expect(revealStart).toBeGreaterThan(-1);
    const reveal = native.slice(revealStart, revealStart + 1_400);
    expect(reveal.indexOf("window.show()")).toBeGreaterThan(-1);
    expect(reveal.indexOf("window.show()")).toBeLessThan(
      reveal.indexOf("enforce_native_frameless_overlay(window)?;"),
    );
    // Only that helper may reveal a protected window, so no path can reveal one
    // without the post-show enforcement or without something to paint.
    expect((nativeCore.match(/window\.show\(\)/gu) ?? []).length).toBe(1);
    expect(reveal.indexOf("native_surface_is_paintable(")).toBeLessThan(
      reveal.indexOf("window.show()"),
    );
    // The frame is no longer held by re-stripping style bits on a cadence: the
    // window procedure gives these windows no non-client area at all, so the
    // steady guard pass has nothing to correct and does not run one.
    expect(nativeCore).toContain("unsafe extern \"system\" fn protected_frameless_window_proc(");
    expect(nativeCore).toContain("if message == WM_NCCALCSIZE && wparam != 0 {");
    expect(
      (
        nativeCore.match(
          /enforce_native_frameless_overlay\(&?(?:window|shield)\)\?;/gu,
        ) ?? []
      ).length,
    ).toBeGreaterThanOrEqual(3);
    expect(native).toContain("apply_protected_frame_contract(&window, surface)");
    // A guard that stops for any reason, including a panic, cannot leave the
    // pair it owned on screen.
    expect(native).toContain("impl Drop for ProtectedWindowGuardOwnership {");
    expect(native).toContain("ProtectedWindowGuardOwnership::claim(&app, epoch);");
    expect(native).toContain("hide_protected_pair_or_destroy(&self.app);");
  });

  it("re-samples the native surface on the foreground-gated backstop so themes are picked up", () => {
    const native = readRelative("../../osl-hub/src/native_discord_overlay.rs");
    expect(native).toContain("let mut last_native_surface_refresh = Instant::now();");
    expect(native).toMatch(
      /let native_surface_backstop_due = periodic_backstop_refresh_due\(\s*osl_foreground,\s*carrier_in_flight,\s*last_native_surface_refresh\.elapsed\(\) >= NATIVE_SURFACE_BACKSTOP_INTERVAL,\s*\);/u,
    );
    // The backstop is now the ONLY blind re-sample trigger. A move or a drag
    // used to be one too, and because a capture needed the protected pair off
    // screen first, that made a drag one hide/reveal per frame -- the reported
    // disappearing composer. What is left is: no sample at all, the strip's
    // pixel dimensions actually changed, or this slow blind backstop.
    expect(native).toContain(
      "!has_sample || backstop_due || (shape_changed && geometry_settled)",
    );
    // And the capture may never take a visible composer off screen to run.
    expect(native).toContain(
      "fn resample_may_take_the_pair_off_screen(ever_revealed: bool, currently_hidden: bool) -> bool {",
    );
    expect(native).toContain("!ever_revealed || currently_hidden");
    expect(native).toContain("resample_may_take_the_pair_off_screen(\n                                ever_revealed,\n                                composer_temporarily_hidden,\n                            )");
    // The backstop adds screen sampling only; no extra accessibility work.
    const guardStart = native.indexOf("let native_surface_backstop_due");
    const guardEnd = native.indexOf("let composer_bounds = if refresh_composer_bounds", guardStart);
    expect(guardEnd).toBeGreaterThan(guardStart);
    expect(native.slice(guardStart, guardEnd)).not.toContain("refresh_verified_bounds");
  });

  it("gives a failed send its own persistent notice that outlives #overlay-status churn", () => {
    const source = readRelative("./overlay.ts");
    const html = readRelative("../overlay.html");

    // The new element exists, is adjacent to #overlay-status in the markup,
    // and is announced the same way main.ts's other failure notices are
    // (role="alert" -- not the "status"/"polite" role used for routine,
    // expected-to-be-overwritten text).
    expect(html).toContain('<output id="overlay-status" role="status" aria-live="polite">Checking protection…</output>');
    const statusIndex = html.indexOf('id="overlay-status"');
    const warningIndex = html.indexOf('id="overlay-send-warning"');
    expect(warningIndex).toBeGreaterThan(statusIndex);
    expect(warningIndex - statusIndex).toBeLessThan(200);
    expect(html).toMatch(/<output id="overlay-send-warning" role="alert" aria-live="assertive"[^>]*><\/output>/u);

    const sendDraftBody = source.slice(source.indexOf("async function sendDraft"), source.indexOf("prepare.addEventListener"));

    // Contract from the prior silent-failure fix: the early return on a failed
    // marker send must still come before the draft is ever cleared, with a
    // `return;` between them, so a failed send always keeps the user's text.
    const markerCheckIndex = sendDraftBody.indexOf("if (!markerSent) {");
    const markerReturnIndex = sendDraftBody.indexOf("return;", markerCheckIndex);
    const draftClearIndex = sendDraftBody.indexOf('draft.value = "";');
    expect(markerCheckIndex).toBeGreaterThan(-1);
    expect(markerReturnIndex).toBeGreaterThan(markerCheckIndex);
    expect(draftClearIndex).toBeGreaterThan(markerReturnIndex);

    // The failure text reaches the persistent element, not only the shared
    // transient status line.
    const failureBlock = sendDraftBody.slice(markerCheckIndex, sendDraftBody.indexOf("return;", markerCheckIndex) + "return;".length);
    expect(failureBlock).toContain("status.textContent = failureNotice;");
    expect(failureBlock).toContain("sendWarning.textContent = failureNotice;");
    expect(failureBlock.indexOf("status.textContent = failureNotice;")).toBeLessThan(
      failureBlock.indexOf("sendWarning.textContent = failureNotice;"),
    );

    // Exactly two clear sites, both explicit user actions, never a keystroke
    // and never a timer: (1) a fresh send attempt starting ("Encrypting…",
    // right after every early-return guard has already passed), and (2) that
    // attempt going on to succeed (beside the draft being wiped).
    const clearSites = [...sendDraftBody.matchAll(/sendWarning\.textContent = "";/gu)].map((m) => m.index ?? -1);
    expect(clearSites).toHaveLength(2);
    const encryptingIndex = sendDraftBody.indexOf('status.textContent = "Encrypting…";');
    expect(encryptingIndex).toBeGreaterThan(-1);
    expect(clearSites[0]).toBeGreaterThan(encryptingIndex);
    expect(clearSites[0]).toBeLessThan(markerCheckIndex);
    expect(clearSites[1]).toBeGreaterThan(draftClearIndex);
    // Never on a timer, and never merely because the user typed a character:
    // every mention of `sendWarning` in the whole file is either its one
    // declaration or inside the vetted sendDraft() body above -- nothing in
    // any setTimeout/setInterval, keydown/keyup/input listener, or any other
    // function ever references it.
    const sendWarningTouches = [...source.matchAll(/sendWarning\.[a-zA-Z]+/gu)].map((m) => m[0]);
    expect(sendWarningTouches.length).toBeGreaterThan(0);
    expect(sendWarningTouches.every((call) => call === "sendWarning.textContent")).toBe(true);
    const declarationIndex = source.indexOf("const sendWarning = requireElement");
    expect(declarationIndex).toBeGreaterThan(-1);
    const beforeSendDraft = source.slice(0, source.indexOf("async function sendDraft"));
    const afterSendDraft = source.slice(source.indexOf("prepare.addEventListener"));
    expect(beforeSendDraft.split("sendWarning")).toHaveLength(2); // exactly the one declaration
    expect(afterSendDraft).not.toContain("sendWarning");

    // Absolute constraint: only fixed literals plus the fixed carrier-status
    // enum label ever compose this message -- never the draft, the plaintext
    // variable, or any Discord row text.
    expect(failureBlock).not.toMatch(/sendWarning\.textContent = .*\bplaintext\b/u);
    expect(failureBlock).not.toMatch(/sendWarning\.textContent = .*draft\.value/u);
    expect(failureBlock).toContain("carrierStatusLabel is a fixed backend enum string");
  });

  it("adds a not-yet-opened caution that is honest about what OSL can and cannot know", () => {
    const source = readRelative("./overlay.ts");

    // The literal itself: a fact about our own acknowledgement ledger for
    // this overlay, plus a conditional (never an assertion) about the peer.
    const captionMatch = source.match(/const PROTECTED_SEND_CAUTION =\s*\n\s*"([^"]*)";/u);
    expect(captionMatch).not.toBeNull();
    const caution = captionMatch![1];
    expect(caution).toBe(
      "Nothing you've sent in this chat has been confirmed opened yet. If they aren't using OSL, they can't read any of it.",
    );

    // OSL can never know whether a Discord account has an OSL identity, so
    // the copy must never assert that the peer lacks OSL or can't decrypt --
    // only the conditional "if they aren't using OSL" is honest here. These
    // are exactly the phrasings a prior investigation ruled out.
    expect(caution).not.toMatch(/they (don't|do not|doesn't|does not) have osl/iu);
    expect(caution).not.toMatch(/they('| a)re not using osl/iu);
    expect(caution).not.toMatch(/they can('|no)t decrypt/iu);
    expect(caution).not.toMatch(/this person (doesn't|does not|isn't|is not) (have|using) osl/iu);
    // The one claim about the peer is strictly conditional.
    expect(caution).toMatch(/^Nothing you've sent .* if they aren't using osl, they can't read/iu);

    // The trigger is a pure function of the two session-scoped ledger flags,
    // never of draft/plaintext/transcript content, and takes no arguments
    // (so it structurally cannot be handed message text).
    const cautionFnIndex = source.indexOf("function protectedSendCaution(): string {");
    expect(cautionFnIndex).toBeGreaterThan(-1);
    const cautionFnBody = source.slice(cautionFnIndex, source.indexOf("}", cautionFnIndex) + 1);
    expect(cautionFnBody).toContain(
      "anyProtectedMessageSent && !anyProtectedMessageAcknowledged ? PROTECTED_SEND_CAUTION : \"\"",
    );
    expect(cautionFnBody).not.toMatch(/plaintext|draft\.value|messagePlaintext|transcriptRows/u);

    // "Sent" flips true only on the real compose-and-send success path in
    // sendDraft() (never the disposable F12 QA probe, and never anywhere the
    // flag could be set from typing or a poll).
    const sentFlagSites = [...source.matchAll(/anyProtectedMessageSent = true;/gu)].map((m) => m.index ?? -1);
    expect(sentFlagSites).toHaveLength(1);
    const sendDraftStart = source.indexOf("async function sendDraft");
    const sendDraftEnd = source.indexOf("prepare.addEventListener");
    expect(sentFlagSites[0]).toBeGreaterThan(sendDraftStart);
    expect(sentFlagSites[0]).toBeLessThan(sendDraftEnd);

    // "Acknowledged" flips true only inside applyAcknowledgment, which is
    // only ever invoked with the backend's own fixed "received"/"opened"
    // enum -- never with anything derived from message content.
    const ackFlagSites = [...source.matchAll(/anyProtectedMessageAcknowledged = true;/gu)].map((m) => m.index ?? -1);
    expect(ackFlagSites).toHaveLength(1);
    const applyAckStart = source.indexOf("function applyAcknowledgment(messageId: string, receipt:");
    expect(applyAckStart).toBeGreaterThan(-1);
    expect(ackFlagSites[0]).toBeGreaterThan(applyAckStart);
    expect(ackFlagSites[0]).toBeLessThan(applyAckStart + 400);
  });

  it("only shows the caution once something is sent and nothing has ever been acknowledged", () => {
    const source = readRelative("./overlay.ts");

    // The threshold is exactly: at least one send this conversation, and the
    // acknowledgement ledger has never advanced past Sent for any of them.
    const cautionFnIndex = source.indexOf("function protectedSendCaution(): string {");
    const cautionFnBody = source.slice(cautionFnIndex, source.indexOf("}", cautionFnIndex) + 1);
    expect(cautionFnBody).toMatch(/return anyProtectedMessageSent && !anyProtectedMessageAcknowledged/u);

    // Both flags are declared false, so a session that has sent nothing shows
    // nothing.
    expect(source).toMatch(/let anyProtectedMessageSent = false;/u);
    expect(source).toMatch(/let anyProtectedMessageAcknowledged = false;/u);

    // Both flags -- and only these two -- are reset where the rest of a
    // conversation's state is discarded, so a caution never leaks into the
    // next conversation.
    const discardStart = source.indexOf("function discardProtectedSession(): void {");
    expect(discardStart).toBeGreaterThan(-1);
    const discardBody = source.slice(discardStart, source.indexOf("\n}\n", discardStart));
    expect(discardBody).toContain("anyProtectedMessageSent = false;");
    expect(discardBody).toContain("anyProtectedMessageAcknowledged = false;");
  });

  it("never lets the not-yet-opened caution clobber a real send failure notice", () => {
    const source = readRelative("./overlay.ts");
    const sendDraftBody = source.slice(source.indexOf("async function sendDraft"), source.indexOf("prepare.addEventListener"));

    // The failure branch returns immediately after writing the failure
    // notice, before `protectedSendCaution()` is ever reached in this
    // attempt -- so a failure this attempt can never be overwritten by the
    // caution computed for a *later* attempt either, since each attempt
    // starts by clearing to "" and only reaches the caution site on success.
    const markerCheckIndex = sendDraftBody.indexOf("if (!markerSent) {");
    const failureReturnIndex = sendDraftBody.indexOf("return;", markerCheckIndex);
    expect(markerCheckIndex).toBeGreaterThan(-1);
    expect(failureReturnIndex).toBeGreaterThan(markerCheckIndex);
    const failureBlock = sendDraftBody.slice(markerCheckIndex, failureReturnIndex + "return;".length);
    expect(failureBlock).not.toContain("protectedSendCaution");

    // The caution write happens exactly once, strictly after that failure
    // branch's `return;`, at the same site that clears a stale failure
    // notice on confirmed success.
    const cautionCallSites = [...sendDraftBody.matchAll(/sendWarning\.textContent = protectedSendCaution\(\);/gu)]
      .map((m) => m.index ?? -1);
    expect(cautionCallSites).toHaveLength(1);
    expect(cautionCallSites[0]).toBeGreaterThan(failureReturnIndex);
    const successClearIndex = sendDraftBody.lastIndexOf('sendWarning.textContent = "";');
    expect(successClearIndex).toBeGreaterThan(failureReturnIndex);
    expect(cautionCallSites[0]).toBeGreaterThan(successClearIndex);

    // `protectedSendCaution()` is invoked as an actual call expression
    // (assigned somewhere) exactly once in the whole file.
    const wholeFileCalls = [...source.matchAll(/= protectedSendCaution\(\);/gu)];
    expect(wholeFileCalls).toHaveLength(1);
  });

  it("is never driven by a timer or a keystroke, only by an explicit send attempt", () => {
    const source = readRelative("./overlay.ts");

    // pollReceived() is the one function scheduled on a timer (via
    // scheduleReceivePoll/window.setTimeout). It updates the acknowledgement
    // flag indirectly through applyAcknowledgment (a real ledger fact, not a
    // timer firing on its own), but it must never itself touch the caution
    // element or call the caution function -- that only ever happens from
    // inside sendDraft(), on an explicit send.
    const pollStart = source.indexOf("async function pollReceived(): Promise<void> {");
    const pollEnd = source.indexOf('document.addEventListener("visibilitychange"', pollStart);
    expect(pollStart).toBeGreaterThan(-1);
    expect(pollEnd).toBeGreaterThan(pollStart);
    const pollBody = source.slice(pollStart, pollEnd);
    expect(pollBody).not.toContain("sendWarning");
    expect(pollBody).not.toContain("protectedSendCaution");
    expect(pollBody).not.toContain("anyProtectedMessageSent = true");

    // No draft/input/keydown/keyup listener anywhere in the file references
    // the caution machinery either.
    const listenerMatches = [...source.matchAll(/addEventListener\("(input|keydown|keyup)"[\s\S]{0,600}?\)\s*;/gu)];
    for (const match of listenerMatches) {
      expect(match[0]).not.toContain("sendWarning");
      expect(match[0]).not.toContain("protectedSendCaution");
      expect(match[0]).not.toContain("renderSendFailureBanner");
    }
  });

  it("paints the failed-send notice where a 736x58 protected window can actually show it", () => {
    const html = readRelative("../overlay.html");
    const css = readRelative("./overlay.css");

    // Why the announced element alone was never enough: it lives in
    // .composer-toolbar, and in the shipping captured-composer configuration
    // that toolbar is display:none outright -- on top of being flex-shrunk to
    // nothing in a window only 58 logical pixels tall.
    expect(css).toMatch(
      /:root\[data-native-composer-capture="true"\]\s+\.composer-toolbar\s*\{[^}]*display:\s*none;/su,
    );
    const toolbarIndex = html.indexOf('class="composer-toolbar"');
    expect(html.indexOf('id="overlay-send-warning"')).toBeGreaterThan(toolbarIndex);

    // The visible band is NOT in that toolbar. It is a child of .composer-box,
    // the one rectangle OSL paints, so it needs no window height that does not
    // exist -- the only surface guaranteed to have pixels at 736x58.
    const composerBoxIndex = html.indexOf('class="composer-box"');
    const bannerIndex = html.indexOf('id="overlay-send-failure"');
    expect(composerBoxIndex).toBeGreaterThan(-1);
    expect(bannerIndex).toBeGreaterThan(composerBoxIndex);
    expect(bannerIndex).toBeLessThan(toolbarIndex);
    expect(html).toContain('<div id="overlay-send-failure" class="send-failure" hidden>');
    expect(html).toContain('id="overlay-send-failure-text"');
    expect(html).toContain('id="overlay-send-failure-dismiss"');

    // It is a band across the top of that rectangle, sized by a fixed variable
    // small enough to leave the operator's own line of text under it.
    expect(css).toMatch(/--osl-send-failure-band:\s*22px;/u);
    expect(css).toMatch(
      /\.send-failure\s*\{[^}]*position:\s*absolute;[^}]*height:\s*var\(--osl-send-failure-band\);/su,
    );
    for (const declaration of ["top: 0;", "right: 0;", "left: 0;"]) {
      expect(css.slice(css.indexOf(".send-failure {"), css.indexOf(".send-failure__badge"))).toContain(declaration);
    }

    // Layered under the cyan lock ring, never over it: "protection is on" has
    // to stay legible on top of a failure notice.
    expect(css).toMatch(/\.send-failure\s*\{[^}]*z-index:\s*2;/su);
    expect(css).toMatch(/\.composer-box::after\s*\{[^}]*z-index:\s*3;/su);
    expect(css.match(/border:\s*1px solid rgba\(73, 214, 255, \.55\)/gu)).toHaveLength(1);

    // The band is an overlay, so both composer layouts hand it rows explicitly
    // rather than letting it sit on top of the user's real typed words. Flow
    // layout moves with padding; the captured layout is absolutely positioned
    // against the padding box, so padding cannot move it and its measured top
    // is re-anchored instead -- left/width untouched, so the draft keeps its
    // horizontal alignment with Discord's own text.
    expect(css).toMatch(
      /:root\[data-osl-send-failure="true"\]\s+\.composer-box\s*\{[^}]*padding-top:\s*var\(--osl-send-failure-band\);/su,
    );
    expect(css).toMatch(
      /:root\[data-osl-send-failure="true"\]\[data-native-composer-capture="true"\]\s+\.draft-field\s*\{[^}]*top:\s*max\(var\(--osl-native-edit-top, 0px\), var\(--osl-send-failure-band\)\);/su,
    );
    const reanchor = css.slice(
      css.indexOf(':root[data-osl-send-failure="true"][data-native-composer-capture="true"] .draft-field'),
    );
    expect(reanchor.slice(0, reanchor.indexOf("}"))).not.toContain("left:");
    expect(reanchor.slice(0, reanchor.indexOf("}"))).not.toContain("width:");

    // The load-bearing transparency is untouched: nothing new paints outside
    // .composer-box, and the shell keeps no colour of its own.
    expect(css).toMatch(/\.overlay-shell\s*\{[^}]*background:\s*transparent;/su);
    const bandRules = [...css.matchAll(/^([^\n{]*\.send-failure[^\n{]*)\{/gmu)].map((m) => m[1]);
    expect(bandRules.length).toBeGreaterThan(0);
    for (const selector of bandRules) {
      expect(selector).not.toMatch(/\.overlay-shell|\btranscript-mount\b|^\s*(html|body)\b/u);
    }
  });

  it("drives the visible band from the same send-only sites as the announced notice", () => {
    const source = readRelative("./overlay.ts");
    const sendDraftStart = source.indexOf("async function sendDraft");
    const sendDraftBody = source.slice(sendDraftStart, source.indexOf("prepare.addEventListener"));

    // Exactly four sites inside sendDraft(): a fresh attempt starting, the
    // marker-check failure, the confirmed success, and the thrown-failure
    // catch -- the same explicit-send-only contract the announced notice has.
    const bandSites = [...sendDraftBody.matchAll(/renderSendFailureBanner\(([^)]*)\);/gu)];
    expect(bandSites).toHaveLength(4);
    for (const [, argument] of bandSites) {
      expect(['""', "failureNotice", "stoppedNotice"]).toContain(argument);
    }

    const encryptingIndex = sendDraftBody.indexOf('status.textContent = "Encrypting…";');
    const markerCheckIndex = sendDraftBody.indexOf("if (!markerSent) {");
    const failureReturnIndex = sendDraftBody.indexOf("return;", markerCheckIndex);
    const draftClearIndex = sendDraftBody.indexOf('draft.value = "";');
    const clearIndexes = bandSites
      .filter((match) => match[1] === '""')
      .map((match) => match.index ?? -1);
    expect(clearIndexes).toHaveLength(2);
    // (1) A genuine new attempt takes the band down, after every early-return
    // guard has passed and before the failure can be re-raised.
    expect(clearIndexes[0]).toBeGreaterThan(encryptingIndex);
    expect(clearIndexes[0]).toBeLessThan(markerCheckIndex);
    // (2) A confirmed success takes it down beside the draft being wiped, so a
    // success never leaves a failure standing on the composer.
    expect(clearIndexes[1]).toBeGreaterThan(draftClearIndex);

    // The failure is raised inside the failure branch, immediately after the
    // announced copy and still before the `return;` that protects the draft.
    const failureBlock = sendDraftBody.slice(markerCheckIndex, failureReturnIndex + "return;".length);
    expect(failureBlock).toContain("sendWarning.textContent = failureNotice;");
    expect(failureBlock).toContain("renderSendFailureBanner(failureNotice);");
    expect(failureBlock.indexOf("sendWarning.textContent = failureNotice;")).toBeLessThan(
      failureBlock.indexOf("renderSendFailureBanner(failureNotice);"),
    );

    // The thrown-failure branch is the same defect one branch over, and is now
    // told the same two ways.
    const catchIndex = sendDraftBody.indexOf("} catch {");
    expect(catchIndex).toBeGreaterThan(draftClearIndex);
    const catchBlock = sendDraftBody.slice(catchIndex, sendDraftBody.indexOf("} finally {", catchIndex));
    expect(catchBlock).toContain(
      'const stoppedNotice = "Protection stopped safely. Nothing was sent. Your draft is still here.";',
    );
    expect(catchBlock).toContain("sendWarning.textContent = stoppedNotice;");
    expect(catchBlock).toContain("renderSendFailureBanner(stoppedNotice);");

    // The success path never raises the band, and the softer not-yet-opened
    // caution never reaches it: a band that seized composer rows after every
    // successful send would train the operator to swat it away.
    expect(source).not.toContain("renderSendFailureBanner(protectedSendCaution())");
    const successTail = sendDraftBody.slice(clearIndexes[1] + 1, sendDraftBody.indexOf("} catch {"));
    expect(successTail).not.toContain("renderSendFailureBanner(");
    expect(successTail).toContain("protectedSendCaution()");
    expect(sendDraftBody).toContain("sendWarning.textContent = protectedSendCaution();");

    // Everywhere else in the file: the declaration, one definite initial state,
    // and the operator's own dismissal. Never a timer, never a keystroke.
    const outside = [
      ...source.slice(0, sendDraftStart).matchAll(/renderSendFailureBanner\(([^)]*)\)/gu),
      ...source.slice(source.indexOf("prepare.addEventListener")).matchAll(/renderSendFailureBanner\(([^)]*)\)/gu),
    ];
    expect(outside.map((match) => match[1])).toEqual(['notice: string', '""', '""']);
    expect(source).toMatch(
      /failureBannerDismiss\.addEventListener\("click", \(\) => \{\s*renderSendFailureBanner\(""\);\s*\}\);/u,
    );
    const pollStart = source.indexOf("async function pollReceived(): Promise<void> {");
    const pollBody = source.slice(pollStart, source.indexOf('document.addEventListener("visibilitychange"', pollStart));
    expect(pollBody).not.toContain("renderSendFailureBanner");
    for (const [timer] of source.matchAll(/setTimeout\([\s\S]{0,400}?\)\s*;/gu)) {
      expect(timer).not.toContain("renderSendFailureBanner");
    }
  });

  it("never lets a draft byte reach the visible failure band", () => {
    const source = readRelative("./overlay.ts");
    const html = readRelative("../overlay.html");

    // The band renders exactly what it is handed, into a text node, and every
    // caller hands it a fixed literal (or the fixed carrier-status enum label
    // already vetted for the announced notice). No draft, plaintext, transcript
    // row or thrown error can reach it, and nothing is logged or persisted.
    const bannerFnIndex = source.indexOf("function renderSendFailureBanner(notice: string): void {");
    expect(bannerFnIndex).toBeGreaterThan(-1);
    const bannerFnBody = source.slice(bannerFnIndex, source.indexOf("\n}", bannerFnIndex));
    expect(bannerFnBody).toContain("failureBannerText.textContent = notice;");
    expect(bannerFnBody).not.toMatch(/plaintext|draft\.value|messagePlaintext|transcriptRows|innerHTML/u);
    expect(bannerFnBody).not.toMatch(/console\.|localStorage|sessionStorage|fetch\(|invoke\(/u);
    expect(source).not.toContain("failureBannerText.innerHTML");

    // Only ever .textContent and .hidden on those nodes -- no other property is
    // ever assigned from a notice.
    const bannerTouches = [...source.matchAll(/failureBanner(?:Text|Dismiss)?\.[a-zA-Z]+/gu)].map((m) => m[0]);
    expect(bannerTouches.length).toBeGreaterThan(0);
    expect(bannerTouches.every((touch) => [
      "failureBannerText.textContent",
      "failureBanner.hidden",
      "failureBannerDismiss.addEventListener",
    ].includes(touch))).toBe(true);

    // Nothing is baked into the markup either: the text node ships empty and
    // the only fixed copy is the severity badge.
    expect(html).toMatch(/<span id="overlay-send-failure-text" class="send-failure__text"><\/span>/u);
    expect(html).toContain('<span class="send-failure__badge">Not sent</span>');

    // The band is a paint, not a second live region: the failure is announced
    // once, by the existing role="alert" element.
    const bannerMarkup = html.slice(
      html.indexOf('<div id="overlay-send-failure"'),
      html.indexOf("</div>", html.indexOf('<div id="overlay-send-failure"')),
    );
    expect(bannerMarkup).not.toContain("aria-live");
    expect(bannerMarkup).not.toContain('role="alert"');
    expect(bannerMarkup).toContain('aria-label="Dismiss this failed-send notice"');
    expect((html.match(/role="alert"/gu) ?? []).length).toBe(1);
  });

  it("uses every measured font metric on its own instead of withholding all four", () => {
    // The all-four-or-nothing guard was a restriction with no upside. The four
    // measurements are independent, so a family name that came back unusable
    // used to discard a perfectly good measured size -- and "discard" here does
    // not mean "render nothing", it means fall through to the hardcoded 14px in
    // overlay.css. Partial application replaces a guess with a measurement on
    // every property OSL actually knows.
    const source = readRelative("./overlay.ts");
    const start = source.indexOf("function applyNativeSurfaceCapture(");
    expect(start).toBeGreaterThan(-1);
    const body = source.slice(start, source.indexOf("\n}\n", start));
    expect(body).not.toMatch(
      /capture\.fontFamily !== null\s*\n?\s*&&\s*capture\.fontSizePx !== null/u,
    );
    for (const [guard, declaration] of [
      ["capture.fontFamily !== null", '"--osl-native-edit-font-family", JSON.stringify(capture.fontFamily)'],
      ["capture.fontSizePx !== null", '"--osl-native-edit-font-size", `${capture.fontSizePx}px`'],
      ["capture.fontWeight !== null", '"--osl-native-edit-font-weight", String(capture.fontWeight)'],
      ["capture.lineHeightPx !== null", '"--osl-native-edit-line-height", `${capture.lineHeightPx}px`'],
    ]) {
      expect(body).toContain(`if (${guard}) {`);
      expect(body).toContain(declaration);
    }
    // Verbatim: no rounding, no flooring, no unit gymnastics between the
    // measurement and the custom property.
    expect(body).not.toMatch(/Math\.(?:round|floor|ceil|max|min)\([^)]*capture\.(?:fontSizePx|lineHeightPx)/u);
    // A property that stops being measured is cleared, not left stale from the
    // previous capture: the removal loop runs before any of these writes.
    expect(body.indexOf("root.style.removeProperty(name)"))
      .toBeLessThan(body.indexOf('"--osl-native-edit-font-size"'));
  });

  it("never fills the composer with a guessed Discord theme colour", () => {
    // The reported "lighter highlight rectangle" behind the placeholder. The
    // fill came from `--osl-composer-bg`, which is Discord's DEFAULT dark-theme
    // composer colour arriving by two routes: a literal in overlay.css, and
    // discordVisualCssVariables() re-deriving the same constant from a
    // four-entry theme-pack table that every custom Discord theme collapses
    // onto. On a near-black theme that is a visibly lighter slab painted inside
    // the operator's real message box.
    const css = readRelative("./overlay.css");
    const source = readRelative("./overlay.ts");
    const declarations = css.replace(/\/\*[\s\S]*?\*\//gu, "");
    // The constants are gone from this sheet entirely, not merely unreferenced.
    expect(declarations).not.toContain("#383a40");
    expect(declarations).not.toContain("#313338");
    expect(declarations).not.toContain("--osl-composer-bg");
    expect(declarations).not.toContain("--osl-overlay-bg");
    // Measured, or nothing. `transparent` is not a degraded mode here: this
    // window sits directly over Discord's real composer, so painting nothing
    // shows Discord's own colour through exactly.
    expect(css).toContain("--osl-composer-fill: var(--osl-native-edit-background, transparent);");
    const fills = [...declarations.matchAll(/\.composer-box\s*\{[^}]*background:\s*([^;]+);/gsu)];
    expect(fills).not.toHaveLength(0);
    for (const [, value] of fills) expect(value.trim()).toBe("var(--osl-composer-fill)");
    // Nor may the recipe put the guess back on :root behind the sheet's back.
    expect(source).toContain("const GUESSED_SURFACE_FILL_VARIABLES: ReadonlySet<string> = new Set([");
    expect(source).toContain('"--osl-overlay-bg",');
    expect(source).toContain('"--osl-composer-bg",');
    expect(source).toContain("if (GUESSED_SURFACE_FILL_VARIABLES.has(name)) continue;");
    // The sampled colour itself is still applied verbatim, and cleared with the
    // rest of the capture so a stale sample cannot outlive its surface.
    expect(source).toContain('root.style.setProperty("--osl-native-edit-background", capture.inputBackground)');
    expect(source).toContain('"--osl-native-edit-background",');
    // A transparent fill means Discord's own live placeholder is visible
    // through the composer, so OSL must not paint a second copy of it a few
    // pixels off. The hiding rule is therefore unconditional, not gated to the
    // captured-composer path, and the exact prompt survives as the aria-label.
    expect(css).toContain(".draft-field textarea::placeholder { color: transparent; }");
    expect(css).not.toContain(
      ':root[data-native-composer-capture="true"] .draft-field textarea::placeholder',
    );
    expect(source).toContain('draft.setAttribute("aria-label", exactDiscordPlaceholder)');
    // The shell stays transparent regardless; an opaque shell is never the fix.
    expect(css).toMatch(/\.overlay-shell\s*\{[^}]*background:\s*transparent;/su);
    // The cyan lock ring is untouched by all of this.
    expect(css).toContain(".composer-box::after");
    expect(css).toContain("border: 1px solid rgba(73, 214, 255, .55)");
  });

  it("re-measures the Discord surface without a restart on every trigger that can invalidate it", () => {
    // Self-heal. Typography, the sampled composer fill and every carrier row
    // rectangle are measurements taken at one instant; a Discord zoom step, a
    // DPI change, a monitor move, a theme switch or a re-mount invalidates them
    // all at once, and a stale measurement is the visible seam this surface
    // exists to avoid. This used to be gated to the QA shell, so the shipping
    // build had no way back to a correct measurement short of a restart.
    const source = readRelative("./overlay.ts");
    const start = source.indexOf("function scheduleNativeSurfaceHeal(): void {");
    expect(start).toBeGreaterThan(-1);
    const heal = source.slice(start, source.indexOf("\n}\n", start));
    // Drop the paint first, then ask the backend to prove the surface again --
    // in the window before the answer arrives OSL must paint nothing rather
    // than paint the wrong thing.
    expect(heal).toContain("applyVerifiedCarrierRows([]);");
    expect(heal).toContain("void refreshProtectedDisplayVisibility();");
    expect(heal.indexOf("applyVerifiedCarrierRows([]);"))
      .toBeLessThan(heal.indexOf("void refreshProtectedDisplayVisibility();"));
    // Coalesced, and never queued behind itself: resize fires per frame and
    // this ends in an IPC round trip.
    expect(heal).toContain("if (surfaceHealTimer !== undefined) window.clearTimeout(surfaceHealTimer);");
    // ...but a trailing-edge coalescer that is re-armed on every event is
    // STARVABLE, and a window drag is exactly a burst that does not stop. The
    // first deferred heal fixes a deadline; past it the armed timer is left to
    // fire rather than deferred again, so the burst costs one round trip per
    // interval instead of never healing at all -- and still not one per event.
    expect(source).toContain("const NATIVE_SURFACE_HEAL_MAX_DEFER_MS = 600;");
    expect(heal).toContain("surfaceHealDeferralDeadline = now + NATIVE_SURFACE_HEAL_MAX_DEFER_MS;");
    expect(heal).toContain("} else if (now + NATIVE_SURFACE_HEAL_DELAY_MS > surfaceHealDeferralDeadline) {");
    expect(heal).toMatch(/surfaceHealDeferralDeadline\) \{[\s\S]*?return;/);
    // The deadline is armed only on the leading edge of a burst, never reset by
    // the events that follow -- resetting it would restore the starvation.
    expect(heal).toContain("if (surfaceHealTimer === undefined) {");
    expect((heal.match(/surfaceHealDeferralDeadline = /gu) ?? []).length).toBe(1);

    // Every trigger, in every build -- none of these sit behind discordQaShell.
    for (const registration of [
      'window.addEventListener("resize", scheduleNativeSurfaceHeal);',
      'window.addEventListener("focus", scheduleNativeSurfaceHeal);',
      "if (!document.hidden) scheduleNativeSurfaceHeal();",
      "window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)",
      "void document.fonts.ready.then(() => {",
    ]) {
      expect(source).toContain(registration);
      const qaGate = source.lastIndexOf("if (discordQaShell) {", source.indexOf(registration));
      const qaGateEnd = qaGate < 0 ? -1 : source.indexOf("\n}\n", qaGate);
      expect(qaGateEnd).toBeLessThan(source.indexOf(registration));
    }
    // The resolution query only fires while it disagrees with the current
    // ratio, so it is re-armed against the new value instead of left matching
    // a stale one.
    expect(source).toContain('query.removeEventListener("change", onChange);');
    expect(source).toContain("watchDevicePixelRatio();");

    // A fresh capture re-applies every variable rather than writing them once:
    // the removal loop and the writes both live in applyNativeSurfaceCapture,
    // and every path that learns of a change calls it again.
    expect(source).toContain("for (const name of nativeSurfaceCssVariables) root.style.removeProperty(name);");
    for (const caller of [
      /listen<void>\(NATIVE_SURFACE_CHANGED_EVENT, \(\) => \{\s*void refreshProtectedDisplayVisibility\(\);/u,
      /async function refreshProtectedDisplayVisibility[\s\S]*?applyNativeSurfaceCapture\(state\.nativeSurface\)/u,
      /async function initializeOverlay[\s\S]*?applyNativeSurfaceCapture\(state\.nativeSurface\)/u,
    ]) expect(source).toMatch(caller);
  });
});
