import { describe, expect, it } from "vitest";
import {
  signalStoryComposerMarkup,
  signalStoryControlNames,
  type SignalStoryComposerSurface,
} from "./signal-story-composer";

const surface = (selectedAudienceAllowed: boolean): SignalStoryComposerSurface => ({
  service: "signal",
  placeKind: "story",
  storiesEnabled: true,
  selectedAudienceAllowed,
});

describe("TASK 1056 Signal story audience allowance", () => {
  it("keeps all story controls unavailable until the backend allows the selected audience", () => {
    const beforeMarkup = signalStoryComposerMarkup(surface(false));
    const beforeNames = signalStoryControlNames(surface(false));
    const afterMarkup = signalStoryComposerMarkup(surface(true));
    const afterNames = signalStoryControlNames(surface(true));

    expect(beforeMarkup).toBe("");
    expect(beforeNames).toEqual([]);
    expect(afterNames).toEqual([
      "lock", "private box", "count", "timer", "eye", "view once", "burn",
    ]);
    expect((afterMarkup.match(/data-signal-story-control="[^"]+"/gu) ?? [])).toHaveLength(7);

    console.info(`TASK1056_UI_CONTROLS_BEFORE=${beforeNames.length}`);
    console.info(`TASK1056_UI_CONTROLS_AFTER=${afterNames.length}`);
  });
});
