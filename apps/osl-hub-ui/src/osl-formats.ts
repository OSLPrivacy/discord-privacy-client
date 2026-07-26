import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";
import type { ImportedWorkspaceFile } from "./osl-import";
import { parseDocument, parsePresentation, parseSheet } from "./osl-office";

export const OSL_SPREADSHEET_DECODE_MAX_BYTES = 32 * 1024 * 1024;
export interface OslOfficeImport extends ImportedWorkspaceFile { kind: "document" | "spreadsheet" | "presentation"; warnings: string[]; }

const record = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);

export function parseOslOfficeImport(value: unknown): OslOfficeImport | null {
  if (!record(value) || Object.keys(value).length !== 6 || !["kind", "title", "body", "folder", "tags", "warnings"].every((key) => Object.hasOwn(value, key)) || !["document", "spreadsheet", "presentation"].includes(String(value.kind)) || typeof value.title !== "string" || !value.title || Array.from(value.title).length > 60 || typeof value.body !== "string" || value.folder !== "Imports" || !Array.isArray(value.tags) || value.tags.length > 8 || !value.tags.every((tag) => typeof tag === "string" && /^[a-z0-9-]{1,32}$/u.test(tag)) || !Array.isArray(value.warnings) || value.warnings.length > 8 || !value.warnings.every((warning) => typeof warning === "string" && warning.length > 0 && warning.length <= 500)) return null;
  if (value.kind === "spreadsheet" && !parseSheet(value.body) || value.kind === "presentation" && !parsePresentation(value.body) || value.kind === "document" && !parseDocument(value.body)) return null;
  return value as unknown as OslOfficeImport;
}

export async function decodeOslOfficeAsset(assetId: string): Promise<OslOfficeImport | null> {
  if (!isTauriRuntime() || !/^[a-f0-9]{32}$/u.test(assetId)) return null;
  return parseOslOfficeImport(await invoke("decode_osl_office_asset", { assetId }));
}

export function officeImportReceiptFile(imported: OslOfficeImport, sourceName: string): ImportedWorkspaceFile {
  const safeSource = Array.from(sourceName.replace(/[\r\n\u0000-\u001f]/gu, " ")).slice(0, 120).join("").replace(/[<>]/gu, "").trim() || "selected file";
  const safeTitle = imported.title.replace(/[\[\]\r\n\u0000-\u001f]/gu, " ").replace(/\s{2,}/gu, " ").trim() || "Imported file";
  const warnings = imported.warnings.map((warning) => warning.replace(/[\r\n\u0000-\u001f]/gu, " ").replace(/\s{2,}/gu, " ").trim());
  return { kind: "note", title: `${imported.title} import receipt`.slice(0, 120), body: `# Import receipt\n\nImported [[${safeTitle}]] from **${safeSource}**. The immutable original remains encrypted locally.\n\n## Preserved limitations\n\n${warnings.map((warning) => `- ${warning}`).join("\n")}\n`, folder: "Imports", tags: ["import-receipt", imported.kind] };
}
