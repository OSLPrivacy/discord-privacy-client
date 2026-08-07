import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  formatReport,
  TASK_4762_DISCLOSURE_SENTENCE,
  tableRows,
} from "./task-4762-watcher-table.mjs";

const script = fileURLToPath(new URL("./task-4762-watcher-table.mjs", import.meta.url));
const proofNumbers = [4753, 4754, 4756, 4757, 4758];

function parseRows(output: string): Array<Record<string, string>> {
  return output
    .split("\n")
    .filter((line) => line.startsWith("row\t"))
    .map((line) => Object.fromEntries(
      line.split("\t").slice(1).map((part) => {
        const split = part.indexOf("=");
        return [part.slice(0, split), part.slice(split + 1)];
      }),
    ));
}

describe("TASK 4762 watcher disclosure table", () => {
  it("prints exactly sixteen rows and every row names the check and printed number", () => {
    const output = formatReport();
    const rows = parseRows(output);
    expect(rows).toHaveLength(16);
    for (const row of rows) {
      expect(row.check).toMatch(/\S/);
      expect(Number(row.printed)).toBeGreaterThan(0);
      expect(proofNumbers).toContain(Number(row.printed));
    }
    expect(output.trimEnd().split("\n").at(-1)).toBe(
      `summary\t${TASK_4762_DISCLOSURE_SENTENCE}`,
    );
  });

  it("keeps the carrier row at nothing for all four choices, proven by 4756", () => {
    const rows = tableRows().filter((row) => row.watcher === "carrier");
    expect(rows).toHaveLength(4);
    expect(rows.map((row) => [row.choice, row.learns, row.printed])).toEqual([
      ["never show me", "nothing", 4756],
      ["only people I have allowed", "nothing", 4756],
      ["people in the same chat", "nothing", 4756],
      ["anyone", "nothing", 4756],
    ]);
  });

  it("keeps the key server row to drawer-only knowledge, proven by 4753", () => {
    const rows = tableRows().filter((row) => row.watcher === "key server");
    expect(rows).toHaveLength(4);
    for (const row of rows) {
      expect(row.learns).toBe("the drawer that was asked for");
      expect(row.printed).toBe(4753);
      expect(row.learns).not.toMatch(/\bwho was asked about\b|\byes\b|\bno\b/i);
    }
  });

  it("keeps same-chat never-show and allow-list cells at nothing, proven by 4757 and 4758", () => {
    const rows = tableRows().filter((row) => row.watcher === "same-chat watcher");
    expect(rows.find((row) => row.choice === "never show me")).toMatchObject({
      learns: "nothing",
      printed: 4757,
    });
    expect(rows.find((row) => row.choice === "only people I have allowed")).toMatchObject({
      learns: "nothing",
      printed: 4758,
    });
  });

  it("states plainly that anyone lets a stranger with the handle learn yes permanently", () => {
    const row = tableRows().find((candidate) =>
      candidate.watcher === "OSL user lying about who they are" &&
      candidate.choice === "anyone"
    );
    expect(row).toMatchObject({
      learns: "a stranger holding the handle learns yes, and that cannot be un-learned",
      printed: 4754,
    });
  });

  it("the executable emits the same table", () => {
    const output = execFileSync(process.execPath, [script], { encoding: "utf8" });
    expect(parseRows(output)).toHaveLength(16);
    expect(output).toContain(`summary\t${TASK_4762_DISCLOSURE_SENTENCE}`);
  });

  it("dropping any proof exits 1 and names the first cell it can no longer fill", () => {
    for (const proof of proofNumbers) {
      const result = spawnSync(process.execPath, [script, "--drop-check", String(proof)], {
        encoding: "utf8",
      });
      expect(result.status, `proof ${proof}`).toBe(1);
      expect(result.stderr, `proof ${proof}`).toContain("missing proof for cell:");
      expect(result.stderr, `proof ${proof}`).toContain(`proof=${proof}`);
      expect(result.stdout, `proof ${proof}`).toBe("");
    }
  });
});
