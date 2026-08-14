import { describe, expect, it } from "vitest";
import {
  COVER_WRITING_VIEWS,
  coverWritingControlsMarkup,
  savedCoverWritingChoice,
} from "./cover-writing-controls";
import {
  FACTORY_MESSAGE_DEFAULTS,
  initialMessageDefaultsScreenState,
  messageDefaultsScreenMarkup,
  saveMessageDefaults,
  chooseMessageDefault,
} from "./message-defaults";

function checkedChoice(markup: string): string | null {
  return markup.match(/data-cover-writing-choice="([^"]+)"[^>]*aria-pressed="true"/u)?.[1] ?? null;
}

describe("TASK3525 saved writing choice", () => {
  it("adds the saved writing choice to Message defaults without adding a third cover-writing control", () => {
    const beforeControlCount = 3;
    const state = initialMessageDefaultsScreenState(FACTORY_MESSAGE_DEFAULTS);
    const markup = messageDefaultsScreenMarkup(state);
    const afterControlCount = markup.match(/data-message-default-group=/gu)?.length ?? 0;
    const writingInputs = markup.match(/data-message-default="writing"/gu)?.length ?? 0;

    expect(afterControlCount).toBe(beforeControlCount + 1);
    expect(writingInputs).toBe(2);
    console.info(`TASK3525 message_defaults_controls_before=${beforeControlCount} after=${afterControlCount} writing_choice_buttons=${writingInputs}`);
  });

  it("uses the saved writer when each of the nine new-message views opens", () => {
    let state = initialMessageDefaultsScreenState(FACTORY_MESSAGE_DEFAULTS);
    state = saveMessageDefaults(chooseMessageDefault(state, "writing", "ai_covertext"));
    const aiChoices = COVER_WRITING_VIEWS.map((view) => checkedChoice(coverWritingControlsMarkup(view, { savedWriting: state.saved.coverWriting })));
    state = saveMessageDefaults(chooseMessageDefault(state, "writing", "plaintext"));
    const wordbankChoices = COVER_WRITING_VIEWS.map((view) => checkedChoice(coverWritingControlsMarkup(view, { savedWriting: state.saved.coverWriting })));

    expect(aiChoices).toEqual(COVER_WRITING_VIEWS.map(() => "ai-covertext"));
    expect(wordbankChoices).toEqual(COVER_WRITING_VIEWS.map(() => "covertext"));
    expect(savedCoverWritingChoice("ai_covertext")).toBe("ai-covertext");
    expect(savedCoverWritingChoice("plaintext")).toBe("covertext");
    console.info(`TASK3525 saved_choice_views=${COVER_WRITING_VIEWS.length} ai_selected=${aiChoices.filter((choice) => choice === "ai-covertext").length} wordbank_selected=${wordbankChoices.filter((choice) => choice === "covertext").length}`);
  });

  it("has no view that bypasses the saved-choice renderer and keeps the strip at two controls", () => {
    const ignoredSavedChoiceViews = COVER_WRITING_VIEWS.filter((view) => !coverWritingControlsMarkup(view, { savedWriting: "ai_covertext" }).includes('aria-pressed="true"'));
    const buttonCount = (coverWritingControlsMarkup("discord", { savedWriting: "plaintext" }).match(/<button\b/gu) ?? []).length;
    expect(ignoredSavedChoiceViews).toHaveLength(0);
    expect(buttonCount).toBe(2);
    console.info(`TASK3525 views_ignoring_saved_choice=${ignoredSavedChoiceViews.length} cover_writing_controls=${buttonCount}`);
  });
});
