import { documentPlainText, parseDocument, parseDrawing, parsePresentation, parseSheet, sheetToCsv } from "./osl-office";
import type { OslNote } from "./osl-notes";

export type OslExportFormat = "markdown" | "text" | "csv" | "svg" | "html" | "osl-json";
export interface WorkspaceExport { name: string; mime: string; data: string; }

const safeName = (value: string) => value.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/gu, "-").replace(/[. ]+$/gu, "").slice(0, 100) || "Untitled";
const xml = (value: string) => value.replace(/[&<>"']/gu, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&apos;" })[character] ?? character);
const html = (value: string) => value.replace(/[&<>"']/gu, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] ?? character);

function drawingSvg(note: OslNote): string | null {
  const drawing = parseDrawing(note.body); if (!drawing) return null;
  const width = Math.max(960, ...drawing.artboards.map((board) => board.x + board.width)); const height = Math.max(540, ...drawing.artboards.map((board) => board.y + board.height));
  const gradients = drawing.items.filter((item) => item.gradientTo).map((item) => `<linearGradient id="g-${item.id}"><stop stop-color="${item.color}"/><stop offset="1" stop-color="${item.gradientTo}"/></linearGradient>`).join("");
  const artboards = drawing.artboards.map((board) => `<rect x="${board.x}" y="${board.y}" width="${board.width}" height="${board.height}" fill="${board.background}"/>`).join("");
  const items = drawing.items.filter((item) => !item.hidden).map((item) => { const common = `opacity="${item.opacity / 100}" transform="rotate(${item.rotation} ${item.x + item.width / 2} ${item.y + item.height / 2})"`; const fill = item.gradientTo ? `url(#g-${item.id})` : item.color; if (item.type === "rectangle") return `<rect ${common} x="${item.x}" y="${item.y}" width="${item.width}" height="${item.height}" rx="8" fill="${fill}" stroke="${item.stroke}" stroke-width="${item.strokeWidth}"/>`; if (item.type === "ellipse") return `<ellipse ${common} cx="${item.x + item.width / 2}" cy="${item.y + item.height / 2}" rx="${item.width / 2}" ry="${item.height / 2}" fill="${fill}" stroke="${item.stroke}" stroke-width="${item.strokeWidth}"/>`; if (["line", "arrow"].includes(item.type)) return `<line ${common} x1="${item.x}" y1="${item.y}" x2="${item.x + item.width}" y2="${item.y + item.height}" stroke="${item.stroke}" stroke-width="${item.strokeWidth}"/>`; if (item.type === "path") return `<polyline ${common} points="${item.points.map((point) => `${point.x},${point.y}`).join(" ")}" fill="none" stroke="${item.stroke}" stroke-width="${item.strokeWidth}"/>`; return `<text ${common} x="${item.x}" y="${item.y + 24}" fill="${fill}">${xml(item.text)}</text>`; }).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="OSL drawing"><defs>${gradients}</defs>${artboards}${items}</svg>\n`;
}

function presentationHtml(note: OslNote): string | null {
  const deck = parsePresentation(note.body); if (!deck) return null;
  const slides = deck.slides.map((slide) => `<section class="${slide.theme}"><h1>${html(slide.title)}</h1><p>${html(slide.body).replace(/\n/gu, "<br>")}</p></section>`).join("\n");
  return `<!doctype html><html><head><meta charset="utf-8"><title>${html(note.title || "OSL presentation")}</title><style>body{margin:0;background:#111;color:#fff;font:24px system-ui}section{box-sizing:border-box;min-height:100vh;padding:10vh 12vw;page-break-after:always}.cyan{background:#062b35}.midnight{background:#080d1c}.paper{background:#faf7ef;color:#191919}.sunset{background:#47202c}h1{font-size:3em}p{line-height:1.5}</style></head><body>${slides}</body></html>\n`;
}

export function exportWorkspaceFile(note: OslNote, requested?: OslExportFormat): WorkspaceExport | null {
  const format = requested ?? (note.kind === "note" ? "markdown" : note.kind === "document" ? "text" : note.kind === "spreadsheet" ? "csv" : note.kind === "drawing" ? "svg" : "html");
  const base = safeName(note.title);
  if (format === "osl-json") return { name: `${base}.osl.json`, mime: "application/json", data: JSON.stringify({ format: "osl-workspace-file-v2", privacy: { metadataStripped: true }, kind: note.kind, title: note.title, body: note.body }, null, 2) };
  if (format === "markdown" && note.kind === "note") return { name: `${base}.md`, mime: "text/markdown", data: note.body };
  if (format === "text" && note.kind === "document") { const document = parseDocument(note.body); return document ? { name: `${base}.txt`, mime: "text/plain", data: documentPlainText(document) } : null; }
  if (format === "csv" && note.kind === "spreadsheet") { const sheet = parseSheet(note.body); return sheet ? { name: `${base}.csv`, mime: "text/csv", data: sheetToCsv(sheet) } : null; }
  if (format === "svg" && note.kind === "drawing") { const data = drawingSvg(note); return data ? { name: `${base}.svg`, mime: "image/svg+xml", data } : null; }
  if (format === "html" && note.kind === "presentation") { const data = presentationHtml(note); return data ? { name: `${base}.html`, mime: "text/html", data } : null; }
  return null;
}
