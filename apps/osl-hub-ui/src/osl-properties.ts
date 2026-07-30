export type OslPropertyType = "text" | "number" | "checkbox" | "date" | "list";
export interface OslProperty { name: string; type: OslPropertyType; value: string | number | boolean | string[]; }
export interface OslPropertyDocument { properties: OslProperty[]; content: string; }

const MAX_PROPERTIES = 32;
const MAX_FRONTMATTER_BYTES = 16 * 1024;
const validName = (value: string) => /^[\p{L}][\p{L}\p{N} _-]{0,39}$/u.test(value) && !value.trim().endsWith(" ");
const safeValue = (value: string) => value.length <= 500 && !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f\u202a-\u202e\u2066-\u2069]/u.test(value);

function validDate(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/u.exec(value); if (!match) return false;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  return date.getUTCFullYear() === Number(match[1]) && date.getUTCMonth() + 1 === Number(match[2]) && date.getUTCDate() === Number(match[3]);
}

function parseValue(raw: string): OslProperty | null {
  if (!safeValue(raw)) return null;
  if (raw === "true" || raw === "false") return { name: "", type: "checkbox", value: raw === "true" };
  if (/^-?(?:0|[1-9]\d*)(?:\.\d+)?$/u.test(raw)) { const number = Number(raw); if (Number.isFinite(number) && Math.abs(number) <= 1e15) return { name: "", type: "number", value: number }; }
  if (/^\d{4}-\d{2}-\d{2}$/u.test(raw)) return validDate(raw) ? { name: "", type: "date", value: raw } : null;
  if (raw.startsWith("[") && raw.endsWith("]")) {
    try { const value = JSON.parse(raw) as unknown; if (Array.isArray(value) && value.length <= 16 && value.every((item) => typeof item === "string" && item.length > 0 && item.length <= 80 && safeValue(item))) return { name: "", type: "list", value }; } catch { return null; }
    return null;
  }
  if (raw.startsWith('"')) {
    try { const value = JSON.parse(raw) as unknown; return typeof value === "string" && safeValue(value) ? { name: "", type: "text", value } : null; } catch { return null; }
  }
  return safeValue(raw) ? { name: "", type: "text", value: raw } : null;
}

export function parseOslProperties(body: string): OslPropertyDocument | null {
  if (!body.startsWith("---\n") && !body.startsWith("---\r\n")) return { properties: [], content: body };
  const lines = body.split(/\r?\n/u); let closing = -1;
  for (let index = 1; index < Math.min(lines.length, 130); index += 1) if (lines[index] === "---") { closing = index; break; }
  if (closing < 0 || lines.slice(0, closing + 1).join("\n").length > MAX_FRONTMATTER_BYTES || closing - 1 > MAX_PROPERTIES) return null;
  const properties: OslProperty[] = []; const names = new Set<string>();
  for (const line of lines.slice(1, closing)) {
    const match = /^([^:]+):(?:\s(.*))?$/u.exec(line); if (!match) return null;
    const name = match[1].trim(); const folded = name.toLocaleLowerCase(); if (!validName(name) || names.has(folded)) return null;
    const parsed = parseValue(match[2] ?? ""); if (!parsed) return null; names.add(folded); properties.push({ ...parsed, name });
  }
  return { properties, content: lines.slice(closing + 1).join("\n") };
}

const encodedValue = (property: OslProperty): string => property.type === "text" ? JSON.stringify(String(property.value)) : property.type === "list" ? JSON.stringify(property.value) : String(property.value);
export function serializeOslProperties(document: OslPropertyDocument): string {
  if (document.properties.length === 0) return document.content;
  return `---\n${document.properties.map((property) => `${property.name}: ${encodedValue(property)}`).join("\n")}\n---\n${document.content}`;
}

function propertyValue(type: OslPropertyType, raw: string): OslProperty["value"] | null {
  if (type === "text") return safeValue(raw) ? raw : null;
  if (type === "checkbox") return raw === "true" ? true : raw === "false" || raw === "" ? false : null;
  if (type === "number") { if (!/^-?(?:0|[1-9]\d*)(?:\.\d+)?$/u.test(raw)) return null; const number = Number(raw); return Number.isFinite(number) && Math.abs(number) <= 1e15 ? number : null; }
  if (type === "date") return validDate(raw) ? raw : null;
  const list = raw.split(",").map((item) => item.trim()).filter(Boolean); return list.length <= 16 && list.every((item) => item.length <= 80 && safeValue(item)) ? list : null;
}

export function setOslProperty(body: string, name: string, type: OslPropertyType, raw: string): string | null {
  const document = parseOslProperties(body); const cleanName = name.trim(); const value = propertyValue(type, raw);
  if (!document || !validName(cleanName) || value === null) return null;
  const index = document.properties.findIndex((property) => property.name.toLocaleLowerCase() === cleanName.toLocaleLowerCase());
  const property: OslProperty = { name: index >= 0 ? document.properties[index].name : cleanName, type, value };
  if (index >= 0) document.properties[index] = property; else if (document.properties.length < MAX_PROPERTIES) document.properties.push(property); else return null;
  return serializeOslProperties(document);
}

export function removeOslProperty(body: string, name: string): string | null {
  const document = parseOslProperties(body); if (!document) return null;
  const before = document.properties.length; document.properties = document.properties.filter((property) => property.name.toLocaleLowerCase() !== name.toLocaleLowerCase());
  return document.properties.length === before ? null : serializeOslProperties(document);
}

export function displayOslProperty(property: OslProperty): string { return property.type === "list" ? (property.value as string[]).join(", ") : property.type === "checkbox" ? property.value ? "Yes" : "No" : String(property.value); }
