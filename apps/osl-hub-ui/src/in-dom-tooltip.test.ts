import { describe, expect, it } from "vitest";
import { inDomTooltipMarkup } from "./in-dom-tooltip";

describe("in-DOM tooltip markup", () => {
  it("keeps dynamic tooltip text in the protected document without a native title attribute", () => {
    const markup = inDomTooltipMarkup('Open <private> & "personal" profile');

    expect(markup).toContain('class="in-dom-tooltip"');
    expect(markup).toContain('role="tooltip"');
    expect(markup).toContain("Open &lt;private&gt; &amp; &quot;personal&quot; profile");
    expect(markup).not.toContain("title=");
  });
});
