import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  EMPTY_VIEW_ONCE_OVERLAY,
  viewOnceOverlayMarkup,
  type ViewOnceOverlayModel,
} from "./view-once-overlay";

// TASK 0564: read from the SAME words file the backend serves.
const SCREEN_WORDS_URL = new URL("../../../crates/ipc/src/screen_words/", import.meta.url);
const EN_WORDS = JSON.parse(readFileSync(new URL("en.json", SCREEN_WORDS_URL), "utf8")).view_once_overlay;
const ES_WORDS = JSON.parse(readFileSync(new URL("es.json", SCREEN_WORDS_URL), "utf8")).view_once_overlay;

describe("view-once overlay", () => {
  it("is hidden in the empty state", () => {
    const markup = viewOnceOverlayMarkup(EMPTY_VIEW_ONCE_OVERLAY, EN_WORDS);
    expect(markup).toContain('data-voo-state="closed"');
    expect(markup).toContain("hidden");
    expect(markup).not.toContain("data-voo-play");
    expect(markup).not.toContain("data-voo-close");
  });

  it("shows the play button and X close action when open", () => {
    const model: ViewOnceOverlayModel = {
      open: true,
      content: { kind: "text", text: "Protected message" },
      durationSeconds: 10,
      durationChoices: [{ seconds: 5 }, { seconds: 10 }, { seconds: 30 }],
    };
    const markup = viewOnceOverlayMarkup(model, EN_WORDS);

    expect(markup).toContain('data-voo-state="open"');
    expect(markup).toContain('data-voo-play');
    expect(markup).toMatch(/data-voo-play[^>]*>&#9654;/u);
    expect(markup).toContain('data-voo-close');
    expect(markup).toMatch(/data-voo-close[^>]*>&times;/u);
    expect(markup).toContain('aria-label="Play view once"');
    expect(markup).toContain('aria-label="Close view once overlay"');
  });

  it("shows a duration choice when open", () => {
    const model: ViewOnceOverlayModel = {
      open: true,
      content: { kind: "text", text: "Protected message" },
      durationSeconds: 10,
      durationChoices: [{ seconds: 5 }, { seconds: 10 }, { seconds: 30 }],
    };
    const markup = viewOnceOverlayMarkup(model, EN_WORDS);

    expect(markup).toContain('data-voo-duration');
    expect(markup).toContain("10 seconds");
    expect(markup).toContain("5 seconds");
    expect(markup).toContain("30 seconds");
    expect(markup).toContain("<option value=\"10\" selected>10 seconds</option>");
  });

  it("renders protected text content", () => {
    const model: ViewOnceOverlayModel = {
      open: true,
      content: { kind: "text", text: "Secret note" },
      durationSeconds: 5,
      durationChoices: [{ seconds: 5 }],
    };
    const markup = viewOnceOverlayMarkup(model, EN_WORDS);

    expect(markup).toContain('data-voo-content-state="text"');
    expect(markup).toContain("Secret note");
  });

  it("renders protected image content", () => {
    const model: ViewOnceOverlayModel = {
      open: true,
      content: { kind: "image", src: "data:image/png;base64,abc", alt: "Sensitive image" },
      durationSeconds: 5,
      durationChoices: [{ seconds: 5 }],
    };
    const markup = viewOnceOverlayMarkup(model, EN_WORDS);

    expect(markup).toContain('data-voo-content-state="image"');
    expect(markup).toContain('data-voo-image');
    expect(markup).toContain("data:image/png;base64,abc");
    expect(markup).toContain("Sensitive image");
  });

  it("the populated markup differs from the empty-state markup", () => {
    const empty = viewOnceOverlayMarkup(EMPTY_VIEW_ONCE_OVERLAY, EN_WORDS);
    const populated = viewOnceOverlayMarkup(
      {
        open: true,
        content: { kind: "text", text: "Protected message" },
        durationSeconds: 10,
        durationChoices: [{ seconds: 5 }, { seconds: 10 }, { seconds: 30 }],
      },
      EN_WORDS,
    );
    expect(populated).not.toBe(empty);
  });

  it("shows different words for the SAME model once the language changes", () => {
    const model: ViewOnceOverlayModel = {
      open: true,
      content: { kind: "text", text: "Protected message" },
      durationSeconds: 10,
      durationChoices: [{ seconds: 10 }],
    };
    const english = viewOnceOverlayMarkup(model, EN_WORDS);
    const spanish = viewOnceOverlayMarkup(model, ES_WORDS);
    expect(english).toContain("View duration");
    expect(spanish).toContain("Duracion de visualizacion");
    expect(english).not.toBe(spanish);
  });

  it("a missing screen word breaks loudly, not blankly", () => {
    const model: ViewOnceOverlayModel = {
      open: true,
      content: { kind: "text", text: "Protected message" },
      durationSeconds: 10,
      durationChoices: [{ seconds: 10 }],
    };
    expect(() => viewOnceOverlayMarkup(model, {})).toThrow(
      /missing view once overlay screen word/u,
    );
  });
});
