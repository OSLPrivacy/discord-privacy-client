import fs from "node:fs";
import { describe, expect, it } from "vitest";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function section(startMarker: string, endMarker: string): string {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start + startMarker.length);
  expect(start).toBeGreaterThan(-1);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("manual Discord QA composer", () => {
  it("wires the visible composer lock without starting P2P", () => {
    const controls = section(
      "function nativeDiscordHeaderControls()",
      "function trustedHeader()",
    );
    const bindings = section(
      'document.querySelector<HTMLButtonElement>("#discord-qa-run-test")',
      'document.querySelectorAll<HTMLButtonElement>("[data-open-burn]")',
    );

    expect(controls).toContain('id="discord-qa-toggle-composer"');
    expect(controls).toContain('nativeDiscordProtectionActive ? "locked" : "unlocked"');
    expect(controls).toContain('aria-pressed="${nativeDiscordProtectionActive}"');
    expect(controls).toContain('aria-label="${composerProtectionLabel}"');
    expect(controls).toContain('title="${composerProtectionLabel}"');
    expect(controls).toContain('"Protected composer on — close"');
    expect(controls).toContain('"Protected composer off — open"');
    expect(controls).toContain('nativeDiscordProtectionActive ? "M8 10V7a4 4 0 0 1 8 0v3" : "M8 10V7a4 4 0 0 1 7.7-1.5"');
    expect(bindings).toContain(
      'document.querySelector<HTMLButtonElement>("#discord-qa-toggle-composer")',
    );
    expect(bindings).toContain("void toggleDiscordQaComposer()");
    expect(controls).not.toContain('id="discord-qa-run-test"');
  });

  it("lets the native overlay command reconcile a lost long-running host response", () => {
    const reconcile = section(
      "async function ensureDiscordQaNativeHost(",
      "async function openDiscordQaComposer(",
    );
    const openComposer = section(
      "async function openDiscordQaComposer(",
      "async function runDiscordQaOneClick()",
    );
    const solePeerOverlay = section(
      "async function openSoleVerifiedDiscordQaOverlay()",
      "function showLocalProtectedChoice()",
    );

    expect(reconcile).toContain("resizeNativeAppWindow()");
    expect(reconcile).toContain('recovered.id !== "discord"');
    expect(reconcile).toContain(
      'recovered.mode !== "existingNativeCompanion"',
    );
    expect(reconcile).toContain('activeNativeHostMode = "existingSession"');

    expect(openComposer).toContain('activeHomeAppId === "discord"');
    expect(openComposer).toContain('activeNativeHostId = "discord"');
    expect(openComposer).toContain('activeNativeHostMode = "existingSession"');
    expect(openComposer).toContain(
      "await ensureDiscordQaNativeHost(reconcileDeadlineMs)",
    );
    expect(openComposer.indexOf("await openSoleVerifiedDiscordQaOverlay()"))
      .toBeGreaterThan(openComposer.indexOf('activeNativeHostId = "discord"'));

    expect(solePeerOverlay).toContain(
      "const verifiedStablePeers = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange)",
    );
    expect(solePeerOverlay).toContain("verifiedStablePeers.length !== 1");
    expect(solePeerOverlay).toContain(
      "await openNativeDiscordProtection(solePeer.personId)",
    );
    expect(solePeerOverlay).toContain(
      "peerProtectedSheet.context?.personId === solePeer.personId",
    );
    expect(solePeerOverlay).not.toContain(
      "const activeToken = nativeDiscordProtectionActive",
    );
    expect(solePeerOverlay).toContain("nativeDiscordProtectionActive = true");
  });

  it("never sends or polls from the manual composer action", () => {
    const openComposer = section(
      "async function openDiscordQaComposer(",
      "async function runDiscordQaOneClick()",
    );

    expect(openComposer).not.toContain("runNativeDiscordHeadlessQa");
    expect(openComposer).not.toContain("pollNativeDiscordHeadlessQa");
    expect(openComposer).not.toContain("startDiscordQaVisualOverlayAttempt");
  });

  it("retries automatic composer opening boundedly without starting P2P", () => {
    const autoOpen = section(
      "async function openDiscordQaComposerAfterHostReady()",
      "async function runDiscordQaOneClick()",
    );

    expect(source).toContain("const discordQaAutoComposerMaxAttempts = 3");
    expect(autoOpen).toContain(
      "attempt < discordQaAutoComposerMaxAttempts && !nativeDiscordProtectionActive",
    );
    expect(autoOpen).toContain("window.setTimeout(resolve, 750)");
    expect(autoOpen.indexOf("window.setTimeout(resolve, 750)"))
      .toBeLessThan(autoOpen.indexOf('await openDiscordQaComposer(15_000, "automatic")'));
    expect(autoOpen.slice(
      autoOpen.indexOf("window.setTimeout(resolve, 750)"),
      autoOpen.indexOf('await openDiscordQaComposer(15_000, "automatic")'),
    )).toContain('activeNativeHostMode !== "existingSession"');
    expect(autoOpen).toContain('await openDiscordQaComposer(15_000, "automatic")');
    expect(autoOpen).toContain('route !== "service"');
    expect(autoOpen).toContain('activeNativeHostId !== "discord"');
    expect(autoOpen).toContain('activeNativeHostMode !== "existingSession"');
    expect(autoOpen).toContain("window.setTimeout(resolve, 500)");
    expect(autoOpen).not.toContain("runNativeDiscordHeadlessQa");
    expect(autoOpen).not.toContain("pollNativeDiscordHeadlessQa");
  });
});

describe("operator-only composer lock disabling (flicker fix)", () => {
  it("(a) never lets an automatic attempt drive the lock's disabled expression", () => {
    const controls = section(
      "function nativeDiscordHeaderControls()",
      "function trustedHeader()",
    );
    // The disabled attribute is still driven solely by discordQaComposerBusy...
    expect(controls).toContain(
      'id="discord-qa-toggle-composer" type="button" aria-pressed="${nativeDiscordProtectionActive}" aria-label="${composerProtectionLabel}" title="${composerProtectionLabel}" ${discordQaComposerBusy ? "disabled" : ""}',
    );

    const openComposer = section(
      "async function openDiscordQaComposer(",
      "async function openDiscordQaComposerAfterHostReady()",
    );
    // ...and discordQaComposerBusy is only ever assigned true/false inside an
    // explicit operator-source branch, never unconditionally.
    expect(openComposer).toContain('source: "operator" | "automatic" = "operator"');
    expect(openComposer).toContain(
      'if (source === "operator") {\n    discordQaComposerBusy = true;',
    );
    expect(openComposer).toContain(
      'if (source === "operator") discordQaComposerBusy = false;',
    );
    // No unconditional (unguarded) assignment remains at either call site.
    expect(openComposer).not.toContain("\n  discordQaComposerBusy = true;");
    expect(openComposer).not.toContain("\n    discordQaComposerBusy = false;\n    render();");
    // Re-entrancy is guarded by a source-agnostic flag instead, so automatic
    // retries can never toggle the operator-facing disabled attribute.
    expect(openComposer).toContain("discordQaComposerOpening = true;");
    expect(openComposer).toContain("discordQaComposerOpening = false;");
    expect(openComposer).toContain(
      "if (!discordQaShell || discordQaComposerOpening || route !== \"service\") return;",
    );

    const autoOpen = section(
      "async function openDiscordQaComposerAfterHostReady()",
      "async function runDiscordQaOneClick()",
    );
    expect(autoOpen).toContain('await openDiscordQaComposer(15_000, "automatic")');
  });

  it("(b) stops automatic retries after the bound and leaves the lock enabled", () => {
    expect(source).toContain("const discordQaAutoComposerMaxAttempts = 3");

    const openComposer = section(
      "async function openDiscordQaComposer(",
      "async function openDiscordQaComposerAfterHostReady()",
    );
    // Automatic failures only ever increment a counter and surface the
    // reason through the existing nativeProtectFailureNotice mechanism; they
    // never disable the lock (discordQaComposerBusy is untouched here).
    expect(openComposer).toContain(
      'if (source === "operator") {\n      showToast(nativeProtectFailureNotice);\n    } else {\n      discordQaAutoComposerFailureCount += 1;\n    }',
    );
    expect(openComposer).toContain(
      "nativeProtectFailureNotice = localActionError(failure, \"Discord composer stopped safely\")",
    );

    const autoOpen = section(
      "async function openDiscordQaComposerAfterHostReady()",
      "async function runDiscordQaOneClick()",
    );
    // The for-loop bound is the same small constant, so after
    // discordQaAutoComposerMaxAttempts consecutive automatic failures the
    // loop simply exits without ever leaving discordQaComposerBusy set.
    expect(autoOpen).toContain(
      "attempt < discordQaAutoComposerMaxAttempts && !nativeDiscordProtectionActive",
    );
    expect(autoOpen).not.toContain("discordQaComposerBusy");
  });

  it("(c) lets an operator click always attempt again and resets the automatic-failure counter", () => {
    const openComposer = section(
      "async function openDiscordQaComposer(",
      "async function openDiscordQaComposerAfterHostReady()",
    );

    // Any operator-sourced attempt (the default) resets the automatic
    // failure budget up front, before the outcome of this attempt is known.
    expect(openComposer).toContain(
      'if (source === "operator") {\n    discordQaComposerBusy = true;\n    // A fresh operator attempt always gets a clean slate for the bounded\n    // automatic retry budget, in case it fails and automatic retries resume.\n    discordQaAutoComposerFailureCount = 0;\n  }',
    );

    // Operator entry points call with the default ("operator") source and are
    // only gated by the source-agnostic re-entrancy flag, never by the
    // automatic-failure counter or discordQaAutoComposerAttempted.
    const toggle = section(
      "async function toggleDiscordQaComposer()",
      "async function openSoleVerifiedDiscordQaOverlay()",
    );
    expect(toggle).toContain("if (!discordQaShell || discordQaComposerBusy) return;");
    expect(toggle).toContain("await openDiscordQaComposer();");
    expect(toggle).not.toContain("discordQaAutoComposerFailureCount");
    expect(toggle).not.toContain("discordQaAutoComposerAttempted");

    expect(source).toContain('document.querySelector<HTMLButtonElement>("#discord-qa-open-composer")?.addEventListener("click", () => {\n    void openDiscordQaComposer();');
    expect(source).toContain('if (discordQaShell) {\n    void openDiscordQaComposer();\n    return;');
  });
});
