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

describe("public Circles network scope", () => {
  const publicCircles = functionSource("publicCirclesUnavailableMarkup", "inboxDestinationContent");
  const inbox = functionSource("inboxDestinationContent", "oslChatContent");

  it("keeps the public Circles network visibly unavailable", () => {
    expect(publicCircles).toContain('data-inbox-osl-surface="circles"');
    expect(publicCircles).toContain('data-public-circles-network="unavailable"');
    expect(publicCircles).toContain("<strong>OSL Circles</strong>");
    expect(publicCircles).toContain('<span class="status-tag">Unavailable</span>');
    expect(publicCircles).toContain("Public Circles network unavailable.");
    expect(publicCircles).toContain("Private audience posts stay off");
    expect(source).toContain("function circlesDestinationContent");
    expect(source).toContain("publicCirclesUnavailableMarkup()");
    expect(inbox).toContain('if (id === "circles") return circlesDestinationContent()');
  });

  it("offers no action or protected-public claim for unavailable Circles", () => {
    const visibleCopy = publicCircles.replace(/\$\{[^}]+\}/g, "");
    expect(publicCircles).not.toMatch(/<button|href=|data-route|data-home-module/i);
    expect(visibleCopy).not.toMatch(/public .*end-to-end encrypted|enable public|start public|open public|join public|global feed|available now|ready now/i);
    expect(visibleCopy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?|scope/i);
  });
});
