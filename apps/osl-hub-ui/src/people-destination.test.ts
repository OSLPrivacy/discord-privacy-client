import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

function functionSource(source: string, name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("People destination", () => {
  const source = readRelative("./main.ts");
  const styles = readRelative("./styles.css");
  const header = readRelative("./people-destination-header.ts");
  const content = functionSource(source, "peopleDestinationContent", "peopleDialogMarkup");

  it("renders People as the trust destination over existing friend state", () => {
    expect(source).toContain('"people"');
    expect(functionSource(source, "workspaceContent", "oslChatContent")).toContain('route === "people"');
    expect(header).toContain("<h1 id=\"route-heading\" tabindex=\"-1\">People</h1>");
    expect(content).toContain("Add or verify a person");
    expect(content).toContain("People you know");
    expect(content).toContain('peopleListMarkup("manage")');
    expect(content).toContain("safetyNumberVerified && !person.pendingKeyChange");
    expect(content).toContain("whitelistCount");
  });

  it("keeps adding inert until separate verification and chat approval", () => {
    expect(content).toContain('id="add-friend-form"');
    expect(content).toContain('id="friend-code-input"');
    expect(content).toContain('id="friend-nickname-input"');
    expect(content).toContain("Private chats stay off");
    expect(content).toContain("compare the verification code another way");
    expect(content).toContain("No approval means OSL refuses protected sends for that chat.");
    expect(content).not.toContain("auto-approve");
  });

  it("routes the header Friends control to the destination without exposing machinery", () => {
    const header = functionSource(source, "homeHeader", "homeCommandIcon");
    const binding = functionSource(source, "bindWorkspace", "openHomeAppFromLauncher");
    expect(header).toContain("data-open-friends");
    expect(binding).toMatch(/querySelectorAll(?:<[^>]+>)?\("\[data-open-friends\]"\)/);
    expect(binding).toContain('route = "people"');
    for (const hiddenTerm of ["keyserver", "ratchet", "receipt", "browser profile", "provider adapter"]) {
      expect(content.toLocaleLowerCase()).not.toContain(hiddenTerm);
    }
  });

  it("uses first-class page styling rather than a modal-only People surface", () => {
    expect(styles).toContain(".people-destination");
    expect(styles).toContain(".people-summary-grid");
    expect(styles).toContain(".people-add-section");
    expect(styles).toContain(".people-review-row");
    expect(content).not.toContain("<dialog");
  });
});
