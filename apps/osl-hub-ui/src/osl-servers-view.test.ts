import { describe, expect, it } from "vitest";
import { oslServersViewMarkup } from "./osl-servers-view";

describe("OSL Enclaves destination", () => {
  it("names the shipping OSL-native servers and keeps third-party access out of scope", () => {
    const markup = oslServersViewMarkup((label) => `<span>${label}</span>`);

    expect(markup).toContain(">OSL Enclaves</h1>");
    expect(markup).toContain("<span>Available</span>");
    expect(markup).toContain("encrypted shared spaces");
    expect(markup).not.toContain("Coming later");
    expect(markup).toContain("OSL does not claim access to provider communities or read provider pages.");
  });
});
