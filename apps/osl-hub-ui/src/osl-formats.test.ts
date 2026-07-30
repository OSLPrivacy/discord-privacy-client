import { describe, expect, it } from "vitest";
import { officeImportReceiptFile, parseOslOfficeImport } from "./osl-formats";

const body = JSON.stringify({ version: 1, rows: 20, columns: 8, cells: { "0:0": "Revenue" } });

describe("native Office import receipt", () => {
  it("accepts only a strict editable spreadsheet result", () => {
    const receipt = { kind: "spreadsheet", title: "Budget", body, folder: "Imports", tags: ["imported", "spreadsheet"], warnings: ["Charts remain in the original."] };
    expect(parseOslOfficeImport(receipt)).toEqual(receipt);
    expect(parseOslOfficeImport({ ...receipt, remoteUrl: "https://example.com" })).toBeNull();
    expect(parseOslOfficeImport({ ...receipt, body: "{}" })).toBeNull();
  });
  it("requires structured documents at the native boundary", () => { const documentBody = JSON.stringify({ format: "osl-document-v2", page: { size: "letter", orientation: "portrait", margin: 1, columns: 1, header: "", footer: "" }, blocks: [{ id: "block0000", type: "paragraph", text: "Private", checked: false, align: "left" }] }); const receipt = { kind: "document", title: "Letter", body: documentBody, folder: "Imports", tags: ["imported", "document"], warnings: ["Layout remains in the original."] }; expect(parseOslOfficeImport(receipt)).toEqual(receipt); expect(parseOslOfficeImport({ ...receipt, body: JSON.stringify({ format: "osl-document-v2" }) })).toBeNull(); });
  it("builds a bounded encrypted limitations note without markup injection", () => { const imported = parseOslOfficeImport({ kind: "spreadsheet", title: "Budget", body, folder: "Imports", tags: ["imported", "spreadsheet"], warnings: ["Charts\nremain in the original."] })!; const receipt = officeImportReceiptFile(imported, "<private>\nbook.xlsx"); expect(receipt).toMatchObject({ kind: "note", folder: "Imports", tags: ["import-receipt", "spreadsheet"] }); expect(receipt.body).toContain("private book.xlsx"); expect(receipt.body).toContain("Charts remain in the original."); expect(receipt.body).not.toContain("<private>"); expect(receipt.body).not.toContain("Charts\nremain"); });
});
