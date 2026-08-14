import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const SOURCE_MARKS = ["DISCORD", "TELEGRAM", "MODS", "COMPLAINTS"] as const;

function specPath(): string | URL {
  const override = process.env.OSL_4850_SPEC_PATH;
  if (override) return resolve(override);
  return new URL("../../../docs/design/osl-enclaves-beacons-roles-best-of-spec.md", import.meta.url);
}

function sourceSections(source: string): string[] {
  return [...source.matchAll(/^## Source section: (DISCORD|TELEGRAM|MODS|COMPLAINTS)$/gmu)]
    .map((match) => match[1]);
}

type SpecRow = {
  id: string;
  cells: string[];
  line: string;
};

function mergedRows(source: string): SpecRow[] {
  return source
    .split("\n")
    .filter((line) => line.startsWith("| BS-"))
    .map((line) => {
      const cells = line.split("|").slice(1, -1).map((cell) => cell.trim());
      return { id: cells[0] ?? "(missing id)", cells, line };
    });
}

function rowSourceMarks(row: SpecRow): string[] {
  return SOURCE_MARKS.filter((mark) => row.line.includes(mark));
}

describe("Task 4850 Enclaves best-of source spec", () => {
  const source = readFileSync(specPath(), "utf8");

  it("has exactly the four requested source sections", () => {
    const sections = sourceSections(source);
    console.log(`TASK4850_SOURCE_SECTIONS=${sections.length} ${sections.join(",")}`);
    expect(sections).toEqual(["DISCORD", "TELEGRAM", "MODS", "COMPLAINTS"]);
  });

  it("has at least 120 merged settings rows", () => {
    const rows = mergedRows(source);
    console.log(`TASK4850_MERGED_ROWS=${rows.length}`);
    expect(rows.length).toBeGreaterThanOrEqual(120);
  });

  it("marks every merged settings row with at least one requested sweep source", () => {
    for (const row of mergedRows(source)) {
      const marks = rowSourceMarks(row);
      expect(
        marks.length,
        `${row.id}: row is missing one of ${SOURCE_MARKS.join(", ")} in its source marks`,
      ).toBeGreaterThan(0);
      expect(row.cells[4], `${row.id}: source marks cell is missing`).toBeTruthy();
    }
  });
});
