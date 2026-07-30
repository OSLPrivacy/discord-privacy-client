import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("Privacy destination", () => {
  const destination = functionSource("privacyDestinationContent", "massCleanupActionLabel");

  it("starts with the active preset and then shows global policy", () => {
    expect(destination).toContain("ACTIVE PRESET");
    expect(destination).toContain("<h2 id=\"privacy-preset-title\">Balanced</h2>");
    expect(destination.indexOf("ACTIVE PRESET")).toBeLessThan(destination.indexOf("Global policy"));
    expect(destination).toContain("Inherited from Balanced");
    expect(destination).toContain("Balanced preset / app / account / conversation exception");
    expect(destination).toContain("Deletion off");
    expect(destination).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/i);
  });

  it("renders the five understandable policy groups from the product plan", () => {
    for (const label of ["Before I send", "After I send", "Incoming content", "My history", "My exposure"]) {
      expect(destination).toContain(label);
    }
    expect(destination).toContain("Risk warnings, public-post checks, and attachment cleaning.");
    expect(destination).toContain("Message timers default to");
    expect(destination).toContain("Link, scam, tracker, and file warnings");
    expect(destination).toContain("Manual scan and guided review");
    expect(destination).toContain("privacy drift checks");
  });

  it("keeps solo privacy tools under Privacy instead of Home", () => {
    for (const label of ["History Cleanup", "Attachment Guard", "Email Privacy", "Exposure Inventory", "Privacy Drift Watch", "Scam Shield", "Data Removal", "Encrypted Capsule"]) {
      expect(destination).toContain(label);
    }
    expect(destination).toContain("Useful even when nobody else uses OSL.");
    expect(destination).toContain('aria-labelledby="privacy-tools-title"');
  });

  it("offers the local scan while refusing unverified cleanup or deletion claims", () => {
    expect(destination).toContain('for="privacy-export-input"');
    expect(destination).toContain("FREE · THIS DEVICE ONLY");
    expect(destination).toContain("Nothing is deleted by this build.");
    expect(destination).toContain("OSL refuses actions it cannot verify.");
    expect(destination).toContain("No auto delete");
    expect(destination).toContain("Apps, people, exports, backups, and opened copies may retain content.");
    expect(destination).not.toMatch(/guaranteed deletion|silently|auto-retry|ban-risk|100%/i);
  });
});
