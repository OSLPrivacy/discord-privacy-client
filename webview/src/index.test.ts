import { describe, expect, it } from "vitest";
import { WEBVIEW_SCAFFOLDING_VERSION } from "./index";

describe("webview scaffolding", () => {
  it("exposes a version sentinel so CI has something to test", () => {
    expect(WEBVIEW_SCAFFOLDING_VERSION).toMatch(/^\d+\.\d+\.\d+$/);
  });
});
