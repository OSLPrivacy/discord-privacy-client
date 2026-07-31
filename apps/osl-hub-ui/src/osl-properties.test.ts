import { describe, expect, it } from "vitest";
import { parseOslProperties, removeOslProperty, serializeOslProperties, setOslProperty } from "./osl-properties";

describe("portable encrypted note properties", () => {
  it("parses typed portable frontmatter without changing note content", () => {
    const body = '---\nStatus: "Doing"\nEstimate: 3.5\nDone: false\nDue: 2026-07-31\nPeople: ["Liam","Sam"]\n---\n# Private plan\nKeep this exact.';
    const parsed = parseOslProperties(body)!;
    expect(parsed.properties.map((property) => property.type)).toEqual(["text", "number", "checkbox", "date", "list"]);
    expect(parsed.content).toBe("# Private plan\nKeep this exact.");
    expect(serializeOslProperties(parsed)).toBe(body);
  });

  it("adds, updates, and removes properties while preserving content", () => {
    const added = setOslProperty("# Body\nunchanged", "Due", "date", "2026-08-01")!;
    const updated = setOslProperty(added, "Due", "date", "2026-08-02")!;
    expect(parseOslProperties(updated)?.content).toBe("# Body\nunchanged");
    expect(parseOslProperties(updated)?.properties[0].value).toBe("2026-08-02");
    expect(removeOslProperty(updated, "due")).toBe("# Body\nunchanged");
  });

  it("fails closed on ambiguous, duplicated, malformed, or unsafe frontmatter", () => {
    expect(parseOslProperties("---\nStatus: one\nstatus: two\n---\nBody")).toBeNull();
    expect(parseOslProperties("---\nDue: 2026-02-30\n---\nBody")).toBeNull();
    expect(parseOslProperties("---\nBad\u202ename: value\n---\nBody")).toBeNull();
    expect(setOslProperty("Body", "1invalid", "text", "value")).toBeNull();
  });
});
