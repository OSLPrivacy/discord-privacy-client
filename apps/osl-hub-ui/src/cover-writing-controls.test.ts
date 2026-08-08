import { describe, expect, it } from "vitest";
import {
  COVER_WRITING_LABELS,
  COVER_WRITING_SHAPES_BEFORE,
  COVER_WRITING_SHAPES_NOW,
  COVER_WRITING_VIEWS,
  coverWritingControlsMarkup,
} from "./cover-writing-controls";

describe("TASK3518 shared cover-writing controls", () => {
  it("renders exactly the two separate choices without a cover-writing menu", () => {
    const markup = coverWritingControlsMarkup("discord");
    expect(markup.match(/data-cover-writing-choice=/gu)).toHaveLength(2);
    expect(markup).toContain(COVER_WRITING_LABELS[0]);
    expect(markup).toContain(COVER_WRITING_LABELS[1]);
    expect(markup).not.toMatch(/<(?:select|option)|aria-haspopup|menu/iu);
    console.info(`TASK3518 buttons=2 labels=${COVER_WRITING_LABELS.join("|")} cover_choice_dropdowns=0`);
  });

  it("uses one shape, replacing the prior two, in each requested app and story view", () => {
    const rendered = COVER_WRITING_VIEWS.map((view) => coverWritingControlsMarkup(view));
    expect(new Set(rendered.map((markup) => markup.replace(/data-cover-writing-view="[^"]+"/u, ""))).size).toBe(1);
    expect(rendered.every((markup) => markup.includes('data-cover-writing-controls="shared"'))).toBe(true);
    console.info(`TASK3518 shapes_before=${COVER_WRITING_SHAPES_BEFORE} shapes_now=${COVER_WRITING_SHAPES_NOW} shared_views=${COVER_WRITING_VIEWS.join(",")} own_copies=0`);
  });
});
