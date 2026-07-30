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
    expect(whitelistWarningBlock).toContain('color:#ffb347');
    expect(whitelistWarningBlock).toContain("border:1px solid currentColor;border-radius:7px");
  });
});
