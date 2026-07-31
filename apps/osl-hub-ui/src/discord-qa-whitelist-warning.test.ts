import { describe, expect, it } from "vitest";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function region(startNeedle: string, endNeedle: string): string {
  const start = source.indexOf(startNeedle);
  expect(start).toBeGreaterThan(-1);
  const end = source.indexOf(endNeedle, start + startNeedle.length);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

const headerControls = region(
  "function nativeDiscordHeaderControls()",
  "function trustedHeader()",
);

// This block sits above the composerRefusal region (which renderLock in
// discord-qa-lock-refusal.test.ts evaluates with a fixed parameter list), so
// it is isolated on purpose: it only ever touches
// nativeDiscordProtectionActive, verifiedPeer and scopeApproved.
const whitelistWarningBlock = region(
  "  const whitelistWarningNotice = nativeDiscordProtectionActive && verifiedPeer && !scopeApproved",
  "\n  const transcriptVisible = peerProtectedSheet.decryptDisplayEnabled;",
);

/**
 * Evaluate the shipped whitelist-warning markup block, so the trigger
 * condition and copy are asserted against the real template instead of a
 * copy of it.
 */
function renderWhitelistWarning(input: {
  protectionActive: boolean;
  verifiedPeer: boolean;
  scopeApproved: boolean;
}): string {
  const build = new Function(
    "nativeDiscordProtectionActive",
    "verifiedPeer",
    "scopeApproved",
    `${whitelistWarningBlock}\nreturn whitelistWarningNotice;`,
  ) as (protectionActive: boolean, verifiedPeer: boolean, scopeApproved: boolean) => string;
  return build(input.protectionActive, input.verifiedPeer, input.scopeApproved);
}

describe("Discord QA whitelist revoke warning", () => {
  it("shows only when protection is active, the peer is verified, and the scope is not approved", () => {
    const combinations = [
      { protectionActive: false, verifiedPeer: false, scopeApproved: false },
      { protectionActive: false, verifiedPeer: false, scopeApproved: true },
      { protectionActive: false, verifiedPeer: true, scopeApproved: false },
      { protectionActive: false, verifiedPeer: true, scopeApproved: true },
      { protectionActive: true, verifiedPeer: false, scopeApproved: false },
      { protectionActive: true, verifiedPeer: false, scopeApproved: true },
      { protectionActive: true, verifiedPeer: true, scopeApproved: true },
    ];
    for (const combo of combinations) {
      expect(renderWhitelistWarning(combo)).toBe("");
    }
    const shown = renderWhitelistWarning({ protectionActive: true, verifiedPeer: true, scopeApproved: false });
    expect(shown).not.toBe("");
  });

  it("carries the id, role=status, and a state attribute a QA probe can read without guessing at colour", () => {
    const shown = renderWhitelistWarning({ protectionActive: true, verifiedPeer: true, scopeApproved: false });
    expect(shown).toContain('id="discord-qa-whitelist-warning"');
    expect(shown).toContain('role="status"');
    expect(shown).toContain('data-whitelist-state="revoked"');
  });

  it("states the real fail-closed consequence and never claims to know about the other person's OSL account", () => {
    const shown = renderWhitelistWarning({ protectionActive: true, verifiedPeer: true, scopeApproved: false });
    expect(shown).toContain("Encryption revoked for this chat");
    expect(shown).toContain("sends will fail until you allow it again");
    // Pressing + is how the operator gets back to a working state.
    expect(shown).toContain("Press the + button to allow this chat again");
    // OSL cannot know whether the other person has an OSL account at all;
    // the copy must never imply otherwise.
    expect(shown.toLowerCase()).not.toContain("osl account");
    expect(shown.toLowerCase()).not.toContain("they don't have");
    expect(shown.toLowerCase()).not.toContain("they do not have");
  });

  it("is a fixed literal — never interpolated draft, message, or other user-typed content", () => {
    // No template interpolation at all in the block that builds the chip:
    // every character of the markup is a literal, not a `${...}` splice.
    expect(whitelistWarningBlock).not.toContain("${");
    expect(whitelistWarningBlock).not.toContain("peerProtectedSheet");
    expect(whitelistWarningBlock).not.toContain("draft");
    expect(whitelistWarningBlock).not.toContain("escapeHtml");
  });

  it("is emitted in the header controls strip alongside its sibling chips", () => {
    expect(headerControls).toContain("${whitelistWarningNotice}");
    expect(headerControls).toContain(
      '${composerRefusalNotice}${transcriptNotice}${transcriptVisibilityControl}${composerControl}${whitelistWarningNotice}',
    );
    expect(headerControls).toContain('class="native-discord-header-controls discord-qa-header-controls"');
  });

  it("matches the house style of its sibling status chips", () => {
    // Asserted in styles.css, where the styling now lives and where it is the
    // only copy. The shipped CSP (tauri.conf.json, app.security.csp) is
    // `style-src 'self'` with no `'unsafe-inline'`, no nonce and no hash, and
    // CSP style-src governs inline style *attributes* as well as <style>
    // blocks -- so the inline copy this used to assert on was dropped by the
    // WebView and styled nothing. Reading it out of main.ts therefore reported
    // green while the chip rendered as bare unstyled text.
    const styles = fs.readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    const declarations = styles.replace(/\/\*[\s\S]*?\*\//gu, "");
    // The shared chip rule the four header-strip chips are declared against.
    const chipStart = declarations.indexOf("\n.native-discord-composer-unreachable,\n");
    expect(chipStart, "the shared header-strip chip rule should exist").toBeGreaterThanOrEqual(0);
    const chipRule = declarations.slice(chipStart, declarations.indexOf("}", chipStart));
    expect(chipRule).toContain(".discord-qa-whitelist-warning,");
    expect(chipRule).toContain("border: 1px solid currentColor;");
    expect(chipRule).toContain("border-radius: 7px;");
    // Its own tone, taken from the palette token rather than a loose hex.
    const ownStart = declarations.indexOf("\n.discord-qa-whitelist-warning {");
    expect(ownStart, ".discord-qa-whitelist-warning should be a top-level rule").toBeGreaterThanOrEqual(0);
    const ownRule = declarations.slice(ownStart, declarations.indexOf("}", ownStart));
    expect(ownRule).toContain("color: var(--warn);");
    // And the CSP-dead copy may not come back.
    expect(whitelistWarningBlock).not.toContain("style=");
  });
});
