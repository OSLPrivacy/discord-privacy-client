import { describe, expect, it } from "vitest";
import { peopleDestinationHeaderMarkup } from "./people-destination-header";

describe("People destination header", () => {
  it("uses the shared two-child destination-header layout", () => {
    const header = peopleDestinationHeaderMarkup();
    const directChildren = [...header.matchAll(/<(div|button)\b/g)].map((match) => match[1]);

    expect(header).toMatch(/^<header class="destination-header">/);
    expect(directChildren).toEqual(["div", "button"]);
    expect(header).not.toContain('data-route="home"');
  });
});
