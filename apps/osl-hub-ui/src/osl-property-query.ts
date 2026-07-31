import type { OslNote } from "./osl-notes";
import { parseOslProperties, type OslProperty, type OslPropertyType } from "./osl-properties";

export const OSL_PROPERTY_QUERY_LIMITS = Object.freeze({ notes: 5_000, predicates: 16, sorts: 4, columns: 24, bytes: 16 * 1024 });

export type OslPropertyQueryOperator = "eq" | "not-eq" | "contains" | "gt" | "gte" | "lt" | "lte" | "is-empty";
export interface OslPropertyQueryPredicate { property: string; type: OslPropertyType; operator: OslPropertyQueryOperator; value?: string | number | boolean | string[]; }
export interface OslPropertyQuerySort { property: string; type: OslPropertyType; direction: "asc" | "desc"; }
export interface OslPropertyQuery { version: 1; predicates: OslPropertyQueryPredicate[]; sorts: OslPropertyQuerySort[]; columns: string[]; }
export interface OslPropertyQueryColumn { name: string; property: OslProperty | null; }
export interface OslPropertyQueryRow { note: OslNote; columns: OslPropertyQueryColumn[]; }

const TYPES: OslPropertyType[] = ["text", "number", "checkbox", "date", "list"];
const OPERATORS: OslPropertyQueryOperator[] = ["eq", "not-eq", "contains", "gt", "gte", "lt", "lte", "is-empty"];
const UNSAFE = /[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069]/u;
const record = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const exactKeys = (value: Record<string, unknown>, keys: string[]) => Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const safeString = (value: unknown, maximum: number) => typeof value === "string" && value.length <= maximum && !UNSAFE.test(value);
const safeName = (value: unknown): value is string => safeString(value, 40) && (value as string).length > 0 && (value as string).trim() === value;
const foldedName = (value: string) => value.toLocaleLowerCase("en-US");

function validDate(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/u.exec(value); if (!match) return false;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  return date.getUTCFullYear() === Number(match[1]) && date.getUTCMonth() + 1 === Number(match[2]) && date.getUTCDate() === Number(match[3]);
}

function validTypedValue(type: OslPropertyType, value: unknown): value is OslProperty["value"] {
  if (type === "text") return safeString(value, 500);
  if (type === "number") return typeof value === "number" && Number.isFinite(value) && Math.abs(value) <= 1e15;
  if (type === "checkbox") return typeof value === "boolean";
  if (type === "date") return typeof value === "string" && validDate(value);
  return Array.isArray(value) && value.length <= 16 && value.every((item) => safeString(item, 80) && item.length > 0);
}

function parsePredicate(raw: unknown): OslPropertyQueryPredicate | null {
  if (!record(raw) || !safeName(raw.property) || !TYPES.includes(raw.type as OslPropertyType) || !OPERATORS.includes(raw.operator as OslPropertyQueryOperator)) return null;
  const type = raw.type as OslPropertyType; const operator = raw.operator as OslPropertyQueryOperator;
  if (operator === "is-empty") return exactKeys(raw, ["property", "type", "operator"]) ? { property: raw.property, type, operator } : null;
  if (!exactKeys(raw, ["property", "type", "operator", "value"])) return null;
  if (operator === "contains") {
    if ((type !== "text" && type !== "list") || !safeString(raw.value, type === "list" ? 80 : 500)) return null;
  } else if (["gt", "gte", "lt", "lte"].includes(operator)) {
    if ((type !== "number" && type !== "date") || !validTypedValue(type, raw.value)) return null;
  } else if (!validTypedValue(type, raw.value)) return null;
  return { property: raw.property, type, operator, value: raw.value as OslProperty["value"] };
}

function parseSort(raw: unknown): OslPropertyQuerySort | null {
  if (!record(raw) || !exactKeys(raw, ["property", "type", "direction"]) || !safeName(raw.property) || !TYPES.includes(raw.type as OslPropertyType) || (raw.direction !== "asc" && raw.direction !== "desc")) return null;
  return { property: raw.property, type: raw.type as OslPropertyType, direction: raw.direction };
}

/** Parses the portable v1 query format. Unknown fields and non-canonical values fail closed. */
export function parseOslPropertyQuery(raw: unknown): OslPropertyQuery | null {
  if (!record(raw) || !exactKeys(raw, ["version", "predicates", "sorts", "columns"]) || raw.version !== 1 || !Array.isArray(raw.predicates) || raw.predicates.length > OSL_PROPERTY_QUERY_LIMITS.predicates || !Array.isArray(raw.sorts) || raw.sorts.length > OSL_PROPERTY_QUERY_LIMITS.sorts || !Array.isArray(raw.columns) || raw.columns.length > OSL_PROPERTY_QUERY_LIMITS.columns) return null;
  try { if (new TextEncoder().encode(JSON.stringify(raw)).length > OSL_PROPERTY_QUERY_LIMITS.bytes) return null; } catch { return null; }
  const predicates = raw.predicates.map(parsePredicate); const sorts = raw.sorts.map(parseSort);
  if (predicates.some((item) => item === null) || sorts.some((item) => item === null) || !raw.columns.every(safeName)) return null;
  const columns = raw.columns as string[]; const columnNames = columns.map(foldedName); const sortNames = (sorts as OslPropertyQuerySort[]).map((sort) => `${foldedName(sort.property)}\0${sort.type}`);
  if (new Set(columnNames).size !== columnNames.length || new Set(sortNames).size !== sortNames.length) return null;
  return { version: 1, predicates: predicates as OslPropertyQueryPredicate[], sorts: sorts as OslPropertyQuerySort[], columns: [...columns] };
}

const findProperty = (properties: OslProperty[], name: string) => properties.find((property) => foldedName(property.name) === foldedName(name));
const valuesEqual = (left: OslProperty["value"], right: OslProperty["value"]) => Array.isArray(left) && Array.isArray(right) ? left.length === right.length && left.every((value, index) => value === right[index]) : left === right;

function matches(properties: OslProperty[], predicate: OslPropertyQueryPredicate): boolean {
  const property = findProperty(properties, predicate.property);
  if (!property) return predicate.operator === "is-empty";
  if (property.type !== predicate.type) return false;
  if (predicate.operator === "is-empty") return property.value === "" || Array.isArray(property.value) && property.value.length === 0;
  if (predicate.operator === "contains") return property.type === "text" ? (property.value as string).includes(String(predicate.value)) : property.type === "list" && (property.value as string[]).includes(String(predicate.value));
  if (predicate.operator === "eq" || predicate.operator === "not-eq") return valuesEqual(property.value, predicate.value as OslProperty["value"]) === (predicate.operator === "eq");
  const comparison = property.type === "number" ? (property.value as number) - (predicate.value as number) : property.type === "date" ? String(property.value).localeCompare(String(predicate.value), "en") : Number.NaN;
  return predicate.operator === "gt" ? comparison > 0 : predicate.operator === "gte" ? comparison >= 0 : predicate.operator === "lt" ? comparison < 0 : comparison <= 0;
}

function compareProperties(left: OslProperty | undefined, right: OslProperty | undefined, type: OslPropertyType): number {
  const leftValid = left?.type === type; const rightValid = right?.type === type;
  if (!leftValid || !rightValid) return leftValid ? -1 : rightValid ? 1 : 0;
  if (type === "number") return (left!.value as number) - (right!.value as number);
  if (type === "checkbox") return Number(left!.value) - Number(right!.value);
  const leftText = Array.isArray(left!.value) ? left!.value.join("\0") : String(left!.value); const rightText = Array.isArray(right!.value) ? right!.value.join("\0") : String(right!.value);
  return leftText < rightText ? -1 : leftText > rightText ? 1 : 0;
}

/** Executes entirely in memory. Invalid queries, oversized inputs, and unsafe frontmatter return null. */
export function runOslPropertyQuery(notes: readonly OslNote[], rawQuery: unknown): OslPropertyQueryRow[] | null {
  const query = parseOslPropertyQuery(rawQuery); if (!query || notes.length > OSL_PROPERTY_QUERY_LIMITS.notes) return null;
  const parsed = notes.map((note, index) => ({ note, index, document: parseOslProperties(note.body) }));
  if (parsed.some((row) => row.document === null)) return null;
  const rows = parsed.filter((row) => query.predicates.every((predicate) => matches(row.document!.properties, predicate)));
  rows.sort((left, right) => {
    for (const sort of query.sorts) {
      const leftProperty = findProperty(left.document!.properties, sort.property); const rightProperty = findProperty(right.document!.properties, sort.property);
      const leftValid = leftProperty?.type === sort.type; const rightValid = rightProperty?.type === sort.type;
      if (leftValid !== rightValid) return leftValid ? -1 : 1;
      const result = compareProperties(leftProperty, rightProperty, sort.type); if (result) return sort.direction === "asc" ? result : -result;
    }
    return left.index - right.index;
  });
  return rows.map(({ note, document }) => ({ note, columns: query.columns.map((name) => ({ name, property: findProperty(document!.properties, name) ?? null })) }));
}
