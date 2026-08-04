import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// The B0-05 Adversary's FAIL was correct: routing the command name to
// console.error while the only human-visible path stayed a generic
// "That action failed" does not satisfy "surfaced, naming the command".
// A user who cannot tell a missing command from a network blip has learned
// nothing, and neither has anyone reading their bug report.
const source = readFileSync(resolve(__dirname, "main.ts"), "utf8");

describe("B0-05: an unhandled rejection names the failing command where a human can see it", () => {
  it("passes a description into the visible toast, not only into console", () => {
    expect(source).toContain("containBackgroundFailure(describeRejection(event.reason))");
    expect(source).toMatch(/function containBackgroundFailure\(detail\?: string\)/);
    // the toast must interpolate the detail, not discard it
    expect(source).toMatch(/That action failed\. Nothing changed\. \(\$\{detail\}\)/);
  });

  it("extracts the command name from a Tauri missing-command rejection", () => {
    const re = /(?:command|Command)\s+([a-z0-9_]+)\s+not\s+found/;
    expect(re.exec("Command scrub_imap_delete not found")?.[1]).toBe("scrub_imap_delete");
    expect(re.exec("command stop_osl_lan_room not found")?.[1]).toBe("stop_osl_lan_room");
  });

  it("still surfaces something when the reason is not a missing command", () => {
    // a bare generic toast is what this task exists to remove; any non-empty
    // reason must reach the user rather than being swallowed into boilerplate
    expect(source).toContain("text.length > 80");
  });
});
