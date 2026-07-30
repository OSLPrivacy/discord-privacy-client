import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function readDoc(relativePath: string): string {
  return readFileSync(new URL(`../../../${relativePath}`, import.meta.url), "utf8");
}

function section(source: string, heading: string): string {
  const lines = source.split("\n");
  const start = lines.findIndex((line) => line.trim() === heading);
  expect(start, `missing section ${heading}`).toBeGreaterThanOrEqual(0);
  const headingLevel = heading.match(/^#+/u)?.[0].length ?? 1;
  const end = lines.findIndex((line, index) => {
    if (index <= start) {
      return false;
    }
    const match = /^(#+)\s/u.exec(line);
    return match !== null && match[1]!.length <= headingLevel;
  });
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n");
}

function parseTable<T extends Record<string, string>>(
  source: string,
  expectedColumns: readonly string[],
): T[] {
  const rows = source
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith("|") && line.endsWith("|"))
    .map((line) => line.slice(1, -1).split("|").map((cell) => cell.trim().replace(/^`|`$/gu, "")));
  const headerIndex = rows.findIndex((row) => row.join("\0") === expectedColumns.join("\0"));
  expect(headerIndex, `missing table header ${expectedColumns.join(",")}`).toBeGreaterThanOrEqual(0);
  expect(rows[headerIndex + 1]?.every((cell) => /^-+$/u.test(cell))).toBe(true);
  const body: T[] = [];
  for (const cells of rows.slice(headerIndex + 2)) {
    if (cells.length !== expectedColumns.length) {
      break;
    }
    body.push(Object.fromEntries(expectedColumns.map((column, index) => [column, cells[index]!])) as T);
  }
  return body;
}

describe("QA documentation contracts", () => {
  it("warm_agent_baseline_snapshot_for_osl_azure_clients_1_and_2", () => {
    const workflow = readDoc("docs/testing/azure-vm-qa-workflow.md");
    const warmAgentBaseline = parseTable<{
      VM: string;
      "Resource group": string;
      Snapshot: string;
      "Lineage tag": string;
      "Required state": string;
      "Agent baseline": string;
      "Included setup": string;
    }>(section(workflow, "### WARM-agent baseline for the crypto A/B pair"), [
      "VM",
      "Resource group",
      "Snapshot",
      "Lineage tag",
      "Required state",
      "Agent baseline",
      "Included setup",
    ]);

    expect(warmAgentBaseline).toHaveLength(2);
    expect(warmAgentBaseline.map((row) => row.VM).sort()).toEqual([
      "OSL-Azure-Client-1",
      "OSL-Azure-Client-2",
    ]);
    for (const row of warmAgentBaseline) {
      expect(row["Resource group"]).toBe("OSL-TWO-CLIENT-LAB");
      expect(row.Snapshot).toBe(`${row.VM}-WARM-agent-baseline-20260726`);
      expect(row["Lineage tag"]).toBe("warm-iteration");
      expect(row["Required state"]).toBe("deallocated");
      expect(row["Agent baseline"]).toBe("registered-logon-task");
      expect(row["Included setup"].split(";").sort()).toEqual([
        "discord-signed-in",
        "osl-identity-created",
      ]);
      expect(row.Snapshot).not.toMatch(/COLD/u);
    }
  });

  it("scrub_tree_merge_plan_records_source_shas_base_tree_and_exclusive_files", () => {
    const plan = readDoc("docs/plans/scrub-tree-merge-plan.md");
    const sourceRows = parseTable<{
      Line: string;
      "Pinned SHA": string;
      Notes: string;
    }>(section(plan, "## Source SHAs"), ["Line", "Pinned SHA", "Notes"]);
    const sourceShas = new Map(sourceRows.map((row) => [row.Line, row["Pinned SHA"]]));
    expect(sourceShas).toEqual(new Map([
      ["main", "16778b297d3ec8d0358b7d3812a95f4f8443e462"],
      ["f1-footprint", "61933d3a4b50e410e3be1d5e05560d955ee72b4c"],
      ["osl-newest-integration", "403cfa2e090bf76ae4cb2950f3febcc72204fc59"],
    ]));

    const baseRows = parseTable<{
      Field: string;
      Value: string;
    }>(section(plan, "## Base Tree"), ["Field", "Value"]);
    const base = new Map(baseRows.map((row) => [row.Field, row.Value]));
    expect(base.get("Base commit")).toBe("16778b297d3ec8d0358b7d3812a95f4f8443e462");
    expect(base.get("Base tree")).toBe("e85e4bd48bfaab810845e307614ab8af238a33d3");

    const exclusiveRows = parseTable<{
      Path: string;
      "Unit coverage": string;
    }>(section(plan, "## Exclusive Files"), ["Path", "Unit coverage"]);
    expect(new Map(exclusiveRows.map((row) => [row.Path, row["Unit coverage"]]))).toEqual(new Map([
      ["apps/osl-hub/src/scrub_imap.rs", "f4, f5, f7, f12, f131"],
      ["crates/store/src/anchor.rs", "a48"],
      ["crates/ipc/src/secure_local_store.rs", "a140"],
      ["docs/plans/scrub-tree-merge-plan.md", "f10"],
      ["docs/design/build-order.md", "f83"],
      ["apps/osl-hub/src/cloud_autoscrub_envelope.rs", "f149"],
    ]));
  });
});
