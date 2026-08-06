import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseCompletedScrubRunStatusProjection, scrubDeletionContract } from "./scrub";
import { initialBeforeSendChecks, onboardingBeforeSendMarkup } from "./onboarding-before-send";
import { initialDeleteChoices, onboardingDeleteMarkup } from "./onboarding-delete";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
// 2026-08-06 restyle: the screen moved out of main.ts into its own module, and
// the three explanatory paragraphs became three animations. What is checked
// below is the rendered markup, not the source text that used to produce it.
const beforeSend = onboardingBeforeSendMarkup(initialBeforeSendChecks());

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

/**
 * 2026-08-06 re-split. `protectionPresetOnboardingContent` used to render the
 * Basic / Balanced / Maximum chooser followed by the Balanced settings list,
 * and the NEXT screen showed the same list again. The preset changed nothing on
 * the send path, so this file's old assertions pinned the look of a control
 * that did not work. The screen is now the three checks that run AT THE MOMENT
 * YOU SEND; `reviewDefaultsOnboardingContent` owns what OSL keeps afterwards.
 *
 * The rules that were riding on the old markup did not go away, so they are
 * re-anchored below rather than deleted.
 */
describe("onboarding before-send checks (the former protection-preset screen)", () => {
  const presetContent = beforeSend;
  const reviewContent = onboardingDeleteMarkup(initialDeleteChoices());
  const presetMarkup = presetContent;

  // Guards the deletion: the inert Basic/Balanced/Maximum chooser -- a question
  // whose answer the send path never read -- must not come back to onboarding.
  it("does not bring the inert Basic, Balanced, and Maximum chooser back", () => {
    expect(presetMarkup).not.toContain("data-protection-preset");
    expect(presetMarkup).not.toContain('name="protection-preset"');
    // The segmented Always/Ask/Never control is radios, so the ban is on the
    // preset's own name rather than on radios in general.
    expect(presetMarkup).not.toMatch(/\b(?:Basic|Balanced|Maximum)\b/u);
    expect(presetMarkup).not.toContain('badge: "Recommended"');
    expect(presetMarkup).not.toContain("Custom");
    expect(presetMarkup).not.toContain("Manual configuration");
  });

  // Protects the replacement: three before-send checks, each its own real
  // toggle, shipping with unprotected-warning on, protected-warning off, and
  // file cleaning set to ask first (it must never edit a file silently).
  it("offers the three before-send checks with their shipped defaults", () => {
    expect(presetContent).toContain("What should OSL check before you send?");
    // Titles are the three answers to the heading. The full instruction each one
    // stands for stays as the control's label, so a screen reader -- which has no
    // heading above the row to lean on -- still gets the whole sentence.
    expect(presetContent).toContain(">Unprotected messages<");
    expect(presetContent).toContain(">Protected messages<");
    expect(presetContent).toContain(">Metadata in files<");
    expect(presetContent).toContain("Warn me before an unprotected message</span>");
    expect(presetContent).toContain("Warn me before a protected message too</span>");
    expect(presetContent).toContain('aria-label="Remove metadata from files"');
    // Shipped defaults: warn on unprotected ON, warn on protected OFF, and files
    // set to ask rather than to strip -- OSL must never edit a file silently.
    expect(initialBeforeSendChecks()).toEqual({ warnUnprotected: true, warnProtected: false, cleanFiles: "ask" });
    expect(presetContent).toContain('id="warn-unprotected" checked');
    expect(presetContent).toContain('id="warn-protected" ');
    expect(presetContent).not.toContain('id="warn-protected" checked');
    expect(presetContent).toContain('value="ask" checked');
    expect(presetContent).not.toContain('value="always" checked');
    // Each row's state comes from its own id, so no row can be flipped as a side
    // effect of another.
    const flipped = onboardingBeforeSendMarkup({ warnUnprotected: false, warnProtected: true, cleanFiles: "never" });
    expect(flipped).toContain('id="warn-protected" checked');
    expect(flipped).not.toContain('id="warn-unprotected" checked');
    expect(flipped).toContain('value="never" checked');
  });

  // Protects the first-run reading level: every check is explained in terms of
  // what happens to the person's message, with no implementation vocabulary.
  it("explains each check in plain language without exposing implementation concepts", () => {
    // The three paragraphs became three animations, so what has to stay legible
    // is the row title itself plus a described picture for anyone who cannot see
    // it. An animation with no text alternative would make the screen unreadable
    // to a screen reader, which is worse than the paragraph it replaced.
    expect(presetContent).toContain('aria-label="an unlocked message stops at a checkpoint and is flagged"');
    expect(presetContent).toContain('aria-label="a locked message stops at the same checkpoint and is flagged"');
    expect(presetContent).toContain('aria-label="a file stops at the checkpoint and its hidden details drop away"');
    // The on-device promise survives the cut from two sentences to one line.
    expect(presetContent).toContain("Checks run on this device only");
    for (const forbidden of ["keyserver", "ratchet", "receipt", "browser profile", "provider adapter"]) {
      expect(presetContent.toLowerCase()).not.toContain(forbidden);
    }
  });

  // Protects the rule the removed "Deletion automation starts off / Fail closed"
  // rows used to carry: setup starts nothing destructive, and a cleanup run that
  // cannot prove account binding plus user-confirmed authority is refused. The
  // copy half now lives on the keep-on-device screen; the behaviour half is
  // executed here instead of string-matched, so it cannot be lost to a reword.
  it("keeps destructive automation off and fails closed without authority", () => {
    // The screen now asks what to DELETE rather than what to keep, so the rule
    // is held by the defaults themselves: both destructive switches start off,
    // and the screen says outright that nothing goes without a confirmation.
    expect(initialDeleteChoices()).toEqual({ deleteDrafts: false, deleteOldMessages: false });
    expect(reviewContent).toContain("Nothing is deleted without confirmation");

    expect(scrubDeletionContract.unattendedDeletionAllowed).toBe(false);
    expect(scrubDeletionContract.completeEditableReviewRequiredEveryBatch).toBe(true);
    expect(scrubDeletionContract.finalConfirmationRequiredEveryBatch).toBe(true);
    expect(scrubDeletionContract.requestedDeletionCountsAsVerified).toBe(false);

    const authorised = {
      runState: "complete",
      completedAtUnixMs: 1_700_000_000_000,
      userReviewed: true,
      accountBinding: "verified",
      cleanupAuthority: "user_confirmed",
      receipts: [],
    };
    // Proves the gate can pass, so the refusals below are a real gate and not a
    // parser that rejects everything.
    expect(parseCompletedScrubRunStatusProjection(authorised)).not.toBeNull();
    expect(parseCompletedScrubRunStatusProjection({ ...authorised, accountBinding: "unverified" })).toBeNull();
    expect(parseCompletedScrubRunStatusProjection({ ...authorised, cleanupAuthority: "inferred" })).toBeNull();
    expect(parseCompletedScrubRunStatusProjection({ ...authorised, userReviewed: false })).toBeNull();

    for (const screen of [presetContent, reviewContent]) {
      expect(screen).not.toContain("auto-delete");
      expect(screen).not.toContain("automatic deletion");
    }
  });

  // Protects the split staying in order: the privacy step renders the
  // before-send screen, and its Continue leads to the keep-on-device screen.
  it("renders from the onboarding privacy step and continues to the keep-on-device step", () => {
    const privacy = functionSource("onboardingPrivacyContent", "protectionPresetOnboardingContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(privacy).toContain("return protectionPresetOnboardingContent();");
    expect(presetContent).toContain('data-onboarding="defaults"');
    expect(binding).toContain('"[data-onboarding]"');
    expect(binding).toContain("onboardingRoute = onboardingRouteForBuild(button.dataset.onboarding as OnboardingRoute)");
  });

  // Protects the preset setting itself, which still exists behind Privacy ->
  // Change preset: choosing one is local-only state plus a rerender, never IPC.
  it("binds preset radios through the same change-rerender pattern as nearby onboarding choices", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");

    expect(binding).toContain('input[name="protection-preset"]');
    expect(binding).toContain("protectionPresetValues.includes(input.value as ProtectionPreset)");
    expect(binding).toContain("protectionPreset = input.value as ProtectionPreset");
    expect(binding).toContain("persistProtectionPreset();");
    expect(binding).toContain("render();");
  });
});
