import { defaultDocumentBody, parseDrawing, parsePresentation, parseSheet, type DrawingItem, type DrawingModel, type SheetModel } from "./osl-office";
import type { OslDocumentKind } from "./osl-notes";
import { parseOslCreativeProject } from "./osl-assets";

export const OSL_IMPORT_MAX_BYTES = 10 * 1024 * 1024;
const OSL_PROJECT_BODY_MAX_BYTES = 256 * 1024;
export interface ImportedWorkspaceFile { kind: OslDocumentKind; title: string; body: string; folder: string; tags: string[]; }

const byteLength = (value: string) => new TextEncoder().encode(value).byteLength;
const bodyFits = (value: string) => byteLength(value) <= OSL_PROJECT_BODY_MAX_BYTES;
const titleFromName = (name: string) => Array.from(name.replace(/\.[^.]+$/u, "").trim()).slice(0, 60).join("") || "Imported file";

function parseDelimited(text: string, delimiter: "," | "\t"): string[][] | null {
  const rows: string[][] = []; let row: string[] = []; let cell = ""; let quoted = false;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (character === '"') { if (quoted && text[index + 1] === '"') { cell += '"'; index += 1; } else quoted = !quoted; }
    else if (!quoted && character === delimiter) { row.push(cell); cell = ""; }
    else if (!quoted && (character === "\n" || character === "\r")) { if (character === "\r" && text[index + 1] === "\n") index += 1; row.push(cell); rows.push(row); row = []; cell = ""; if (rows.length > 200) return null; }
    else cell += character;
    if (cell.length > 2_000) return null;
  }
  if (quoted) return null; row.push(cell); if (row.some(Boolean) || rows.length === 0) rows.push(row);
  return rows.length <= 200 && Math.max(...rows.map((item) => item.length)) <= 50 ? rows : null;
}

function sheetBody(text: string, delimiter: "," | "\t"): string | null {
  const rows = parseDelimited(text, delimiter); if (!rows) return null;
  const model: SheetModel = { version: 3, rows: Math.max(20, rows.length), columns: Math.max(8, ...rows.map((row) => row.length)), cells: {}, formats: {}, frozenRows: 0, frozenColumns: 0, filters: {}, sort: null, validations: {}, conditions: {}, charts: [] };
  rows.forEach((row, rowIndex) => row.forEach((cell, columnIndex) => { if (cell) model.cells[`${rowIndex}:${columnIndex}`] = cell; }));
  const body = JSON.stringify(model); return bodyFits(body) ? body : null;
}

function attribute(source: string, name: string): string | null { return new RegExp(`\\s${name}=["']([^"']+)["']`, "iu").exec(source)?.[1] ?? null; }
function numberAttribute(source: string, name: string, fallback: number): number { const value = Number(attribute(source, name)); return Number.isFinite(value) && value >= 0 && value <= 2_000 ? value : fallback; }
function safeColor(source: string): string { const value = attribute(source, "fill")?.toLowerCase() ?? "#06b6d4"; return ["#06b6d4", "#8b5cf6", "#49c58a", "#f2b84b", "#ef626b", "#e8e8e8"].includes(value) ? value : "#06b6d4"; }
function importedDrawingItem(item: Pick<DrawingItem, "id" | "type" | "x" | "y" | "width" | "height" | "text" | "color">): DrawingItem { return { ...item, stroke: item.color, strokeWidth: 4, opacity: 100, rotation: 0, locked: false, hidden: false, name: item.type, gradientTo: null, points: [] }; }

function svgBody(text: string): string | null {
  if (text.length > OSL_IMPORT_MAX_BYTES || !/^\s*<svg[\s>]/iu.test(text) || /<(?:script|foreignObject)|\son\w+\s*=|\b(?:href|src)\s*=|url\s*\(/iu.test(text)) return null;
  const items: DrawingItem[] = [];
  for (const match of text.matchAll(/<(rect|ellipse|circle)\b[^>]*>/giu)) {
    const source = match[0]; const type = match[1].toLowerCase(); const itemId = `svg${items.length.toString(36).padStart(4, "0")}`;
    if (type === "rect") items.push(importedDrawingItem({ id: itemId, type: "rectangle", x: numberAttribute(source, "x", 20), y: numberAttribute(source, "y", 20), width: numberAttribute(source, "width", 120), height: numberAttribute(source, "height", 80), text: "", color: safeColor(source) }));
    else { const radiusX = type === "circle" ? numberAttribute(source, "r", 50) : numberAttribute(source, "rx", 60); const radiusY = type === "circle" ? radiusX : numberAttribute(source, "ry", 40); const centerX = numberAttribute(source, "cx", 100); const centerY = numberAttribute(source, "cy", 100); items.push(importedDrawingItem({ id: itemId, type: "ellipse", x: Math.max(0, centerX - radiusX), y: Math.max(0, centerY - radiusY), width: radiusX * 2, height: radiusY * 2, text: "", color: safeColor(source) })); }
    if (items.length >= 500) break;
  }
  for (const match of text.matchAll(/<text\b([^>]*)>([^<]{0,500})<\/text>/giu)) { if (items.length >= 500) break; const source = match[1]; items.push(importedDrawingItem({ id: `svg${items.length.toString(36).padStart(4, "0")}`, type: "text", x: numberAttribute(source, "x", 40), y: Math.max(0, numberAttribute(source, "y", 60) - 24), width: 180, height: 36, text: match[2].replace(/&lt;/gu, "<").replace(/&gt;/gu, ">").replace(/&amp;/gu, "&"), color: safeColor(source) })); }
  const model: DrawingModel = { version: 5, selectedId: items[0]?.id ?? null, items, prototype: { startItemId: null, links: [], tokens: {} }, artboards: [{ id: "board001", name: "Imported SVG", x: 0, y: 0, width: 960, height: 540, background: "#e8e8e8" }], selectedArtboardId: "board001", components: [], instances: {}, constraints: {}, workspace: { mode: "artboards", tool: "select", viewport: { x: 0, y: 0, zoom: 1 }, sources: [] } }; const body = JSON.stringify(model); return bodyFits(body) ? body : null;
}

function decodeHtmlDocument(text: string): string | null {
  if (!/^\s*(?:<!doctype\s+html[^>]*>\s*)?<html[\s>]/iu.test(text) || /<(?:script|style|iframe|object|embed|svg|math)\b|\son\w+\s*=|\b(?:href|src)\s*=|url\s*\(/iu.test(text)) return null;
  const entities = (value: string) => value.replace(/&(#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos|nbsp);/giu, (_, entity: string) => { const key = entity.toLowerCase(); if (key === "amp") return "&"; if (key === "lt") return "<"; if (key === "gt") return ">"; if (key === "quot") return '"'; if (key === "apos") return "'"; if (key === "nbsp") return " "; const number = key.startsWith("#x") ? Number.parseInt(key.slice(2), 16) : Number.parseInt(key.slice(1), 10); return Number.isFinite(number) && number <= 0x10ffff ? String.fromCodePoint(number) : ""; });
  const body = entities(text.replace(/<(?:br|hr)\s*\/?\s*>/giu, "\n").replace(/<\/(?:p|div|h[1-6]|li|blockquote|tr|section|article)>/giu, "\n").replace(/<[^>]{1,2000}>/gu, "")).replace(/[ \t]+\n/gu, "\n").replace(/\n{3,}/gu, "\n\n").trim(); return body && bodyFits(body) ? body : null;
}

function decodeRtfDocument(text: string): string | null {
  if (!/^\{\\rtf1\b/u.test(text) || /\\(?:object|objdata|pict|filetbl|field|include|link)\b/iu.test(text)) return null; let output = ""; let depth = 0; let skipDepth = -1;
  for (let index = 0; index < text.length && output.length <= 262_144; index += 1) { const character = text[index]; if (character === "{") { depth += 1; const destination = /^\\(?:\*\\)?(?:fonttbl|colortbl|stylesheet|info|header|footer)\b/iu.exec(text.slice(index + 1)); if (destination && skipDepth < 0) skipDepth = depth; continue; } if (character === "}") { if (depth === skipDepth) skipDepth = -1; depth = Math.max(0, depth - 1); continue; } if (skipDepth >= 0) continue; if (character !== "\\") { if (character !== "\r" && character !== "\n") output += character; continue; } const next = text[index + 1]; if (next === "\\" || next === "{" || next === "}") { output += next; index += 1; continue; } if (next === "'") { const byte = Number.parseInt(text.slice(index + 2, index + 4), 16); if (Number.isFinite(byte)) output += String.fromCharCode(byte); index += 3; continue; } const control = /^\\([a-z]+)(-?\d+)? ?/iu.exec(text.slice(index)); if (!control) continue; index += control[0].length - 1; const word = control[1].toLowerCase(); if (word === "par" || word === "line") output += "\n"; else if (word === "tab") output += "\t"; else if (word === "u" && control[2]) { const code = Number(control[2]); output += String.fromCharCode(code < 0 ? code + 65536 : code); if (text[index + 1] === "?") index += 1; } }
  const body = output.replace(/\n{3,}/gu, "\n\n").trim(); return depth === 0 && body && bodyFits(body) ? body : null;
}

function oslJson(text: string): ImportedWorkspaceFile | null {
  try {
    const value = JSON.parse(text) as Record<string, unknown>; const keys = Object.keys(value);
    const legacy = value.format === "osl-workspace-file-v1"; const privateExport = value.format === "osl-workspace-file-v2" && typeof value.privacy === "object" && value.privacy !== null && !Array.isArray(value.privacy) && Object.keys(value.privacy).length === 1 && (value.privacy as Record<string, unknown>).metadataStripped === true;
    const allowed = legacy ? ["format", "kind", "title", "body", "folder", "tags"] : ["format", "privacy", "kind", "title", "body"];
    if (!keys.every((key) => allowed.includes(key)) || !legacy && !privateExport || !["note", "document", "spreadsheet", "drawing", "presentation", "photo", "video", "audio", "model3d"].includes(String(value.kind)) || typeof value.title !== "string" || byteLength(value.title) > 240 || typeof value.body !== "string" || !bodyFits(value.body) || legacy && (typeof value.folder !== "string" || byteLength(value.folder) > 80 || !Array.isArray(value.tags) || value.tags.length > 16 || !value.tags.every((tag) => typeof tag === "string" && byteLength(tag) <= 32))) return null;
    const kind = value.kind as OslDocumentKind;
    if (kind === "spreadsheet" && !parseSheet(value.body) || kind === "drawing" && !parseDrawing(value.body) || kind === "presentation" && !parsePresentation(value.body)) return null;
    if (["photo", "video", "audio", "model3d"].includes(kind) && !parseOslCreativeProject(value.body)) return null;
    return { kind, title: value.title, body: value.body, folder: legacy ? value.folder as string : "", tags: legacy ? value.tags as string[] : [] };
  } catch { return null; }
}

export function importWorkspaceText(name: string, text: string): ImportedWorkspaceFile | null {
  if (new TextEncoder().encode(text).byteLength > OSL_IMPORT_MAX_BYTES || text.includes("\0")) return null;
  const extension = name.toLowerCase().split(".").pop() ?? ""; const title = titleFromName(name);
  if (["md", "markdown"].includes(extension)) return bodyFits(text) ? { kind: "note", title, body: text, folder: "Imports", tags: ["imported"] } : null;
  if (extension === "txt") return bodyFits(text) ? { kind: "document", title, body: text, folder: "Imports", tags: ["imported"] } : null;
  if (["html", "htm"].includes(extension)) { const body = decodeHtmlDocument(text); return body ? { kind: "document", title, body, folder: "Imports", tags: ["imported", "html"] } : null; }
  if (extension === "rtf") { const body = decodeRtfDocument(text); return body ? { kind: "document", title, body, folder: "Imports", tags: ["imported", "rtf"] } : null; }
  if (extension === "csv" || extension === "tsv") { const body = sheetBody(text, extension === "csv" ? "," : "\t"); return body ? { kind: "spreadsheet", title, body, folder: "Imports", tags: ["imported"] } : null; }
  if (extension === "svg") { const body = svgBody(text); return body ? { kind: "drawing", title, body, folder: "Imports", tags: ["imported"] } : null; }
  if (extension === "json") return oslJson(text);
  return null;
}

export function textImportReceiptFile(imported: ImportedWorkspaceFile, sourceName: string): ImportedWorkspaceFile | null {
  const extension = sourceName.toLowerCase().split(".").pop() ?? "";
  const limitation = ["html", "htm"].includes(extension) ? "Text was preserved. Page layout, styles, links, images, forms, scripts, and remote resources remain only in the encrypted original." : extension === "rtf" ? "Text was preserved. Rich formatting, images, fields, headers, footers, and embedded objects remain only in the encrypted original." : extension === "svg" ? "Supported rectangles, ellipses, circles, and text were converted. Paths, transforms, strokes, filters, fonts, links, scripts, and unsupported SVG content remain only in the encrypted original." : null;
  if (!limitation) return null;
  const safeSource = Array.from(sourceName.replace(/[\r\n\u0000-\u001f]/gu, " ")).slice(0, 120).join("").replace(/[<>]/gu, "").trim() || "selected file";
  const safeTitle = imported.title.replace(/[\[\]\r\n\u0000-\u001f]/gu, " ").replace(/\s{2,}/gu, " ").trim() || "Imported file";
  return { kind: "note", title: `${imported.title} import receipt`.slice(0, 120), body: `# Import receipt\n\nImported [[${safeTitle}]] from **${safeSource}**. The byte-exact original remains encrypted locally.\n\n## Preserved limitation\n\n- ${limitation}\n`, folder: "Imports", tags: ["import-receipt", extension === "htm" ? "html" : extension] };
}

export function blankImportedFile(kind: OslDocumentKind): ImportedWorkspaceFile { return { kind, title: "", body: defaultDocumentBody(kind), folder: "", tags: [] }; }
