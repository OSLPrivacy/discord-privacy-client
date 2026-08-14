import { describe, expect, it } from "vitest";
import { friendPictureMarkup } from "./friend-picture";

/**
 * TASK 0236: Test that friend pictures render correctly when present,
 * and colored initials render when pictures are absent.
 */
describe("friend pictures and fallbacks", () => {
  it("shows a picture when available", () => {
    const markup = friendPictureMarkup({
      picture: "data:image/png;base64,iVBORw0KGg=",
      fallbackLetter: "A",
      fallbackColour: "#06b6d4",
    });
    expect(markup).toContain('class="friend-picture"');
    expect(markup).toContain('src="data:image/png;base64,iVBORw0KGg="');
    expect(markup).not.toContain("friend-picture-fallback");
  });

  it("shows a colored initial circle when picture is absent", () => {
    const markup = friendPictureMarkup({
      picture: null,
      fallbackLetter: "M",
      fallbackColour: "#14b8a6",
    });
    expect(markup).toContain('class="friend-picture-fallback"');
    expect(markup).toContain('background-color: #14b8a6');
    expect(markup).toContain(">M<");
    expect(markup).not.toContain('class="friend-picture"');
  });

  it("escapes HTML in picture URLs", () => {
    const markup = friendPictureMarkup({
      picture: "data:image/svg+xml,<svg></svg>",
      fallbackLetter: "X",
      fallbackColour: "#ffffff",
    });
    expect(markup).toContain("&lt;svg&gt;");
    expect(markup).not.toContain("<svg>");
  });

  it("escapes HTML in fallback letter", () => {
    const markup = friendPictureMarkup({
      picture: null,
      fallbackLetter: "<",
      fallbackColour: "#ffffff",
    });
    expect(markup).toContain("&lt;</span>");
  });
});
