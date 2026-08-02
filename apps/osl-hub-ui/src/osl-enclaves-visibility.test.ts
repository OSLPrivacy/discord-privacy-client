import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const visibilityTable = readFileSync(
  new URL("../../../docs/design/osl-enclaves-visibility.md", import.meta.url),
  "utf8",
);

const requiredSubjects = [
  "Message content",
  "Roster",
  "Roles",
  "Join and leave",
  "Presence",
  "Timing",
  "Size",
  "Channel names",
  "Governance events",
] as const;

function visibilityRows(source: string): readonly string[][] {
  return source
    .split("\n")
    .filter((line) => line.startsWith("| ") && !line.startsWith("| Visibility") && !line.startsWith("|---"))
    .map((line) => line.split("|").slice(1, -1).map((cell) => cell.trim()));
}

describe("T21-T2 Enclave visibility table", () => {
  it("covers every required visibility subject", () => {
    const rows = visibilityRows(visibilityTable);
    expect(rows.map(([subject]) => subject)).toEqual(requiredSubjects);
  });

  it("does not give admins a broader visibility column than members", () => {
    for (const [subject, member, admin] of visibilityRows(visibilityTable)) {
      expect(admin, `${subject}: admin visibility must equal member visibility`).toBe(member);
    }
  });

  it("marks each row NOT BUILT until it has a code or contract citation", () => {
    for (const [subject, member, admin, relay] of visibilityRows(visibilityTable)) {
      const evidence = `${member} ${admin} ${relay}`;
      expect(
        /NOT BUILT|(?:^|[\s`])(?:[\w/-]+\.(?:rs|ts|md):\d+|03-CONTRACTS\/[^\s`]+\s+§\d)/u.test(evidence),
        `${subject}: needs NOT BUILT or a code/contract citation`,
      ).toBe(true);
    }
  });
});
