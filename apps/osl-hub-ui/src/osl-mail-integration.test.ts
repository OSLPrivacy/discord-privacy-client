import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("OSL Mail Home integration", () => {
  it("adds a first-party Home tile and route", () => {
    expect(main).toContain('{ id: "osl-mail", name: "OSL Mail", available: true }');
    expect(main).toContain('route = "osl-mail"');
    expect(main).toContain('if (route === "osl-mail") return oslMailContent()');
  });

  it("provisions only from the claimed signed OSL username", () => {
    expect(main).toContain("provisionOslMail(claimedOslUsername)");
    expect(main).not.toContain("osl-mail-phone");
  });

  it("does not allow external outbound", () => {
    expect(main).toContain('recipient.endsWith("@oslprivacy.com")');
    expect(main).toContain("External outbound mail is unavailable in v1");
  });
});
