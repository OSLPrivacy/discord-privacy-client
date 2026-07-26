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
const openComposer = region(
  "async function openDiscordQaComposer(",
  "async function openDiscordQaComposerAfterHostReady()",
);
const autoOpen = region(
  "async function openDiscordQaComposerAfterHostReady()",
  "async function runDiscordQaOneClick()",
);
const operatorOpen = region(
  "async function openDiscordQaProtectionForOperator(",
  "async function openDiscordQaComposer(",
);

function escapeHtmlStub(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

type LockMarkup = {
  composerLockState: string;
  composerProtectionLabel: string;
  composerControl: string;
  composerRefusalNotice: string;
};

/**
 * Evaluate the shipped lock markup block, so the states are asserted against
 * the real template instead of a copy of it.
 */
function renderLock(input: {
  protectionActive?: boolean;
  busy?: boolean;
  refusal?: { message: string; reason: string } | null;
  markerAvailable?: boolean;
}): LockMarkup {
  const block = region(
    "  const composerRefusal = nativeDiscordProtectionActive ? null : discordQaComposerRefusal;",
    '\n  return `<div class="native-discord-header-controls',
  );
  const build = new Function(
    "nativeDiscordProtectionActive",
    "discordQaComposerBusy",
    "discordQaComposerRefusal",
    "discordMarkerAvailable",
    "lock",
    "escapeHtml",
    `${block}\nreturn { composerLockState, composerProtectionLabel, composerControl, composerRefusalNotice };`,
  ) as (
    protectionActive: boolean,
    busy: boolean,
    refusal: { message: string; reason: string } | null,
    markerAvailable: boolean,
    lock: string,
    escapeHtml: (value: string) => string,
  ) => LockMarkup;
  return build(
    input.protectionActive ?? false,
    input.busy ?? false,
    input.refusal ?? null,
    input.markerAvailable ?? true,
    "<svg></svg>",
    escapeHtmlStub,
  );
}

/** Evaluate the shipped reason mapper; only its type annotations are stripped. */
function mapReason(reason: string): string {
  const declaration = region(
    "function discordQaComposerRefusalMessage(reason: string): string {",
    "\nfunction discordQaComposerRefusalFrom(",
  );
  const body = declaration.slice(declaration.indexOf("{") + 1, declaration.lastIndexOf("}"));
  const map = new Function("reason", body) as (value: string) => string;
  return map(reason);
}

// Contract change (live bug, 2026-07-25): this reason means "OSL could not put
// its own saved copy back", which says nothing about what is in the message box
// and was firing at a provably empty one. It used to render as "Discord's
// message box still holds a leftover message. Clear Discord's message box",
// which the operator could not act on -- the box was already clear. Only a
// refusal that has proven there is text in the box may say that now.
const refusal = {
  message: "OSL could not put back the message it saved from Discord's message box. Leave the box alone, then press the lock again.",
  reason: "OSL could not open protection or restore the saved Discord draft",
};

describe("Discord QA lock refusal is visible and explained", () => {
  it("distinguishes on, off, busy, and refused on the lock control", () => {
    const on = renderLock({ protectionActive: true });
    const off = renderLock({});
    const busy = renderLock({ busy: true });
    const refused = renderLock({ refusal });

    expect(on.composerLockState).toBe("on");
    expect(off.composerLockState).toBe("off");
    expect(busy.composerLockState).toBe("busy");
    expect(refused.composerLockState).toBe("refused");
    for (const [state, markup] of [
      ["on", on],
      ["off", off],
      ["busy", busy],
      ["refused", refused],
    ] as const) {
      expect(markup.composerControl).toContain(`data-lock-state="${state}"`);
      expect(markup.composerControl).toContain('id="discord-qa-toggle-composer"');
    }
    // Only busy is disabled; a refused lock must stay clickable so the operator
    // can retry after fixing what the message told them.
    expect(busy.composerControl).toContain("disabled");
    expect(refused.composerControl).not.toContain("disabled");
    expect(off.composerControl).not.toContain("disabled");
    // Refusal is the only state that is invalid, and it is never colour-only:
    // it carries a bang mark and says so in the accessible name.
    expect(refused.composerControl).toContain('aria-invalid="true"');
    expect(refused.composerControl).toContain(">!</span>");
    expect(refused.composerProtectionLabel).toContain("refused");
    for (const other of [on, off, busy]) {
      expect(other.composerControl).not.toContain("aria-invalid");
      expect(other.composerControl).not.toContain(">!</span>");
    }
    expect(off.composerProtectionLabel).toBe("Protected composer off — open");
    expect(busy.composerProtectionLabel).toContain("opening");
    expect(on.composerProtectionLabel).toContain("on");
  });

  it("renders the reason in the header strip, the one surface above Discord", () => {
    const refused = renderLock({ refusal });

    expect(refused.composerRefusalNotice).toContain('id="discord-qa-composer-refusal"');
    expect(refused.composerRefusalNotice).toContain('role="status"');
    expect(refused.composerRefusalNotice).toContain("could not put back the message it saved");
    // The chip is emitted inside the header-controls strip, next to the lock and
    // the eye chip that live measurement already confirmed draws above Discord —
    // not in the host viewport or a toast, both of which Discord covers.
    expect(headerControls).toContain(
      '${composerRefusalNotice}${transcriptNotice}${transcriptVisibilityControl}${composerControl}',
    );
    expect(headerControls).toContain('class="native-discord-header-controls discord-qa-header-controls"');
    // Not toast-only: the failure path writes state the strip renders.
    expect(openComposer).toContain("discordQaComposerRefusal = discordQaComposerRefusalFrom(nativeProtectFailureNotice)");
    // Nothing untrusted is interpolated raw.
    const injected = renderLock({
      refusal: { message: '<img src=x onerror="x">', reason: '"><script>x</script>' },
    });
    expect(injected.composerRefusalNotice).not.toContain("<img");
    expect(injected.composerRefusalNotice).not.toContain("<script>");
    expect(injected.composerProtectionLabel).not.toContain("<img");
  });

  it("persists the refusal until the next operator attempt or a real open", () => {
    // Cleared exactly where the question is answered: a new operator attempt,
    // a composer that opened, and both protection-open success paths.
    expect(openComposer).toContain('if (source === "operator") discordQaComposerRefusal = null;');
    expect(openComposer).toContain("discordQaComposerRefusal = null;\n    discordQaAutoComposerFailureCount = 0;");
    expect(source.match(/discordQaComposerRefusal = null/gu) ?? []).toHaveLength(5);
    // An open composer never displays a stale refusal even if one is still held.
    expect(headerControls).toContain(
      "const composerRefusal = nativeDiscordProtectionActive ? null : discordQaComposerRefusal;",
    );
    // No timer of its own: the refusal is event-driven, and this file now starts
    // no repeating timer at all -- the geometry keeper's one cadence moved inside
    // the keeper itself.
    expect(source.match(/window\.setInterval\(/gu) ?? []).toHaveLength(0);
    expect(source).not.toContain("setTimeout(() => { discordQaComposerRefusal");
  });

  it("only an operator attempt paints a refusal; the 3 automatic retries stay quiet", () => {
    // The write is inside an explicit operator-source branch, and the automatic
    // path only increments its bounded counter.
    expect(openComposer).toContain(
      'if (source === "operator") {\n      discordQaComposerRefusal = discordQaComposerRefusalFrom(nativeProtectFailureNotice);\n    }',
    );
    expect(autoOpen).not.toContain("discordQaComposerRefusal");
    expect(autoOpen).not.toContain("discordQaComposerBusy");
    expect(autoOpen).toContain("attempt < discordQaAutoComposerMaxAttempts && !nativeDiscordProtectionActive");
    expect(source).toContain("const discordQaAutoComposerMaxAttempts = 3");
    // The flicker fix is untouched: busy is still operator-only and the
    // re-entrancy guard is still source-agnostic.
    expect(openComposer).toContain('if (source === "operator") {\n    discordQaComposerBusy = true;');
    expect(openComposer).toContain('if (source === "operator") discordQaComposerBusy = false;');
    expect(openComposer).toContain("discordQaComposerOpening = true;");
    expect(openComposer).toContain(
      'if (source === "operator") {\n      showToast(nativeProtectFailureNotice);\n    } else {\n      discordQaAutoComposerFailureCount += 1;\n    }',
    );
    // The friend picker is operator-only, so it reports its own refusal.
    expect(source).toContain("void openDiscordQaProtectionForOperator(button.dataset.nativeProtectPerson ?? \"\")");
    expect(operatorOpen).toContain("discordQaComposerRefusal = discordQaComposerRefusalFrom(nativeProtectFailureNotice)");
    expect(operatorOpen).not.toContain("discordQaComposerBusy");
    expect(operatorOpen).not.toContain("nativeDiscordProtectionActive =");
  });

  it("maps the reasons it can observe to plain language and falls back verbatim", () => {
    expect(mapReason("OSL could not open protection or restore the saved Discord draft"))
      .toBe(refusal.message);
    expect(mapReason("The saved Discord draft could not be typed back into the composer"))
      .toBe(refusal.message);
    // The one refusal the native side only returns after proving the composer
    // holds the operator's own text is the one -- and the only one -- that may
    // ask them to clear it.
    expect(mapReason("The Discord message box still holds your own draft and OSL did not touch it"))
      .toContain("Clear Discord's message box");
    // Contract change: OSL holding a saved copy for a conversation that is not
    // on screen is not a statement about the composer in front of the operator.
    // It used to render as "Clear Discord's message box", which latched on
    // every lock press at an empty box until OSL was restarted.
    expect(mapReason("OSL saved a draft from another Discord conversation and cannot put it back here"))
      .toContain("Reopen that conversation");
    // Unknown is reported as unknown, never as "there is something in the box".
    expect(mapReason("The Discord draft could not be cleared safely"))
      .toContain("could not confirm what is in Discord's message box");
    expect(mapReason(
      "Discord did not expose the composer text, so OSL cannot tell whether a draft needs preserving",
    )).toContain("cannot tell whether anything is in it");
    expect(mapReason("The native Discord draft probe did not clear exactly"))
      .toContain("cannot tell whether anything is in it");
    expect(mapReason("The Discord composer changed before protection opened"))
      .toContain("changed while protection was opening");
    expect(mapReason("Discord did not expose one exact visible message composer"))
      .toContain("Open a direct message");
    expect(mapReason("The protected Discord identity is not registered yet"))
      .toContain("still registering");
    expect(mapReason("Protection stopped: this QA identity requires exactly one verified friend."))
      .toContain("Verify a friend first");
    expect(mapReason("The native Discord window changed before protection opened"))
      .toContain("Bring Discord forward");
    expect(mapReason("Discord carrier-row accessibility timed out"))
      .toContain("Press the lock again");
    // Exactly one native reason may produce an instruction to clear the box, so
    // an operator staring at an empty composer is never told to empty it.
    const clearsTheBox = [
      "OSL could not open protection or restore the saved Discord draft",
      "The saved Discord draft could not be typed back into the composer",
      "The saved Discord draft could not be verified after restoration",
      "OSL saved a draft from another Discord conversation and cannot put it back here",
      "The Discord draft could not be cleared safely",
      "The Discord draft could not be cleared safely (binding changed)",
      "Discord did not expose the composer text, so OSL cannot tell whether a draft needs preserving",
      "The native Discord draft probe did not clear exactly",
      "The Discord message box still holds your own draft and OSL did not touch it",
    ].filter((reason) => mapReason(reason).includes("Clear Discord's message box"));
    expect(clearsTheBox)
      .toEqual(["The Discord message box still holds your own draft and OSL did not touch it"]);

    // Never invent a cause: an unrecognised reason is shown exactly as returned.
    const unknown = "Some brand new native refusal nobody mapped";
    expect(mapReason(unknown)).toBe(unknown);
    // An empty reason still produces something the operator can read.
    expect(source).toContain('reason.trim() || "Protection did not open and gave no reason."');
  });

  it("never puts draft or message text in the refusal", () => {
    const mapper = region(
      "function discordQaComposerRefusalMessage(reason: string): string {",
      "\nfunction discordQaComposerRefusalFrom(",
    );
    // Every mapped message is a fixed literal; only the fallback echoes the
    // native reason, which never carries composer text.
    expect(mapper).not.toContain("${");
    expect(mapper).not.toContain("peerProtectedSheet");
    expect(mapper).not.toContain("draft.");
    expect(mapper).not.toContain("console.");
    for (const forbidden of ["peerProtectedSheet", "localProtectedSheet", "console.", "openedPlaintext"]) {
      expect(operatorOpen).not.toContain(forbidden);
    }
    // The eye stays independent of the lock's refusal state.
    expect(headerControls).toContain("const transcriptOutcome");
    expect(region("  const transcriptMode =", "\n  const lock =")).not.toContain("discordQaComposerRefusal");
    // The composer-less lock-visibility gate stays inert.
    expect(source).toContain("let discordMarkerAvailable = true;");
    expect(headerControls).toContain(
      "const composerControl = discordMarkerAvailable || nativeDiscordProtectionActive",
    );
  });
});
