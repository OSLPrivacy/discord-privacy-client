import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("T2-61 shipping attachment progress wiring", () => {
  it("subscribes in the production renderer and renders only parsed native progress events", () => {
    expect(main).toMatch(/from\s*["']\.\/attachment-progress["']/u);
    expect(main).toMatch(/function bindAttachmentProgressEvents[\s\S]*listen<unknown>\("osl:\/\/attachment-progress"/u);
    expect(main).toMatch(/parseAttachmentProgressEvent\(event\.payload\)/u);
    expect(main).toMatch(/function oslChatContent\(\): string[\s\S]*attachmentProgressMarkupForActiveChat\(\)/u);
    expect(main).toMatch(/if \(!runningUnderVitest\) \{\s*bindAttachmentProgressEvents\(\)/u);
  });
});
