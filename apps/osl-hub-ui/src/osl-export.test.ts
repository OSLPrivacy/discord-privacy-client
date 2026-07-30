import { describe, expect, it } from "vitest";
import { exportWorkspaceFile } from "./osl-export";
import type { OslNote } from "./osl-notes";
const note = (kind: OslNote["kind"], body: string): OslNote => ({ id: "a".repeat(32), kind, title: "Budget / 2026", body, folder: "", tags: [], favorite: false, pinned: false, createdAt: 1, updatedAt: 1, deletedAt: null });
describe("portable local exports", () => {
  it("exports evaluated CSV without executing formulas", () => { const output = exportWorkspaceFile(note("spreadsheet", JSON.stringify({ version: 1, rows: 1, columns: 3, cells: { "0:0": "2", "0:1": "3", "0:2": "=A1+B1" } }))); expect(output?.name).toBe("Budget - 2026.csv"); expect(output?.data).toBe("2,3,5"); });
  it("escapes drawing text in standalone SVG", () => { const output = exportWorkspaceFile(note("drawing", JSON.stringify({ version: 1, selectedId: "shape1", items: [{ id: "shape1", type: "text", x: 10, y: 10, width: 100, height: 30, text: "<private>", color: "#06b6d4" }] }))); expect(output?.data).toContain("&lt;private&gt;"); expect(output?.data).not.toContain("<private>"); });
  it("strips workspace metadata from portable OSL JSON", () => { const source = { ...note("note", "hello"), folder: "Private clients", tags: ["secret"] }; const output = exportWorkspaceFile(source, "osl-json"); const parsed = JSON.parse(output?.data ?? ""); expect(parsed.format).toBe("osl-workspace-file-v2"); expect(parsed.privacy.metadataStripped).toBe(true); expect(parsed).not.toHaveProperty("folder"); expect(parsed).not.toHaveProperty("tags"); expect(parsed).not.toHaveProperty("updatedAt"); });
});
