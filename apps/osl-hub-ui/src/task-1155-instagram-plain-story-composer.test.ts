import { describe, expect, it } from "vitest";
import {
  instagramPlainStoryComposerMarkup,
  instagramPlainStoryControlNames,
  type InstagramStoryComposerSurface,
} from "./instagram-story-composer";

const plainUploadedDesktopStory: InstagramStoryComposerSurface = {
  service: "instagram",
  placeKind: "story",
  composerKind: "plain",
  uploadKind: "uploaded_file",
  viewport: "desktop",
};

describe("TASK1155 Instagram plain uploaded-file story composer", () => {
  it("shows the six named controls without blank or placeholder rows", () => {
    const markup = instagramPlainStoryComposerMarkup(plainUploadedDesktopStory);
    const names = instagramPlainStoryControlNames(plainUploadedDesktopStory);
    const renderedRows = markup.match(/data-instagram-story-control="[^"]+"/gu) ?? [];
    const unnamedRows = (markup.match(/data-instagram-story-control=""/gu) ?? []).length;

    expect(names).toEqual(["lock", "private box", "count", "timer", "eye", "view once"]);
    expect(renderedRows).toHaveLength(6);
    expect(unnamedRows).toBe(0);
    expect(markup.trim().length).toBeGreaterThan(300);
    for (const name of names) {
      expect(markup).toContain(`data-instagram-story-control="${name}"`);
      expect(markup).toContain(`aria-label="${name}"`);
    }

    console.log(`TASK1155_STORY_CONTROL_NAMES=${names.join(",")}`);
    console.log(`TASK1155_STORY_CONTROL_COUNT=${renderedRows.length}`);
    console.log(`TASK1155_STORY_UNNAMED_PLACEHOLDER_ROWS=${unnamedRows}`);
    console.log(`TASK1155_STORY_MARKUP_LENGTH=${markup.trim().length}`);
  });

  it("does not add the toolbar to mobile or non-uploaded story composers", () => {
    expect(instagramPlainStoryComposerMarkup({ ...plainUploadedDesktopStory, viewport: "mobile" })).toBe("");
    expect(instagramPlainStoryComposerMarkup({ ...plainUploadedDesktopStory, uploadKind: "camera" })).toBe("");
  });
});
