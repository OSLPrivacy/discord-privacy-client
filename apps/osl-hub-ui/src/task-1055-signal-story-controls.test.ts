import { describe, expect, it } from "vitest";
import {
  signalStoryComposerMarkup,
  signalStoryControlNames,
  type SignalStoryComposerSurface,
} from "./signal-story-composer";

const enabledSignalStory: SignalStoryComposerSurface = {
  service: "signal",
  placeKind: "story",
  storiesEnabled: true,
};

describe("TASK 1055 Signal story controls", () => {
  it("shows all seven named controls in an enabled Signal story composer", () => {
    const markup = signalStoryComposerMarkup(enabledSignalStory);
    const names = signalStoryControlNames(enabledSignalStory);
    const renderedControls = markup.match(/data-signal-story-control="[^"]+"/gu) ?? [];
    const unnamedControls = (markup.match(/data-signal-story-control=""/gu) ?? []).length;

    expect(names).toEqual([
      "lock", "private box", "count", "timer", "eye", "view once", "burn",
    ]);
    expect(renderedControls).toHaveLength(7);
    expect(unnamedControls).toBe(0);
    for (const name of names) {
      expect(markup).toContain(`data-signal-story-control="${name}"`);
      expect(markup).toContain(`aria-label="${name}"`);
    }

    console.info(`TASK1055_STORY_CONTROL_NAMES=${names.join(",")}`);
    console.info(`TASK1055_STORY_CONTROL_COUNT=${renderedControls.length}`);
    console.info(`TASK1055_STORY_UNNAMED_CONTROLS=${unnamedControls}`);
  });

  it("does not add story controls to disabled Stories or other Signal places", () => {
    expect(signalStoryComposerMarkup({ ...enabledSignalStory, storiesEnabled: false })).toBe("");
    expect(signalStoryComposerMarkup({ ...enabledSignalStory, placeKind: "direct_message" })).toBe("");
    expect(signalStoryComposerMarkup({ ...enabledSignalStory, service: "instagram" })).toBe("");
  });
});
