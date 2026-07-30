#!/usr/bin/env node

// WHAT THIS GATE CANNOT DO, BY CONSTRUCTION (recorded 2026-07-27)
//
// This is a STRING gate. It answers exactly one question: does this text
// contain a phrase section D forbids? It cannot answer whether the code behind
// the text is reachable.
//
// The case that proves the limit: `osl_notes` has UI command strings that are
// entirely truthful sentences, and no backend command registered anywhere. This
// gate reads the string, finds nothing forbidden, and passes -- correctly, on
// its own terms. The sentence is not a lie about what the code does; it is a
// true sentence about code that is not wired. No property of the STRING
// distinguishes it from the same sentence about working code.
//
// A narrow reachability check bolted on here would cover almost nothing --
// registry ids are not command names, and most UI strings carry no capability
// marker at all -- while making this gate LOOK more complete. That is the
// false-confidence failure this file exists to prevent.
//
// That class is caught by a DIFFERENT gate with a different input: a
// reachability sweep over generate_handler! and the call graph. It belongs with
// whoever owns app Rust. See "F0" in docs/design/osl-public-claim-allowlist.md.

import { promises as fs } from "node:fs";
import { createHash } from "node:crypto";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(import.meta.dirname, "..");
const ALLOWLIST_PATH = path.join(
  REPO_ROOT,
  "docs/design/osl-public-claim-allowlist.md",
);
const APP_SRC_ROOT = path.join(REPO_ROOT, "apps/osl-hub-ui/src");
const RUST_APP_SRC_ROOT = path.join(REPO_ROOT, "apps/osl-hub/src");
const README_PATH = path.join(REPO_ROOT, "README.md");
const GATE_SOURCE_PATH = fileURLToPath(import.meta.url);
const GATE_CONTRACT_PATTERN =
  /^> Claim-gate source SHA-256: `([0-9a-f]{64})`$/m;

const MIN_BANNED_PHRASES = 8; // Prevents a malformed section-D parse from approving everything.
const MIN_TS_STRING_LITERALS = 300; // Ensures the app copy scan cannot pass after extracting nothing.
const MIN_RUST_STRING_LITERALS = 40; // Ensures the Rust high-precision subset cannot pass after extracting nothing.
const MIN_README_BYTES = 1; // Ensures the public README claim surface was actually scanned.

const REQUIRED_ATTACHMENT_BANS = [
  "discord attachment scanning defeated",
  "defeats discord attachment scanning",
  "discord cannot scan attachments",
  "discord sees only decoys",
  "discord's attachment scanner is defeated by osl",
  "osl bypasses discord's attachment inspection",
  "discord receives harmless cover files instead of the attachment",
  "uploaded files are opaque to discord's scanners",
];

function repoRelative(filePath) {
  return path.relative(REPO_ROOT, filePath).split(path.sep).join("/");
}

async function readUtf8(filePath) {
  return fs.readFile(filePath, "utf8");
}

function decodeClaimEntity(entity) {
  const body = entity.slice(1, -1).toLowerCase();
  if (/^#\d+$/.test(body)) {
    const codePoint = Number.parseInt(body.slice(1), 10);
    return codePoint <= 0x10ffff ? String.fromCodePoint(codePoint) : entity;
  }
  if (/^#x[0-9a-f]+$/.test(body)) {
    const codePoint = Number.parseInt(body.slice(2), 16);
    return codePoint <= 0x10ffff ? String.fromCodePoint(codePoint) : entity;
  }
  return new Map([
    ["amp", "&"],
    ["apos", "'"],
    ["gt", ">"],
    ["lt", "<"],
    ["nbsp", " "],
    ["quot", '"'],
  ]).get(body) ?? entity;
}

function normalizedClaimTextWithSourceMap(source) {
  const characters = [];
  const sourceIndexes = [];
  const blockTags =
    /^(?:address|article|aside|blockquote|br|dd|div|dl|dt|footer|form|h[1-6]|header|hr|li|main|nav|ol|p|section|table|td|th|tr|ul)$/i;

  function append(character, sourceIndex) {
    const normalized = character === "’" || character === "‘" ? "'" : character;
    if (normalized === "\n" || normalized === "\r") {
      if (characters.length > 0 && characters.at(-1) !== "\n") {
        characters.push("\n");
        sourceIndexes.push(sourceIndex);
      }
      return;
    }
    if (normalized === "·") {
      if (characters.length > 0 && characters.at(-1) !== "\n") {
        characters.push("\n");
        sourceIndexes.push(sourceIndex);
      }
      return;
    }
    if (/\s/.test(normalized)) {
      if (characters.length === 0 || characters.at(-1) === " " || characters.at(-1) === "\n") {
        return;
      }
      characters.push(" ");
      sourceIndexes.push(sourceIndex);
      return;
    }
    characters.push(normalized.toLowerCase());
    sourceIndexes.push(sourceIndex);
  }

  for (let index = 0; index < source.length;) {
    if (source[index] === "<") {
      if (source.startsWith("<!--", index)) {
        const close = source.indexOf("-->", index + 4);
        if (close !== -1) {
          for (let inner = index + 4; inner < close; inner += 1) {
            append(source[inner], inner);
          }
          index = close + 3;
          continue;
        }
      }
      const close = source.indexOf(">", index + 1);
      if (close !== -1) {
        const rawTag = source.slice(index, close + 1);
        for (const attribute of rawTag.matchAll(
          /\bdata-[a-z0-9_:-]+\s*=\s*(["'])([\s\S]*?)\1/gi,
        )) {
          const valueStart = index + (attribute.index ?? 0)
            + attribute[0].indexOf(attribute[2]);
          append("\n", valueStart);
          for (let offset = 0; offset < attribute[2].length;) {
            if (attribute[2][offset] === "&") {
              const entity = attribute[2].slice(offset)
                .match(/^&(?:#[0-9]+|#x[0-9a-f]+|[a-z][a-z0-9]+);/i);
              if (entity) {
                for (const character of decodeClaimEntity(entity[0])) {
                  append(character, valueStart + offset);
                }
                offset += entity[0].length;
                continue;
              }
            }
            append(attribute[2][offset], valueStart + offset);
            offset += 1;
          }
          append("\n", valueStart + attribute[2].length);
        }
        const tag = source.slice(index, close + 1).match(/^<\s*\/?\s*([a-z][a-z0-9]*)\b/i);
        if (tag && blockTags.test(tag[1])) {
          append("\n", index);
        }
        index = close + 1;
        continue;
      }
    }
    if (source[index] === "&") {
      const entity = source.slice(index).match(/^&(?:#[0-9]+|#x[0-9a-f]+|[a-z][a-z0-9]+);/i);
      if (entity) {
        for (const character of decodeClaimEntity(entity[0])) {
          append(character, index);
        }
        index += entity[0].length;
        continue;
      }
    }
    append(source[index], index);
    index += 1;
  }

  return {
    text: characters.join(""),
    sourceIndexes,
  };
}

function splitMarkdownRow(line) {
  const trimmed = line.trim();
  if (!trimmed.startsWith("|") || !trimmed.endsWith("|")) {
    return [];
  }

  const cells = [];
  let cell = "";
  let escaped = false;

  for (let i = 1; i < trimmed.length - 1; i += 1) {
    const char = trimmed[i];

    if (escaped) {
      cell += char;
      escaped = false;
      continue;
    }

    if (char === "\\") {
      escaped = true;
      cell += char;
      continue;
    }

    if (char === "|") {
      cells.push(cell.trim());
      cell = "";
      continue;
    }

    cell += char;
  }

  cells.push(cell.trim());
  return cells;
}

function splitQuotedAlternatives(phrase) {
  const parts = phrase
    .split("/")
    .map((part) => part.trim())
    .filter(Boolean);

  if (parts.length <= 1) {
    return [phrase.trim()];
  }

  const first = parts[0];
  const prefixEnd = first.lastIndexOf(" ");
  const sharedPrefix = prefixEnd === -1 ? "" : first.slice(0, prefixEnd + 1);

  return parts.map((part, index) => {
    if (index === 0 || part.includes(" ") || sharedPrefix === "") {
      return part;
    }

    return `${sharedPrefix}${part}`;
  });
}

function parseBannedPhrases(markdown) {
  const startMatch = markdown.match(/^## D · NOT ELIGIBLE\b.*$/m);
  if (!startMatch || startMatch.index === undefined) {
    return [];
  }

  const sectionStart = startMatch.index + startMatch[0].length;
  const rest = markdown.slice(sectionStart);
  const endMatch = rest.match(/^##\s+/m);
  const section = endMatch ? rest.slice(0, endMatch.index) : rest;
  const phrases = new Map();

  for (const line of section.split(/\r?\n/)) {
    const cells = splitMarkdownRow(line);
    if (cells.length < 2) {
      continue;
    }

    const firstCell = cells[0].trim();
    if (
      /^forbidden phrase$/i.test(firstCell) ||
      /^:?-{3,}:?$/.test(firstCell)
    ) {
      continue;
    }

    const matches = firstCell.matchAll(/"([^"]+)"/g);
    for (const match of matches) {
      for (const alternative of splitQuotedAlternatives(match[1])) {
        const normalized = normalizedClaimTextWithSourceMap(alternative.trim()).text;
        if (normalized) {
          phrases.set(normalized, alternative.trim());
        }
      }
    }
  }

  return [...phrases.entries()].map(([normalized, display]) => ({
    normalized,
    display,
  }));
}

async function listTypeScriptFiles(root) {
  const files = [];

  async function walk(dir) {
    const entries = await fs.readdir(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        await walk(fullPath);
        continue;
      }

      if (
        entry.isFile() &&
        entry.name.endsWith(".ts") &&
        !entry.name.endsWith(".test.ts") &&
        !entry.name.endsWith(".d.ts")
      ) {
        files.push(fullPath);
      }
    }
  }

  await walk(root);
  files.sort();
  return files;
}

async function listRustFiles(root) {
  const files = [];

  async function walk(dir) {
    const entries = await fs.readdir(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        await walk(fullPath);
        continue;
      }

      if (entry.isFile() && entry.name.endsWith(".rs")) {
        files.push(fullPath);
      }
    }
  }

  await walk(root);
  files.sort();
  return files;
}

function lineStartsFor(source) {
  const starts = [0];
  for (let i = 0; i < source.length; i += 1) {
    if (source[i] === "\n") {
      starts.push(i + 1);
    }
  }
  return starts;
}

function lineNumberAt(lineStarts, index) {
  let low = 0;
  let high = lineStarts.length - 1;

  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    if (lineStarts[mid] <= index) {
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }

  return high + 1;
}

function skipQuotedString(source, start, quote) {
  let i = start + 1;
  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      i += 2;
      continue;
    }

    if (char === quote) {
      return i + 1;
    }

    i += 1;
  }

  return source.length;
}

function skipTemplate(source, start) {
  let i = start + 1;
  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      i += 2;
      continue;
    }

    if (char === "`") {
      return i + 1;
    }

    if (char === "$" && source[i + 1] === "{") {
      i = skipTemplateExpression(source, i + 2);
      continue;
    }

    i += 1;
  }

  return source.length;
}

function skipTemplateExpression(source, start) {
  let depth = 1;
  let i = start;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      const newline = source.indexOf("\n", i + 2);
      i = newline === -1 ? source.length : newline + 1;
      continue;
    }

    if (char === "/" && next === "*") {
      const close = source.indexOf("*/", i + 2);
      i = close === -1 ? source.length : close + 2;
      continue;
    }

    if (char === "'" || char === '"') {
      i = skipQuotedString(source, i, char);
      continue;
    }

    if (char === "`") {
      i = skipTemplate(source, i);
      continue;
    }

    if (char === "{") {
      depth += 1;
      i += 1;
      continue;
    }

    if (char === "}") {
      depth -= 1;
      i += 1;
      if (depth === 0) {
        return i;
      }
      continue;
    }

    i += 1;
  }

  return source.length;
}

function parseQuotedLiteral(source, start, quote) {
  let text = "";
  let i = start + 1;

  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      if (i + 1 < source.length) {
        text += source[i + 1];
      }
      i += 2;
      continue;
    }

    if (char === quote) {
      return { text, end: i + 1 };
    }

    text += char;
    i += 1;
  }

  return { text, end: source.length };
}

function parseTemplateLiteral(source, start) {
  let text = "";
  let i = start + 1;

  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      if (i + 1 < source.length) {
        text += source[i + 1];
      }
      i += 2;
      continue;
    }

    if (char === "`") {
      return { text, end: i + 1 };
    }

    if (char === "$" && source[i + 1] === "{") {
      text += " ";
      i = skipTemplateExpression(source, i + 2);
      continue;
    }

    text += char;
    i += 1;
  }

  return { text, end: source.length };
}

function isImportExportSpecifier(source, literalStart) {
  const before = source.slice(Math.max(0, literalStart - 1000), literalStart);
  const lastSemicolon = before.lastIndexOf(";");
  const statement = before.slice(lastSemicolon + 1);

  if (/^\s*import(?:\s|$)[\s\S]*(?:\bfrom\s*)?$/.test(statement)) {
    return true;
  }

  if (/\bimport\s*\($/.test(statement)) {
    return true;
  }

  if (/^\s*export\b[\s\S]*\bfrom\s*$/.test(statement)) {
    return true;
  }

  return false;
}

function extractTypeScriptStrings(source) {
  const strings = [];
  const lineStarts = lineStartsFor(source);
  let i = 0;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      const newline = source.indexOf("\n", i + 2);
      i = newline === -1 ? source.length : newline + 1;
      continue;
    }

    if (char === "/" && next === "*") {
      const close = source.indexOf("*/", i + 2);
      i = close === -1 ? source.length : close + 2;
      continue;
    }

    if (char === "'" || char === '"') {
      const literal = parseQuotedLiteral(source, i, char);
      if (!isImportExportSpecifier(source, i)) {
        strings.push({
          text: literal.text,
          line: lineNumberAt(lineStarts, i),
        });
      }
      i = literal.end;
      continue;
    }

    if (char === "`") {
      const literal = parseTemplateLiteral(source, i);
      if (!isImportExportSpecifier(source, i)) {
        strings.push({
          text: literal.text,
          line: lineNumberAt(lineStarts, i),
        });
      }
      i = literal.end;
      continue;
    }

    i += 1;
  }

  return strings;
}

function parseRustQuotedLiteral(source, start) {
  let text = "";
  let i = start + 1;

  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      if (i + 1 < source.length) {
        text += source[i + 1];
      }
      i += 2;
      continue;
    }

    if (char === '"') {
      return { text, end: i + 1 };
    }

    text += char;
    i += 1;
  }

  return { text, end: source.length };
}

function parseRustRawLiteral(source, start) {
  if (source[start] !== "r") {
    return null;
  }

  let i = start + 1;
  while (source[i] === "#") {
    i += 1;
  }

  if (source[i] !== '"') {
    return null;
  }

  const hashes = i - start - 1;
  const close = `"${"#".repeat(hashes)}`;
  const textStart = i + 1;
  const textEnd = source.indexOf(close, textStart);

  if (textEnd === -1) {
    return { text: source.slice(textStart), end: source.length };
  }

  return {
    text: source.slice(textStart, textEnd),
    end: textEnd + close.length,
  };
}

function skipRustCharLiteral(source, start) {
  let i = start + 1;
  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      i += 2;
      continue;
    }

    if (char === "'") {
      return i + 1;
    }

    if (char === "\n") {
      return i;
    }

    i += 1;
  }

  return source.length;
}

function skipRustStringLike(source, start) {
  const raw = parseRustRawLiteral(source, start);
  if (raw) {
    return raw.end;
  }

  if (source[start] === '"') {
    return parseRustQuotedLiteral(source, start).end;
  }

  if (source[start] === "'") {
    return skipRustCharLiteral(source, start);
  }

  return start + 1;
}

function skipRustLineComment(source, start) {
  const newline = source.indexOf("\n", start + 2);
  return newline === -1 ? source.length : newline + 1;
}

function skipRustBlockComment(source, start) {
  let depth = 1;
  let i = start + 2;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "*") {
      depth += 1;
      i += 2;
      continue;
    }

    if (char === "*" && next === "/") {
      depth -= 1;
      i += 2;
      if (depth === 0) {
        return i;
      }
      continue;
    }

    i += 1;
  }

  return source.length;
}

function findMatchingRustBrace(source, openIndex) {
  let depth = 1;
  let i = openIndex + 1;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      i = skipRustLineComment(source, i);
      continue;
    }

    if (char === "/" && next === "*") {
      i = skipRustBlockComment(source, i);
      continue;
    }

    if (char === "r" || char === '"' || char === "'") {
      i = skipRustStringLike(source, i);
      continue;
    }

    if (char === "{") {
      depth += 1;
      i += 1;
      continue;
    }

    if (char === "}") {
      depth -= 1;
      i += 1;
      if (depth === 0) {
        return i;
      }
      continue;
    }

    i += 1;
  }

  return source.length;
}

function findRustItemEnd(source, start) {
  let i = start;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      i = skipRustLineComment(source, i);
      continue;
    }

    if (char === "/" && next === "*") {
      i = skipRustBlockComment(source, i);
      continue;
    }

    if (char === "r" || char === '"' || char === "'") {
      i = skipRustStringLike(source, i);
      continue;
    }

    if (char === "{") {
      return findMatchingRustBrace(source, i);
    }

    if (char === ";") {
      return i + 1;
    }

    i += 1;
  }

  return source.length;
}

function rustTestRanges(source) {
  const ranges = [];
  const cfgTestRe = /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]/g;
  const modTestsRe = /\bmod\s+tests\s*\{/g;
  let match;

  while ((match = cfgTestRe.exec(source)) !== null) {
    ranges.push({
      start: match.index,
      end: findRustItemEnd(source, match.index + match[0].length),
    });
  }

  while ((match = modTestsRe.exec(source)) !== null) {
    const openIndex = source.indexOf("{", match.index);
    ranges.push({
      start: match.index,
      end: findMatchingRustBrace(source, openIndex),
    });
  }

  ranges.sort((a, b) => a.start - b.start || a.end - b.end);
  return ranges;
}

function isInRange(index, range) {
  return range && index >= range.start && index < range.end;
}

function isIdentifierShapedRustLiteral(text) {
  const trimmed = text.trim();
  if (/\s/.test(trimmed)) {
    return false;
  }

  return (
    trimmed.includes("::") ||
    trimmed.includes("/") ||
    trimmed.includes("_") ||
    /^[a-z]+$/.test(trimmed)
  );
}

const RUST_USER_VISIBLE_FIELDS = new Set([
  "label",
  "title",
  "detail",
  "message",
  "warning",
  "warnings",
  "display_name",
  "summary",
  "description",
  "body",
  "heading",
  "subtitle",
]);
const RUST_USER_VISIBLE_FIELD_RE = new RegExp(
  `(?:^|[,{]\\s*)(${[...RUST_USER_VISIBLE_FIELDS].join("|")})\\s*:\\s*(?:(?:[A-Za-z_][A-Za-z0-9_:]*!?\\s*)?[\\(\\[\\{]\\s*)*$`,
);
const RUST_USER_VISIBLE_METHOD_RE =
  /\.(?:title|set_title|add_filter|set_message|set_detail)\s*\(\s*$/;

function rustLiteralIsStructFieldValue(source, start) {
  const before = source.slice(Math.max(0, start - 300), start);
  const match = before.match(RUST_USER_VISIBLE_FIELD_RE);
  return Boolean(match && RUST_USER_VISIBLE_FIELDS.has(match[1]));
}

function rustLiteralIsMethodArgument(source, start) {
  const before = source.slice(Math.max(0, start - 160), start);
  return RUST_USER_VISIBLE_METHOD_RE.test(before);
}

function rustLiteralIsErrValue(source, start, end) {
  const before = source.slice(Math.max(0, start - 160), start);
  const after = source.slice(end, Math.min(source.length, end + 160));

  if (/(?:^|[^\w])Err\s*\(\s*$/.test(before)) {
    return /^\s*\.\s*(?:into|to_string)\s*\(\s*\)\s*\)/.test(after);
  }

  if (/(?:^|[^\w])Err\s*\(\s*format!\s*\(\s*$/.test(before)) {
    return /^\s*(?:,|\)\s*\))/.test(after);
  }

  return false;
}

function rustLiteralIsBareReturnedValue(source, start, end) {
  const before = source.slice(Math.max(0, start - 160), start);
  const after = source.slice(end, Math.min(source.length, end + 80));

  return (
    /(?:\breturn\s+|=>\s*)$/.test(before) &&
    /^\s*\.\s*(?:into|to_string)\s*\(\s*\)/.test(after)
  );
}

function shouldSelectRustLiteral(source, start, end, text) {
  if (isIdentifierShapedRustLiteral(text)) {
    return false;
  }

  return (
    rustLiteralIsStructFieldValue(source, start) ||
    rustLiteralIsMethodArgument(source, start) ||
    rustLiteralIsErrValue(source, start, end) ||
    rustLiteralIsBareReturnedValue(source, start, end)
  );
}

function extractRustStrings(source) {
  const strings = [];
  const lineStarts = lineStartsFor(source);
  const testRanges = rustTestRanges(source);
  let testRangeIndex = 0;
  let i = 0;

  while (i < source.length) {
    while (testRangeIndex < testRanges.length && i >= testRanges[testRangeIndex].end) {
      testRangeIndex += 1;
    }

    if (isInRange(i, testRanges[testRangeIndex])) {
      i = testRanges[testRangeIndex].end;
      continue;
    }

    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      i = skipRustLineComment(source, i);
      continue;
    }

    if (char === "/" && next === "*") {
      i = skipRustBlockComment(source, i);
      continue;
    }

    const raw = parseRustRawLiteral(source, i);
    if (raw) {
      if (shouldSelectRustLiteral(source, i, raw.end, raw.text)) {
        strings.push({
          text: raw.text,
          line: lineNumberAt(lineStarts, i),
        });
      }
      i = raw.end;
      continue;
    }

    if (char === '"') {
      const literal = parseRustQuotedLiteral(source, i);
      if (shouldSelectRustLiteral(source, i, literal.end, literal.text)) {
        strings.push({
          text: literal.text,
          line: lineNumberAt(lineStarts, i),
        });
      }
      i = literal.end;
      continue;
    }

    if (char === "'") {
      i = skipRustCharLiteral(source, i);
      continue;
    }

    i += 1;
  }

  return strings;
}

function excerptAround(text, index, length) {
  const start = Math.max(0, index - 50);
  const end = Math.min(text.length, index + length + 50);
  return text
    .slice(start, end)
    .replace(/\s+/g, " ")
    .trim();
}

function sentenceBounds(text, start, end) {
  const before = text.slice(0, start);
  const leftBoundary = Math.max(
    before.lastIndexOf("."),
    before.lastIndexOf("!"),
    before.lastIndexOf("?"),
    before.lastIndexOf(";"),
    before.lastIndexOf("\n"),
  );
  const boundaryIndexes = [".", "!", "?", ";", "\n"]
    .map((boundary) => text.indexOf(boundary, end))
    .filter((index) => index !== -1);
  const rightBoundary =
    boundaryIndexes.length === 0 ? text.length : Math.min(...boundaryIndexes);
  return {
    start: leftBoundary + 1,
    end: rightBoundary,
  };
}

function genericNegationGovernsClaim(text, start, end) {
  const bounds = sentenceBounds(text, start, end);
  const before = text.slice(bounds.start, start);
  return (
    /\b(?:is|are|was|were|does|do|did|has|have|had|can|could|will|would)\s+not\s+(?:yet\s+)?(?:an?\s+)?$/i.test(before)
    || /\bnever\s+(?:an?\s+)?$/i.test(before)
    || /\bnot\s+(?:an?\s+)?(?:claim|promise|assertion)\s+(?:of|that)\s*$/i.test(before)
  );
}

function attachmentLimitationGovernsClaim(text, start, end) {
  const bounds = sentenceBounds(text, start, end);
  const before = text.slice(bounds.start, start);
  const after = text.slice(end, bounds.end);
  const limitation =
    "(?:planned|unavailable|unproved|unproven|unknown|not\\s+yet\\s+(?:available|implemented)|not\\s+established)";
  const beforePattern = new RegExp(
    `(?:\\b${limitation}\\b|\\bno\\b[^.!?;]{0,80}\\b(?:proves?|establishes?|shows?|demonstrates?|verifies?)\\s+)`
      + "\\s*"
      + "(?:(?:claim|property|behaviou?r|assertion)\\s+)?"
      + "(?:(?:that|whether|if)\\s+)?"
      + "(?:(?:the|this|it|osl|release|build|feature|transport)\\s+){0,6}$",
    "i",
  );
  const afterPattern = new RegExp(
    "^\\s*(?:,?\\s*(?:(?:a|the|this)\\s+)?"
      + "(?:(?:claim|property|behaviou?r|assertion)\\s+)?"
      + "(?:(?:that|which)\\s+)?)?"
      + `(?:is|are|remains?|stays?|has\\s+not\\s+been|have\\s+not\\s+been)\\s+${limitation}\\b`,
    "i",
  );
  return beforePattern.test(before) || afterPattern.test(after);
}

function semanticAttachmentClaimSpans(text) {
  const spans = [];
  const sentencePattern = /[^.!?;\n]+[.!?;]?/g;

  function token(sentence, pattern) {
    const match = sentence.match(pattern);
    return match && match.index !== undefined
      ? { start: match.index, end: match.index + match[0].length }
      : null;
  }

  function tokenAfter(sentence, pattern, after) {
    if (!after) {
      return null;
    }
    const match = sentence.slice(after.end).match(pattern);
    return match && match.index !== undefined
      ? {
          start: after.end + match.index,
          end: after.end + match.index + match[0].length,
        }
      : null;
  }

  function record(sentenceStart, tokens) {
    const present = tokens.filter(Boolean);
    if (present.length !== tokens.length) {
      return;
    }
    const start = sentenceStart + Math.min(...present.map((item) => item.start));
    const end = sentenceStart + Math.max(...present.map((item) => item.end));
    const key = `${start}:${end}`;
    if (!spans.some((span) => span.key === key)) {
      spans.push({ key, start, end });
    }
  }

  function recordPattern(pattern) {
    for (const match of text.matchAll(pattern)) {
      const start = match.index ?? 0;
      const end = start + match[0].length;
      const key = `${start}:${end}`;
      if (!spans.some((span) => span.key === key)) {
        spans.push({ key, start, end });
      }
    }
  }

  for (const sentenceMatch of text.matchAll(sentencePattern)) {
    const sentence = sentenceMatch[0];
    const sentenceStart = sentenceMatch.index ?? 0;
    const discord = token(sentence, /\bdiscord\b/i);
    const attachment = token(sentence, /\b(?:attachments?|uploaded\s+files?|files?)\b/i);
    const inspection = token(sentence, /\b(?:scann?(?:er|ers|ing|ed|s)?|inspection)\b/i);
    const defeated = token(
      sentence,
      /\b(?:defeat(?:ed|s|ing)?|solv(?:e|ed|es|ing)|bypass(?:ed|es|ing)?|neutraliz(?:e|ed|es|ing)|block(?:ed|s|ing)?|evad(?:e|ed|es|ing)|opaque)\b/i,
    );
    record(sentenceStart, [discord, attachment, inspection, defeated]);

    const receives = token(sentence, /\b(?:receiv(?:e|es|ed|ing)|gets?|sees?)\b/i);
    const cover = token(sentence, /\b(?:harmless\s+)?cover\s+files?\b/i);
    const instead = token(sentence, /\b(?:instead\s+of|rather\s+than)\b/i);
    const replacedAttachment = tokenAfter(
      sentence,
      /\b(?:attachments?|uploaded\s+files?|files?)\b/i,
      instead,
    );
    record(sentenceStart, [discord, receives, cover, instead, replacedAttachment]);

    const decoy = token(sentence, /\bdecoys?\b/i);
    const exclusive = token(sentence, /\b(?:only|instead\s+of|rather\s+than)\b/i);
    record(sentenceStart, [discord, decoy, exclusive]);
  }

  // Relational paraphrases found by the independent successor audit. These
  // patterns bind the actor, downstream surface, and claimed outcome; they do
  // not ban isolated words such as "placeholder", "blocks", or "unreadable".
  for (const pattern of [
    /\bosl\s+(?:thwart(?:s|ed|ing)?|circumvent(?:s|ed|ing)?)\s+discord(?:'s)?[^.!?;\n]{0,80}\b(?:attachment|upload(?:ed)?\s+files?)[^.!?;\n]{0,45}\b(?:scann?(?:er|ers|ing)?|inspection|checks?)\b/gi,
    /\bosl\s+prevent(?:s|ed|ing)?\s+discord\s+from\s+inspect(?:s|ed|ing)?\s+(?:attachments?|uploaded\s+files?|files?|uploads?)\b/gi,
    /\bdiscord(?:'s)?\s+checks?\s+on\s+attachments?\s+(?:are|is|were|was|have\s+been|has\s+been)\s+rendered\s+ineffective\s+by\s+osl\b/gi,
    /\battachment\s+inspection\s+by\s+discord\s+no\s+longer\s+works(?:\s+when\s+osl\s+is\s+used)?\b/gi,
    /\bdiscord\s+(?:gets?|receives?|sees?)\s+(?:an?\s+|the\s+)?(?:(?:harmless|benign|safe)\s+)?(?:placeholder|stand-in|dummy|surrogate|replacement|decoy)(?:\s+(?:file|image|blob|media))?\s+(?:instead\s+of|rather\s+than)\s+(?:the\s+)?(?:(?:real|original|actual|user's)\s+)?(?:attachments?|uploads?|uploaded\s+files?|files?)\b/gi,
    /\bonly\s+(?:an?\s+)?(?:(?:harmless|benign|safe)\s+)?(?:placeholder|stand-in|dummy|surrogate|replacement|decoy)(?:\s+(?:file|image|blob|media))?\s+reaches\s+discord(?:\s*(?:;|,)\s*(?:the\s+)?(?:(?:real|original|actual|user's)\s+)?(?:attachments?|uploads?|files?)\s+(?:does\s+not|stays?\s+off)|\s+(?:instead\s+of|rather\s+than)\s+(?:the\s+)?(?:(?:real|original|actual|user's)\s+)?(?:attachments?|uploads?|files?))\b/gi,
    /\bosl\s+substitut(?:e|es|ed|ing)\s+(?:an?\s+|the\s+)?(?:(?:harmless|benign|safe)\s+)?(?:placeholder|stand-in|dummy|surrogate|replacement|decoy)(?:\s+(?:file|image|blob|media))?\s+for\s+(?:every\s+|the\s+|an?\s+)?(?:attachments?|uploads?|files?)\s+(?:sent|uploaded)\s+to\s+discord\b/gi,
    /\b(?:actual|original|real)\s+(?:attachments?|uploads?|files?)\s+(?:stays?|remains?)\s+off\s+discord\s*;\s*(?:an?\s+)?(?:(?:harmless|benign|safe)\s+)?(?:placeholder|stand-in|dummy|surrogate|replacement|decoy)(?:\s+(?:file|image|blob|media))?\s+is\s+(?:uploaded|sent)\s+in\s+its\s+place\b/gi,
    /\bdiscord\s+sees\s+nothing\s+except\s+(?:decoy|fake|dummy|placeholder|surrogate)(?:\s+(?:files?|images?|media|blobs?))?\b/gi,
    /\bevery\s+(?:file|attachment|upload)\s+visible\s+to\s+discord\s+is\s+(?:an?\s+)?(?:decoy|fake|dummy|placeholder|surrogate)(?:\s+(?:file|image|blob))?\b/gi,
    /\bdiscord\s+can\s+inspect\s+only\s+(?:an?\s+)?(?:decoy|fake|dummy|placeholder|surrogate)(?:\s+(?:file|image|blob|upload))?\s*,?\s*not\s+(?:the\s+)?(?:user's|real|original|actual)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\b(?:the\s+)?(?:user's|real|original|actual)\s+(?:attachments?|uploads?|files?)\s+(?:is|are|being|remains?|stays?)\s+(?:opaque|unreadable)\s+to\s+discord\b/gi,
    /\bdiscord(?:'s)?\s+(?:attachment\s+)?scann?(?:er|ers)\s+learns?\s+nothing\s+about\s+(?:the\s+)?(?:user's|real|original|actual)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\bosl\s+(?:sidestep(?:s|ped|ping)?|dodg(?:e|es|ed|ing)|outflank(?:s|ed|ing)?|nullif(?:y|ies|ied|ying)|slip(?:s|ped|ping)?\s+(?:attachments?|uploads?)\s+past|route(?:s|d|ing)?\s+(?:attachments?|uploads?)\s+around|tunnel(?:s|ed|ing)?\s+(?:attachments?|uploads?)\s+(?:past|beyond))\s+discord(?:'s)?[^.!?;\n]{0,90}\b(?:inspection|review|scrutiny|screening|file[- ]analysis(?:\s+pass)?)\b/gi,
    /\bosl\s+(?:sidestep(?:s|ped|ping)?|dodg(?:e|es|ed|ing)|outflank(?:s|ed|ing)?|nullif(?:y|ies|ied|ying))\s+discord(?:'s)?\s+(?:inspection|review|scrutiny|screening|file[- ]analysis(?:\s+pass)?)\s+of\s+(?:uploaded\s+)?(?:attachments?|uploads?|files?)\b/gi,
    /\bosl\s+sidesteps\s+discord(?:'s)?\s+inspection\s+of\s+uploaded\s+attachments\b/gi,
    /\bosl\b[^.!?]{0,90}[.!?]\s*it\s+(?:sidestep(?:s|ped|ping)?|dodg(?:e|es|ed|ing)|outflank(?:s|ed|ing)?|nullif(?:y|ies|ied|ying))\s+discord(?:'s)?\s+(?:inspection|review|scrutiny|screening|file[- ]analysis(?:\s+pass)?)\s+of\s+(?:uploaded\s+)?(?:attachments?|uploads?|files?)\b/gi,
    /\bdiscord(?:'s)?\s+(?:attachment|upload|file)?\s*(?:inspection|review|scrutiny|screening|file[- ]analysis(?:\s+pass)?)\s+(?:is|was|has\s+been)\s+(?:outflanked|nullified|sidestepped|dodged|made\s+(?:useless|ineffective))\s+by\s+osl\b/gi,
    /\bdiscord\s+(?:examines?|reviews?|screens?|inspects?)\s+(?:attachments?|uploads?|files?)[.!?]\s*(?:with\s+osl[^.!?]{0,35},?\s*)?(?:that|the)\s+(?:inspection|review|screening)\s+cannot\s+reach\s+(?:the\s+)?(?:actual|real|source|user's)?\s*(?:attachments?|uploads?|files?)\b/gi,
    /\bdiscord(?:'s)?\s+(?:attachment|upload)?\s*(?:inspection|review|screening)\s+(?:is|was)\s+made\s+(?:useless|ineffective)\s+(?:whenever|when)\s+osl\s+(?:sends?|is\s+used)\b/gi,
    /\bdiscord\s+(?:is\s+handed|gets?|receives?|is\s+shown)\s+(?:an?\s+|the\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)(?:\s+(?:file|upload|blob|media))?\b[^.!?;\n]{0,100}\b(?:genuine|actual|real|original|source|user's)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\b(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:is|are)\s+(?:exchanged|swapped|substituted)\s+for\s+(?:an?\s+|the\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\b[^.!?;\n]{0,80}\bdiscord\b/gi,
    /\bin\s+(?:the\s+)?(?:attachments?|uploads?|files?)(?:'s)?\s+place\s*,?\s*discord\s+(?:gets?|receives?|is\s+handed)\s+(?:an?\s+|the\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\b/gi,
    /\b(?:the\s+)?(?:actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:stays?|remains?)\s+local[.!?]\s*discord\s+(?:gets?|receives?|is\s+handed)\s+(?:an?\s+)?(?:separate\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\b/gi,
    /\b(?:(?:an?\s+)?(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\s+(?:is\s+)?(?:sent|handed|given)\s+to\s+discord\s+(?:in\s+place\s+of|instead\s+of|rather\s+than)\s+(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\bosl\s+(?:swaps?|exchanges?|substitutes?)\s+(?:an?\s+|the\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\s+for\s+(?:each|every|the|an?)\s+(?:user's|genuine|actual|real|original|source)?\s*(?:attachments?|uploads?|files?)[^.!?;\n]{0,55}\bdiscord\b/gi,
    /\bdiscord\s+(?:is\s+blind\s+to|gains?\s+(?:no|zero)\s+information\s+from|cannot\s+(?:discern|decipher|understand|read))\s+(?:the\s+)?(?:contents?\s+of\s+)?(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\b(?:nothing|zero\s+information)\s+(?:about\s+)?(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:is\s+)?(?:intelligible|visible|available|revealed)\s+to\s+discord\b/gi,
    /\b(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:is|are|becomes?|remain(?:s)?)\s+(?:indecipherable|invisible|unintelligible)\s+to\s+discord\b/gi,
    /\b(?:the\s+)?(?:referenced\s+)?(?:attachments?|uploads?|files?)\s+reveals?\s+(?:no|zero)\s+(?:content|information|meaning)\s+to\s+discord\b/gi,
    /\bdiscord\s+receives?\s+(?:an?\s+|the\s+)?(?:upload|attachment|file)\s+reference[.!?]\s*(?:the\s+)?referenced\s+(?:attachments?|uploads?|files?)\s+reveals?\s+(?:no|zero)\s+(?:content|information|meaning)\s+to\s+it\b/gi,
    /\bto\s+discord\s*,?\s*(?:every\s+|the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:is|are|becomes?|remain(?:s)?)\s+(?:indecipherable|invisible|unintelligible)\b/gi,
    /\bdiscord\s+can\s+extract\s+(?:no|zero)\s+(?:content|information|meaning)\s+from\s+(?:the\s+)?(?:payload|attachment|upload|file)\b/gi,
    /\bosl\s+(?:skirts?|skirted|skirting)\s+discord(?:'s)?\s+(?:attachment\s+|file\s+|upload\s+)?audit\b/gi,
    /\b(?:osl\s+)?renders?\s+discord(?:'s)?\s+(?:attachment\s+|file\s+|upload\s+)?audit\s+toothless\b/gi,
    /\bdiscord\s+(?:gets?|receives?|sees?)\s+(?:an?\s+|the\s+)?(?:harmless|benign|safe|sanitized)?\s*(?:double|lookalike)\b/gi,
    /\b(?:an?\s+|the\s+)?(?:harmless|benign|safe|sanitized)\s+(?:double|lookalike)\s+(?:in\s+lieu\s+of|instead\s+of|rather\s+than)\s+(?:the\s+)?(?:genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\bdiscord\s+cannot\s+make\s+sense\s+of\s+(?:the\s+)?(?:genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\bdiscord\s+sees\s+only\s+gibberish\b/gi,
    /\b(?:discord|the\s+(?:service|platform)|the\s+downstream\s+(?:service|platform))(?:'s)?\s+(?:content[-\s]+analysis\s+(?:machinery|system|pipeline)|analysis\s+(?:machinery|system|pipeline))\s+(?:gets?|receives?|has)\s+no\s+(?:useful|meaningful)\s+(?:view|visibility|information)\b/gi,
    /\b(?:discord|the\s+(?:service|platform))\s+(?:gets?|receives?|sees?)\s+(?:an?\s+)?(?:benign|harmless|sanitized)\s+(?:twin|double|lookalike)\b[^\n]{0,120}\b(?:source|original|real|actual)\s+(?:attachments?|uploads?|files?)\s+never\s+(?:leaves?|reaches?|arrives?)\b/gi,
    /\b(?:the\s+)?(?:attachments?|uploads?|files?)(?:'s)?\s+(?:substance|contents?|meaning)\s+(?:is|are|remains?|becomes?)\s+(?:unintelligible|indecipherable|invisible)\s+to\s+(?:discord|the\s+(?:service|platform))\b/gi,
    /\b(?:osl\s+)?skirts?\s+discord(?:'s)?\s+(?:attachment\s+|file\s+|upload\s+)?audit\b/gi,
  ]) {
    recordPattern(pattern);
  }

  return spans.map(({ start, end }) => ({ start, end }));
}

function semanticAtRestClaimSpans(text) {
  const spans = [];
  const universalScope =
    /(?<!\bat\s)\ball\b|\b(?:each|every|everything|entire|entirety|whole|complete|totality|no|none|nothing|never|zero)\b|100\s*%/i;
  const stateObject =
    /\b(?:state|data|information|records?|storage|metadata|preferences?|settings?|profile|history|files?|content|cache|database|items?|things?|secrets?|artifacts?|residue|material)\b/i;
  const broadCategory =
    /\b(?:private|local|sensitive|confidential)\s+(?:conversation\s+)?(?:state|data|information|records?|storage|metadata|preferences?|settings?|profile|history|artifacts?|residue|material)\b/i;
  const protection =
    /\b(?:encrypt(?:s|ed|ing)?|decrypt(?:s|ed|ing)?|encipher(?:s|ed|ing)?|unencrypted|ciphertext|cleartext|plain[-\s]*text|sealed?|protect(?:s|ed|ing)?|secur(?:e|es|ed|ing)?|gated|guards?|locked|unlocks?|inaccessible|unreadable|opaque|passphrase|password|in\s+the\s+clear)\b/i;
  const localContext =
    /\bat[-\s]+rest\b|\bon[-\s]+disk\b|\bfilesystem\b|\blocal(?:ly)?\b|\bon[-\s]+device\b|\bon\s+(?:this|your|the)\s+(?:device|computer|machine)\b|\bwhole[-\s]+profile\b|\b(?:persist(?:s|ed|ing)?|retain(?:s|ed|ing)?|saved?|stored?)\b/i;
  const destructive =
    /\b(?:delete|deletes|deleted|deleting|remove|removes|removed|removing|uninstall|clear|clears|cleared|clearing)\b/i;
  const residueAbsence =
    /\b(?:(?:osl\s+)?leaves?\s+(?:behind\s+)?(?:no|zero)\s+(?:readable\s+)?(?:private|confidential|sensitive)?\s*(?:residue|artifacts?|data|material)|nothing\s+(?:private|confidential|sensitive)\s+survives?\s+(?:on[-\s]+disk|at[-\s]+rest|locally|on\s+(?:the|your|this)\s+(?:device|computer|machine)))\b/i;
  const attachedLimitation =
    /\b(?:planned|unavailable|unknown|unproved|unproven|not\s+yet\s+(?:available|implemented|proved)|not\s+established|does\s+not\s+(?:claim|cover|protect|encrypt|secure|mean|imply)(?:\s+that)?\s+(?:all|each|every|nothing)|not\s+(?:all|each|every|everything|the\s+(?:entire|whole|complete))|may\s+remain\s+plaintext|plaintext\s+(?:fallback|writes?)|without\s+(?:an?\s+)?(?:installed\s+)?storage\s+key|remov(?:e|es|ed|ing)\b.{0,80}\b(?:restores?|causes?)\s+plaintext\s+writes?)\b/i;

  for (const block of text.matchAll(/[^\n]+/g)) {
    const blockText = block[0];
    const sentences = [...blockText.matchAll(/[^.!?;\n]+[.!?;]?/g)]
      .map((match) => ({
        start: match.index ?? 0,
        text: match[0].trim(),
      }))
      .filter(({ text: sentence }) => sentence);
    const scope = sentences.find(({ text: sentence }) => {
      const broadProtectedState = protection.test(sentence)
        && localContext.test(sentence)
        && (broadCategory.test(sentence)
          || (universalScope.test(sentence) && stateObject.test(sentence)));
      return !attachedLimitation.test(sentence)
        && !destructive.test(sentence)
        && (residueAbsence.test(sentence) || broadProtectedState);
    });
    if (!scope) {
      continue;
    }
    spans.push({
      start: (block.index ?? 0) + scope.start,
      end: (block.index ?? 0) + scope.start + scope.text.length,
    });
  }
  return spans;
}

function semanticScrubClaimSpans(text) {
  const spans = [];
  const scrubContext = /\b(?:auto\s*scrub|scrub)\b/i;
  const attachedLimitation =
    /\b(?:planned|coming\s+soon|unavailable|not\s+(?:available|implemented|wired|supported|proved|proven|qualified)|not\s+yet\s+(?:available|implemented|wired|supported|proved|proven|qualified)|implemented[-\s]+unwired|test[-\s]+proven(?:[-\s]+only)?|unwired|unproved|unproven|unknown|view[-\s]+only|manual(?:ly|\s+only)?|requires?\s+(?:your\s+)?(?:review|confirmation)|does\s+not|cannot|never|may\s+(?:omit|exclude|miss)|can\s+be\s+incomplete|future|intended|design)\b/i;
  const completeHistory =
    /(?:\b(?:complete|full|entire|whole|all|fully\s+reconciled)\b.{0,45}\b(?:history|content|messages?|posts?|records?|account\s+data|exports?|downloads?)\b|\b(?:history|content|messages?|posts?|records?|account\s+data|exports?|downloads?)\b.{0,45}\b(?:complete|full|entire|whole|all|fully\s+reconciled|nothing\s+(?:is\s+)?omitted)\b|\bnothing\s+(?:is\s+)?omitted\b.{0,45}\b(?:scrub|exports?|downloads?|history|content)\b)/i;
  const awayOperation =
    /(?:\b(?:works?|runs?|scans?|cleans?|delet(?:e|es|ed|ing)|remov(?:e|es|ed|ing))\b.{0,55}\b(?:while\s+you.{0,8}\baway|while\s+the\s+user\s+is\s+away|while\s+away|unattended|in\s+the\s+background|without\s+(?:you|the\s+user))\b|\b(?:while\s+you.{0,8}\baway|while\s+the\s+user\s+is\s+away|while\s+away|unattended|in\s+the\s+background|without\s+(?:you|the\s+user))\b.{0,55}\b(?:works?|runs?|scans?|cleans?|delet(?:e|es|ed|ing)|remov(?:e|es|ed|ing))\b)/i;
  const automaticDeletion =
    /(?:\b(?:automatically|autonomously|on\s+its\s+own|without\s+(?:your\s+)?(?:review|confirmation|approval))\b.{0,45}\b(?:deletes?|removes?|cleans?|erases?)\b|\b(?:deletes?|removes?|cleans?|erases?)\b.{0,45}\b(?:automatically|autonomously|on\s+its\s+own|without\s+(?:your\s+)?(?:review|confirmation|approval))\b)/i;
  const providerSupport =
    /\b(?:supports?|works?\s+with|handles?|imports?\s+from|covers?|available\s+(?:for|across|on)|compatible\s+with|(?:natively\s+)?understands?)\b/i;
  const fiveProviderWording =
    /\b(?:five|5)[-\s]+(?:providers?|services?|platforms?|apps?|connectors?)\b/i;
  const providerPatterns = [
    /\bdiscord\b/i,
    /\b(?:meta|facebook|instagram)\b/i,
    /\bwhats\s*app\b/i,
    /\b(?:google|gmail)\b/i,
    /\b(?:twitter|x\/twitter|x)\b/i,
    /\b(?:microsoft|outlook)\b/i,
  ];

  for (const sentenceMatch of text.matchAll(/[^.!?;\n]+[.!?;]?/g)) {
    const sentence = sentenceMatch[0].trim();
    if (!sentence || !scrubContext.test(sentence) || attachedLimitation.test(sentence)) {
      continue;
    }
    const providerCount = providerPatterns.filter((pattern) => pattern.test(sentence)).length;
    if (
      completeHistory.test(sentence)
      || awayOperation.test(sentence)
      || automaticDeletion.test(sentence)
      || fiveProviderWording.test(sentence)
      || (providerSupport.test(sentence) && providerCount >= 5)
    ) {
      const start = sentenceMatch.index ?? 0;
      spans.push({ start, end: start + sentenceMatch[0].length });
    }
  }
  return spans;
}

function countNewlinesBefore(text, index) {
  let count = 0;
  for (let i = 0; i < index; i += 1) {
    if (text[i] === "\n") {
      count += 1;
    }
  }
  return count;
}

// Section D bans '"Audited" / "reviewed" / "independently verified"' as SECURITY
// claims. Two of those are also ordinary English: the first run of this gate
// flagged "Selected apps reviewed" and "Every batch is reviewed and confirmed",
// which are about the *user* reviewing and have nothing to do with an audit.
//
// A gate that cries wolf on honest UI copy gets switched off, so these terms
// only fire in a security context. Multi-word section D phrases stay absolute —
// "cryptographic burn" is never innocent.
const CONTEXT_GATED_TERMS = new Set(["audited", "reviewed", "independently verified"]);
const SECURITY_CONTEXT_RE =
  /\b(osl|security|securely|crypto|cryptograph\w*|encryption|encrypted|protocol|third[- ]party|externally|independent\w*|auditor\w*|penetration|pentest)\b/i;
const SECURITY_CONTEXT_WINDOW = 90;

function inSecurityContext(text, index, length) {
  const start = Math.max(0, index - SECURITY_CONTEXT_WINDOW);
  const end = Math.min(text.length, index + length + SECURITY_CONTEXT_WINDOW);
  const before = text.slice(start, index);
  const after = text.slice(index + length, end);
  return SECURITY_CONTEXT_RE.test(before) || SECURITY_CONTEXT_RE.test(after);
}

function analyseFragments(file, fragments, bannedPhrases) {
  const violations = [];

  for (const fragment of fragments) {
    const normalized = normalizedClaimTextWithSourceMap(fragment.text);
    const lower = normalized.text;
    const exactAttachmentRanges = [];

    for (const phrase of bannedPhrases) {
      const contextGated = CONTEXT_GATED_TERMS.has(phrase.normalized);
      let index = lower.indexOf(phrase.normalized);
      while (index !== -1) {
        const end = index + phrase.normalized.length;
        const attachmentClaim = REQUIRED_ATTACHMENT_BANS.includes(phrase.normalized);
        const gatedOut =
          contextGated && !inSecurityContext(lower, index, phrase.normalized.length);
        const limited =
          genericNegationGovernsClaim(lower, index, end)
          || (attachmentClaim && attachmentLimitationGovernsClaim(lower, index, end));
        if (attachmentClaim) {
          exactAttachmentRanges.push({ start: index, end });
        }
        if (!gatedOut && !limited) {
          const sourceIndex = normalized.sourceIndexes[index] ?? 0;
          violations.push({
            file,
            line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
            phrase: phrase.display,
            excerpt: excerptAround(fragment.text, sourceIndex, phrase.normalized.length),
          });
        }

        index = lower.indexOf(phrase.normalized, index + phrase.normalized.length);
      }
    }

    for (const span of semanticAttachmentClaimSpans(lower)) {
      const overlapsExact = exactAttachmentRanges.some(
        (exact) => span.start < exact.end && span.end > exact.start,
      );
      if (overlapsExact || attachmentLimitationGovernsClaim(lower, span.start, span.end)) {
        continue;
      }
      const sourceIndex = normalized.sourceIndexes[span.start] ?? 0;
      violations.push({
        file,
        line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
        phrase: "attachment-scanning overclaim",
        excerpt: excerptAround(
          fragment.text,
          sourceIndex,
          Math.max(1, span.end - span.start),
        ),
      });
    }

    for (const span of semanticAtRestClaimSpans(lower)) {
      const sourceIndex = normalized.sourceIndexes[span.start] ?? 0;
      violations.push({
        file,
        line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
        phrase: "at-rest/local-protection overclaim",
        excerpt: excerptAround(
          fragment.text,
          sourceIndex,
          Math.max(1, span.end - span.start),
        ),
      });
    }

    for (const span of semanticScrubClaimSpans(lower)) {
      const sourceIndex = normalized.sourceIndexes[span.start] ?? 0;
      violations.push({
        file,
        line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
        phrase: "Scrub capability overclaim",
        excerpt: excerptAround(
          fragment.text,
          sourceIndex,
          Math.max(1, span.end - span.start),
        ),
      });
    }
  }

  return violations;
}

function padCell(value, width) {
  return String(value).padEnd(width, " ");
}

function printSummary(rows, totals) {
  const headers = ["File", "Units", "Violations"];
  const widths = [
    Math.max(headers[0].length, ...rows.map((row) => row.file.length)),
    Math.max(headers[1].length, ...rows.map((row) => String(row.units).length)),
    Math.max(headers[2].length, ...rows.map((row) => String(row.violations).length)),
  ];

  console.log(
    `${padCell(headers[0], widths[0])}  ${padCell(headers[1], widths[1])}  ${padCell(headers[2], widths[2])}`,
  );
  console.log(`${"-".repeat(widths[0])}  ${"-".repeat(widths[1])}  ${"-".repeat(widths[2])}`);

  for (const row of rows) {
    console.log(
      `${padCell(row.file, widths[0])}  ${padCell(row.units, widths[1])}  ${padCell(row.violations, widths[2])}`,
    );
  }

  console.log(`${"-".repeat(widths[0])}  ${"-".repeat(widths[1])}  ${"-".repeat(widths[2])}`);
  console.log(
    `${padCell("TOTAL", widths[0])}  ${padCell(totals.units, widths[1])}  ${padCell(totals.violations, widths[2])}`,
  );
}

function printViolations(violations) {
  if (violations.length === 0) {
    return;
  }

  console.error("\nViolations:");
  for (const violation of violations) {
    console.error(
      `${violation.file}:${violation.line}: "${violation.phrase}" in "${violation.excerpt}"`,
    );
  }
}

function printFloorFailures(failures) {
  if (failures.length === 0) {
    return;
  }

  console.error("\nFloor failures:");
  for (const failure of failures) {
    console.error(
      `${failure.name}: expected at least ${failure.expected}, actual ${failure.actual}`,
    );
  }
}

async function loadBannedPhrases() {
  const allowlist = await readUtf8(ALLOWLIST_PATH);
  return parseBannedPhrases(allowlist);
}

function bannedPhraseInputFailures(bannedPhrases) {
  const failures = [];
  if (bannedPhrases.length < MIN_BANNED_PHRASES) {
    failures.push({
      name: "banned phrases parsed from section D",
      expected: MIN_BANNED_PHRASES,
      actual: bannedPhrases.length,
    });
  }
  const present = new Set(bannedPhrases.map((phrase) => phrase.normalized));
  const attachmentBanCount = REQUIRED_ATTACHMENT_BANS.filter((phrase) => present.has(phrase)).length;
  if (attachmentBanCount < REQUIRED_ATTACHMENT_BANS.length) {
    failures.push({
      name: "attachment bans parsed from section D",
      expected: REQUIRED_ATTACHMENT_BANS.length,
      actual: attachmentBanCount,
    });
  }
  return failures;
}

async function scanRepository() {
  const bannedPhrases = await loadBannedPhrases();
  const rows = [];
  const allViolations = [];
  const floorFailures = bannedPhraseInputFailures(bannedPhrases);
  let tsStringCount = 0;
  let rustStringCount = 0;
  let readmeBytes = 0;

  const tsFiles = await listTypeScriptFiles(APP_SRC_ROOT);
  for (const filePath of tsFiles) {
    const source = await readUtf8(filePath);
    const fragments = extractTypeScriptStrings(source);
    const file = repoRelative(filePath);
    const violations = analyseFragments(file, fragments, bannedPhrases);

    tsStringCount += fragments.length;
    allViolations.push(...violations);
    rows.push({
      file,
      units: fragments.length,
      violations: violations.length,
    });
  }

  const rustFiles = await listRustFiles(RUST_APP_SRC_ROOT);
  for (const filePath of rustFiles) {
    const source = await readUtf8(filePath);
    const fragments = extractRustStrings(source);
    const file = repoRelative(filePath);
    const violations = analyseFragments(file, fragments, bannedPhrases);

    rustStringCount += fragments.length;
    allViolations.push(...violations);
    rows.push({
      file,
      units: fragments.length,
      violations: violations.length,
    });
  }

  let readmeText = "";
  try {
    readmeText = await readUtf8(README_PATH);
    readmeBytes = Buffer.byteLength(readmeText);
  } catch (error) {
    if (error && error.code !== "ENOENT") {
      throw error;
    }
  }

  const readmeFragments = readmeText ? [{ text: readmeText, line: 1 }] : [];
  const readmeViolations = analyseFragments("README.md", readmeFragments, bannedPhrases);
  allViolations.push(...readmeViolations);
  rows.push({
    file: "README.md",
    units: readmeFragments.length,
    violations: readmeViolations.length,
  });

  if (tsStringCount < MIN_TS_STRING_LITERALS) {
    floorFailures.push({
      name: "TypeScript string literals extracted",
      expected: MIN_TS_STRING_LITERALS,
      actual: tsStringCount,
    });
  }

  if (rustStringCount < MIN_RUST_STRING_LITERALS) {
    floorFailures.push({
      name: "Rust user-visible string literals extracted",
      expected: MIN_RUST_STRING_LITERALS,
      actual: rustStringCount,
    });
  }

  if (readmeBytes < MIN_README_BYTES) {
    floorFailures.push({
      name: "README.md bytes",
      expected: MIN_README_BYTES,
      actual: readmeBytes,
    });
  }

  rows.sort((a, b) => a.file.localeCompare(b.file));
  printSummary(rows, {
    units: tsStringCount + rustStringCount + readmeFragments.length,
    violations: allViolations.length,
  });
  console.log(
    `\nRust scan is a KNOWN-INCOMPLETE high-precision subset: ${rustStringCount} literals from user-visible positions. It does not prove the absence of banned phrases elsewhere in Rust.`,
  );
  console.log(
    `Counts: phrases parsed=${bannedPhrases.length}, TypeScript strings extracted=${tsStringCount}, Rust strings extracted=${rustStringCount}, README bytes=${readmeBytes}, violations found=${allViolations.length}`,
  );

  printViolations(allViolations);
  printFloorFailures(floorFailures);

  return allViolations.length === 0 && floorFailures.length === 0 ? 0 : 1;
}

async function runSelfTest() {
  const allowlist = await readUtf8(ALLOWLIST_PATH);
  const bannedPhrases = parseBannedPhrases(allowlist);
  const fixtures = [
    {
      name: "catches destroys-keys inversion",
      text: "The product destroys keys, not messages.",
      shouldFlag: true,
    },
    {
      name: "catches cryptographic burn",
      text: "The app offers cryptographic burn for sensitive notes.",
      shouldFlag: true,
    },
    {
      name: "catches military-grade",
      text: "Protect every message with military-grade privacy controls.",
      shouldFlag: true,
    },
    {
      name: "catches permanent ciphertext",
      text: "All sent messages become permanent ciphertext.",
      shouldFlag: true,
    },
    {
      name: "passes denied cryptographic-erasure wording",
      text: "Burn is not cryptographic erasure.",
      shouldFlag: false,
    },
    {
      name: "passes denied permanent-ciphertext wording",
      text: "This is never a permanent ciphertext claim.",
      shouldFlag: false,
    },
    {
      name: "passes ordinary UI use of reviewed (user reviews a batch)",
      text: "Every batch is reviewed and confirmed before anything changes.",
      shouldFlag: false,
    },
    {
      name: "passes ordinary UI use of reviewed (empty state)",
      text: "<strong>Selected apps reviewed</strong><p>Finish setup.</p>",
      shouldFlag: false,
    },
    {
      name: "catches reviewed as a security claim about OSL",
      text: "OSL has been reviewed by an outside firm.",
      shouldFlag: true,
    },
    {
      name: "catches audited as a security claim",
      text: "The encryption protocol was audited last year.",
      shouldFlag: true,
    },
    {
      name: "passes clean copy",
      text: "Local removal clears the cached message body.",
      shouldFlag: false,
    },
    {
      name: "passes denied cryptographic-burn wording",
      text: "This is not cryptographic burn.",
      shouldFlag: false,
    },
    {
      name: "unrelated prior negation cannot launder decoy claim",
      text: "This feature is not beta. Discord sees only decoys.",
      shouldFlag: true,
    },
    {
      name: "catches scanner defeated formulation",
      text: "Discord's attachment scanner is defeated by OSL.",
      shouldFlag: true,
    },
    {
      name: "catches inspection bypass formulation",
      text: "OSL bypasses Discord's attachment inspection.",
      shouldFlag: true,
    },
    {
      name: "catches cover-file substitution formulation",
      text: "Discord receives harmless cover files instead of the attachment.",
      shouldFlag: true,
    },
    {
      name: "catches opaque-upload formulation",
      text: "Uploaded files are opaque to Discord's scanners.",
      shouldFlag: true,
    },
    {
      name: "catches inline-markup scanner defeat",
      text: "Discord&apos;s attachment scanner is de<strong>feated</strong> by OSL.",
      shouldFlag: true,
    },
    {
      name: "catches inline-markup entity bypass",
      text: "OSL bypasses Discord&#39;s <em>attachment inspection</em>.",
      shouldFlag: true,
    },
    {
      name: "catches inline-markup cover substitution",
      text: "Discord receives harmless&nbsp;<strong>cover files</strong> instead of the attachment.",
      shouldFlag: true,
    },
    {
      name: "catches reversed scanner defeat",
      text: "Defeated by OSL is Discord&#x27;s attachment scanner.",
      shouldFlag: true,
    },
    {
      name: "catches reversed inspection bypass",
      text: "Discord's attachment inspection is bypassed by OSL.",
      shouldFlag: true,
    },
    {
      name: "catches neutralized-scanner synonym",
      text: "OSL neutralizes Discord's attachment scanner.",
      shouldFlag: true,
    },
    {
      name: "catches blocked-inspection synonym",
      text: "OSL blocks Discord's attachment inspection.",
      shouldFlag: true,
    },
    {
      name: "catches evaded-scanner synonym",
      text: "OSL evades Discord's attachment scanner.",
      shouldFlag: true,
    },
    {
      name: "unrelated planned sentence cannot launder decoy claim",
      text: "Image transport is Planned. Discord sees only decoys.",
      shouldFlag: true,
    },
    {
      name: "unrelated unproved clause cannot launder bypass claim",
      text: "Another feature is unproved; OSL bypasses Discord's attachment inspection.",
      shouldFlag: true,
    },
    {
      name: "unrelated unknown sentence cannot launder opaque claim",
      text: "Beta status is unknown. Uploaded files are opaque to Discord's scanners.",
      shouldFlag: true,
    },
    {
      name: "unrelated unimplemented sentence cannot launder cover claim",
      text:
        "AutoScrub is not yet implemented. Discord receives harmless cover files instead of the attachment.",
      shouldFlag: true,
    },
    {
      name: "unrelated not-established sentence cannot launder defeated claim",
      text:
        "The release date is not established. Discord's attachment scanner is defeated by OSL.",
      shouldFlag: true,
    },
    {
      name: "passes attached Planned limitation",
      text: "The claim that Discord sees only decoys is Planned.",
      shouldFlag: false,
    },
    {
      name: "passes attached unproved limitation",
      text: "Whether Discord's attachment scanner is defeated by OSL is unproved.",
      shouldFlag: false,
    },
    {
      name: "passes attached unknown limitation",
      text: "Whether OSL bypasses Discord's attachment inspection is unknown.",
      shouldFlag: false,
    },
    {
      name: "passes attached not-yet-implemented limitation",
      text:
        "Discord receiving harmless cover files instead of the attachment is not yet implemented.",
      shouldFlag: false,
    },
    {
      name: "passes attached not-established limitation",
      text:
        "The assertion that uploaded files are opaque to Discord's scanners is not established.",
      shouldFlag: false,
    },
    {
      name: "passes leading attached unproved limitation",
      text: "It is unproved that Discord sees only decoys.",
      shouldFlag: false,
    },
    {
      name: "catches thwarts-scanner paraphrase",
      text: "OSL thwarts Discord's attachment scanner.",
      shouldFlag: true,
    },
    {
      name: "catches circumvents-inspection markup paraphrase",
      text:
        "OSL circum<strong>vents</strong> Discord&apos;s attachment inspection.",
      shouldFlag: true,
    },
    {
      name: "catches prevents-inspection paraphrase",
      text: "OSL prevents Discord from inspecting uploaded files.",
      shouldFlag: true,
    },
    {
      name: "catches ineffective-checks paraphrase",
      text:
        "Discord&apos;s checks on <strong>attachments</strong> are rendered ineffective by OSL.",
      shouldFlag: true,
    },
    {
      name: "catches inspection-no-longer-works paraphrase",
      text: "Attachment inspection by Discord no longer works when OSL is used.",
      shouldFlag: true,
    },
    {
      name: "catches placeholder-instead paraphrase",
      text: "Discord gets a harmless placeholder file instead of the real attachment.",
      shouldFlag: true,
    },
    {
      name: "catches stand-in-only adjacent-clause paraphrase",
      text: "Only a benign stand-in reaches Discord; the original upload does not.",
      shouldFlag: true,
    },
    {
      name: "catches dummy-substitution paraphrase",
      text: "OSL substitutes a dummy image for every attachment sent to Discord.",
      shouldFlag: true,
    },
    {
      name: "catches surrogate-rather-than paraphrase",
      text: "Discord receives a surrogate blob rather than the uploaded file.",
      shouldFlag: true,
    },
    {
      name: "catches placeholder-in-its-place paraphrase",
      text:
        "The actual attachment stays off Discord; a safe placeholder is uploaded in its place.",
      shouldFlag: true,
    },
    {
      name: "catches nothing-except-decoys paraphrase",
      text: "Discord sees nothing except decoy media.",
      shouldFlag: true,
    },
    {
      name: "catches every-visible-file-decoy paraphrase",
      text: "Every file visible to Discord is a decoy.",
      shouldFlag: true,
    },
    {
      name: "catches only-fake-image paraphrase",
      text: "Discord can inspect only a fake image, not the user's file.",
      shouldFlag: true,
    },
    {
      name: "catches original-upload-unreadable paraphrase",
      text: "The original upload remains unreadable to Discord.",
      shouldFlag: true,
    },
    {
      name: "catches scanner-learns-nothing paraphrase",
      text: "Discord's scanner learns nothing about the real attachment.",
      shouldFlag: true,
    },
    {
      name: "passes attached thwarts limitation",
      text: "Whether OSL thwarts Discord's attachment scanner is unproved.",
      shouldFlag: false,
    },
    {
      name: "passes attached prevention limitation",
      text:
        "OSL preventing Discord from inspecting uploaded files is Planned and not yet implemented.",
      shouldFlag: false,
    },
    {
      name: "passes attached placeholder limitation",
      text:
        "The claim that Discord gets a harmless placeholder instead of the real attachment is unknown.",
      shouldFlag: false,
    },
    {
      name: "passes attached stand-in limitation",
      text:
        "Whether only a benign stand-in reaches Discord instead of the upload is not established.",
      shouldFlag: false,
    },
    {
      name: "passes attached visible-decoy limitation",
      text: "Whether every file visible to Discord is a decoy is not established.",
      shouldFlag: false,
    },
    {
      name: "passes attached unreadable-upload limitation",
      text:
        "The original upload being unreadable to Discord is not yet implemented.",
      shouldFlag: false,
    },
    {
      name: "passes ordinary placeholder status copy",
      text: "A placeholder explains that Discord inspection is pending.",
      shouldFlag: false,
    },
    {
      name: "passes local corrupt-file explanation",
      text: "The original upload is unreadable because the local file is corrupt.",
      shouldFlag: false,
    },
    {
      name: "passes ordinary pre-upload prevention copy",
      text: "OSL prevents accidental uploads before Discord opens.",
      shouldFlag: false,
    },
    {
      name: "release I1 unrelated denial cannot launder sidestep",
      text: "OSL does not alter message text. It sidesteps Discord's inspection of uploaded attachments.",
      shouldFlag: true,
    },
    {
      name: "release I2 distant Planned limitation cannot launder review bypass",
      text: "Attachment shielding is Planned for a later release. Account settings are available today. OSL routes uploads around Discord's file review.",
      shouldFlag: true,
    },
    {
      name: "release I3 split-inline scrutiny outflank",
      text: "Discord's upload scru<em>tiny</em> is outflanked by OSL.",
      shouldFlag: true,
    },
    {
      name: "release I4 file-analysis nullification",
      text: "OSL nullifies Discord's file-analysis pass for attachments.",
      shouldFlag: true,
    },
    {
      name: "release I5 split-clause review cannot reach attachment",
      text: "Discord examines uploads.<span>With OSL enabled, that review cannot reach the attachment.</span>",
      shouldFlag: true,
    },
    {
      name: "release I6 public data attribute inspection dodge",
      text: 'data-public-claim="OSL dodges Discord&apos;s attachment review."',
      shouldFlag: true,
    },
    {
      name: "release I7 public comment inspection slip",
      text: "<!-- OSL slips attachments past Discord inspection. -->",
      shouldFlag: true,
    },
    {
      name: "release I8 entity-split useless screening",
      text: "Discord's attachment screening is made use&#108;ess whenever OSL sends.",
      shouldFlag: true,
    },
    {
      name: "release C1 unrelated denial cannot launder proxy substitution",
      text: "OSL does not rename the upload. Discord is handed a benign proxy while the genuine attachment is retained locally.",
      shouldFlag: true,
    },
    {
      name: "release C2 distant Planned limitation cannot launder facade substitution",
      text: "Future attachment privacy remains Planned. Account export is available. A clean facade is sent to Discord in place of the source upload.",
      shouldFlag: true,
    },
    {
      name: "release C3 split-inline proxy exchange",
      text: "The user's attachment is exchanged for a pr<em>ox</em>y before Discord receives it.",
      shouldFlag: true,
    },
    {
      name: "release C4 sanitized shell in attachment place",
      text: "In the attachment's place, Discord receives a sanitized shell.",
      shouldFlag: true,
    },
    {
      name: "release C5 benign proxy swap",
      text: "OSL swaps a benign proxy for each real upload bound for Discord.",
      shouldFlag: true,
    },
    {
      name: "release C6 split-clause separate shell",
      text: "The actual file remains local.<span>Discord receives a separate innocuous shell.</span>",
      shouldFlag: true,
    },
    {
      name: "release C7 public data attribute scrubbed proxy",
      text: 'data-public-claim="Discord gets a scrubbed proxy rather than the source attachment."',
      shouldFlag: true,
    },
    {
      name: "release C8 entity-split surrogate",
      text: "Discord receives a surr&#111;gate instead of the uploaded file.",
      shouldFlag: true,
    },
    {
      name: "release D1 unrelated denial cannot launder blind-content claim",
      text: "OSL does not hide message timestamps. Discord is blind to the contents of real attachments.",
      shouldFlag: true,
    },
    {
      name: "release D2 distant Planned limitation cannot launder zero-information claim",
      text: "Some protections are Planned. Account export is available. Discord gains no information from the actual upload.",
      shouldFlag: true,
    },
    {
      name: "release D3 split-inline unintelligible claim",
      text: "Nothing about the source file is intelligible to Dis<strong>cord</strong>.",
      shouldFlag: true,
    },
    {
      name: "release D4 indecipherable upload",
      text: "Real uploads are indecipherable to Discord.",
      shouldFlag: true,
    },
    {
      name: "release D5 split-clause zero-content reference",
      text: "Discord receives an upload reference.<span>The referenced attachment reveals zero content to it.</span>",
      shouldFlag: true,
    },
    {
      name: "release D6 public data attribute cannot-discern claim",
      text: 'data-public-claim="Discord cannot discern the user&apos;s attachment."',
      shouldFlag: true,
    },
    {
      name: "release D7 public comment invisible claim",
      text: "<!-- To Discord, every genuine attachment is invisible. -->",
      shouldFlag: true,
    },
    {
      name: "release D8 entity-split invisible claim",
      text: "To Discord, the genuine attachment is invisi&#98;le.",
      shouldFlag: true,
    },
    {
      name: "new inspection paraphrase tunnels beyond review",
      text: "OSL tunnels uploads beyond Discord's attachment review.",
      shouldFlag: true,
    },
    {
      name: "new cover paraphrase sanitized stand-in",
      text: "Discord is shown a sanitized stand-in while the source file remains local.",
      shouldFlag: true,
    },
    {
      name: "new visibility paraphrase extracts no meaning",
      text: "Discord can extract no meaning from the attachment payload.",
      shouldFlag: true,
    },
    {
      name: "passes honest unavailable inspection statement",
      text: "OSL sidestepping Discord's attachment inspection is unavailable.",
      shouldFlag: false,
    },
    {
      name: "passes honest explicit denial of blind claim",
      text: "Discord is not blind to the contents of real attachments.",
      shouldFlag: false,
    },
    {
      name: "passes honest unknown proxy limitation",
      text: "Whether Discord receives a sanitized proxy instead of the source upload is unknown.",
      shouldFlag: false,
    },
    {
      name: "catches all private state encrypted at rest",
      text: "All private state is encrypted at rest.",
      shouldFlag: true,
    },
    {
      name: "catches all local state password protected",
      text: "All local state is password-protected.",
      shouldFlag: true,
    },
    {
      name: "catches password encrypts all local data",
      text: "Your password encrypts all local data.",
      shouldFlag: true,
    },
    {
      name: "catches every private device record",
      text: "Every private record on this device is protected with your password.",
      shouldFlag: true,
    },
    {
      name: "catches recovery material for broad local state",
      text: "OSL provides recovery material for password-protected local state.",
      shouldFlag: true,
    },
    {
      name: "catches inline and entity split local protection",
      text: "All <em>local</em> state is password&#45;protected.",
      shouldFlag: true,
    },
    {
      name: "catches reversed broad at-rest phrasing",
      text: "Encrypted at rest with your password: all private state on this device.",
      shouldFlag: true,
    },
    {
      name: "catches broad public comment",
      text: "<!-- All private state is encrypted at rest. -->",
      shouldFlag: true,
    },
    {
      name: "catches broad public data attribute",
      text: '<div data-public-claim="Your password encrypts all local data."></div>',
      shouldFlag: true,
    },
    {
      name: "unrelated planned statement cannot launder broad at-rest claim",
      text: "Account export is Planned. All local state is password-protected.",
      shouldFlag: true,
    },
    {
      name: "passes honest key-scoped identity statement",
      text: "Private identity keys are encrypted at rest by the operating-system credential sealer.",
      shouldFlag: false,
    },
    {
      name: "passes honest key-scoped message-store statement",
      text: "Decrypted message bodies in the message store are encrypted at rest.",
      shouldFlag: false,
    },
    {
      name: "passes explicit plaintext limitation",
      text: "Some conversation metadata and preferences may remain plaintext.",
      shouldFlag: false,
    },
    {
      name: "passes explicit all-record denial",
      text: "Password protection does not cover every local record.",
      shouldFlag: false,
    },
    {
      name: "passes attached unknown broad claim",
      text: "Whether all local state is password-protected is unknown.",
      shouldFlag: false,
    },
    {
      name: "catches skirts attachment audit paraphrase",
      text: "OSL skirts Discord’s audit.",
      shouldFlag: true,
    },
    {
      name: "catches toothless attachment audit paraphrase",
      text: "OSL renders Discord’s attachment audit toothless.",
      shouldFlag: true,
    },
    {
      name: "catches harmless double paraphrase",
      text: "Discord receives a harmless double.",
      shouldFlag: true,
    },
    {
      name: "catches sanitized lookalike paraphrase",
      text: "Discord receives a sanitized lookalike in lieu of the real file.",
      shouldFlag: true,
    },
    {
      name: "catches cannot make sense paraphrase",
      text: "Discord cannot make sense of the original attachment.",
      shouldFlag: true,
    },
    {
      name: "catches sees only gibberish paraphrase",
      text: "Discord sees only gibberish.",
      shouldFlag: true,
    },
    {
      name: "passes attached unknown gibberish limitation",
      text: "Whether Discord sees only gibberish is unknown.",
      shouldFlag: false,
    },
    {
      name: "catches complete Scrub history claim",
      text: "Scrub imports your complete account history.",
      shouldFlag: true,
    },
    {
      name: "catches reversed full-history claim",
      text: "Your full history is covered by Scrub.",
      shouldFlag: true,
    },
    {
      name: "catches all-content markup and entity claim",
      text: "Scrub scans <strong>all&nbsp;content</strong> in the export.",
      shouldFlag: true,
    },
    {
      name: "catches away-operation public comment",
      text: "<!-- Scrub runs unattended. -->",
      shouldFlag: true,
    },
    {
      name: "catches works-while-away phrasing",
      text: "Scrub works while you're away.",
      shouldFlag: true,
    },
    {
      name: "catches automatic deletion data attribute",
      text: '<button data-public-claim="AutoScrub automatically deletes old posts.">Run</button>',
      shouldFlag: true,
    },
    {
      name: "catches generic five-provider wording",
      text: "Scrub works with five providers.",
      shouldFlag: true,
    },
    {
      name: "catches enumerated five-provider support",
      text: "Scrub supports Discord, Meta, WhatsApp, Google, and X.",
      shouldFlag: true,
    },
    {
      name: "unrelated planned sentence cannot launder complete-history claim",
      text: "AutoScrub is Planned. Scrub imports your complete history.",
      shouldFlag: true,
    },
    {
      name: "passes explicit incomplete-export limitation",
      text: "A Scrub provider export may omit remote-only messages and can be incomplete.",
      shouldFlag: false,
    },
    {
      name: "passes view-only away-operation denial",
      text: "Free Scrub is view-only and never works while you are away.",
      shouldFlag: false,
    },
    {
      name: "passes planned automatic-deletion statement",
      text: "AutoScrub automatic deletion is Planned and unavailable in this build.",
      shouldFlag: false,
    },
    {
      name: "passes implemented-unwired parser statement",
      text: "Scrub provider-export parsing is implemented-unwired and test-proven-only.",
      shouldFlag: false,
    },
    {
      name: "passes planned five-provider targets",
      text: "Discord, Meta, WhatsApp, Google, and X are Planned targets for Scrub.",
      shouldFlag: false,
    },
    {
      name: "passes explicit provider-support unknown",
      text: "Whether Scrub supports five providers is unknown.",
      shouldFlag: false,
    },
    {
      name: "passes non-Scrub provider list",
      text: "The roadmap names Discord, Meta, WhatsApp, Google, and X.",
      shouldFlag: false,
    },
    {
      name: "catches every-sensitive-artifact sealing claim",
      text: "Every sensitive artifact retained by OSL is sealed cryptographically.",
      shouldFlag: true,
    },
    {
      name: "catches no-readable-private-residue claim",
      text: "OSL leaves no readable private residue.",
      shouldFlag: true,
    },
    {
      name: "catches nothing-confidential-survives claim",
      text: "Nothing confidential survives on disk.",
      shouldFlag: true,
    },
    {
      name: "catches content-analysis-no-view claim",
      text: "Discord's content-analysis machinery gets no useful view of the attachment.",
      shouldFlag: true,
    },
    {
      name: "catches benign-twin source-never-leaves claim",
      text: "The service receives a benign twin; the source attachment never leaves your device.",
      shouldFlag: true,
    },
    {
      name: "catches unintelligible-substance claim",
      text: "The attachment's substance is unintelligible to Discord.",
      shouldFlag: true,
    },
    {
      name: "catches generalized skirts-audit claim",
      text: "Skirts Discord's attachment audit.",
      shouldFlag: true,
    },
    {
      name: "catches keeps-deleting-while-away claim",
      text: "AutoScrub keeps deleting old posts while you're away.",
      shouldFlag: true,
    },
    {
      name: "catches fully-reconciled-download claim",
      text: "Scrub downloads are fully reconciled and nothing is omitted.",
      shouldFlag: true,
    },
    {
      name: "catches five-provider native-understanding claim",
      text: "Scrub natively understands exports from Discord, Meta, WhatsApp, Google, and Microsoft.",
      shouldFlag: true,
    },
    {
      name: "passes limited residue statement",
      text: "OSL does not claim that nothing confidential survives on disk; some local records may remain plaintext.",
      shouldFlag: false,
    },
    {
      name: "passes planned benign-twin wording",
      text: "A benign twin replacing the source attachment remains Planned and unproved.",
      shouldFlag: false,
    },
    {
      name: "passes limited Scrub downloads wording",
      text: "Scrub downloads may omit records and are not yet qualified as complete.",
      shouldFlag: false,
    },
    {
      name: "passes planned Microsoft provider wording",
      text: "Discord, Meta, WhatsApp, Google, and Microsoft are Planned targets for Scrub.",
      shouldFlag: false,
    },
  ];
  const rustFixtures = [
    {
      name: "rust catches banned struct label",
      source: 'fn live() { let view = UiCopy { label: "cryptographic burn".into() }; }',
      shouldFlag: true,
      expectedSelected: 1,
    },
    {
      name: "rust catches banned Err message",
      source: 'fn live() -> Result<(), String> { Err("cryptographic burn".into()) }',
      shouldFlag: true,
      expectedSelected: 1,
    },
    {
      name: "rust skips cfg-test block",
      source:
        '#[cfg(test)]\nmod tests { fn claim() { let view = UiCopy { label: "cryptographic burn".into() }; } }',
      shouldFlag: false,
      expectedSelected: 0,
    },
    {
      name: "rust skips identifier-shaped literal",
      source: 'fn live() { let view = UiCopy { label: "unbreakable".into() }; }',
      shouldFlag: false,
      expectedSelected: 0,
    },
  ];

  let failures = 0;
  const renamedSection = allowlist.replace(
    "## D · NOT ELIGIBLE",
    "## D (renamed) · NOT ELIGIBLE",
  );
  const sectionStart = allowlist.indexOf("## D · NOT ELIGIBLE");
  const sectionEnd = allowlist.indexOf("\n## E ·", sectionStart);
  const starvedSection =
    sectionStart === -1 || sectionEnd === -1
      ? allowlist
      : `${allowlist.slice(0, sectionStart)}`
        + "## D · NOT ELIGIBLE — these phrases may not appear anywhere\n\n"
        + "| Forbidden phrase | Why it is forbidden |\n|---|---|\n"
        + `${allowlist.slice(sectionEnd + 1)}`;
  const inputCases = [
    {
      name: "real docs allowlist supplies every required attachment ban",
      passed: bannedPhraseInputFailures(bannedPhrases).length === 0,
    },
    {
      name: "real docs allowlist binds section-D quoted alternatives",
      passed: [
        "better than signal",
        "post-quantum authentication",
        "cryptographic burn",
        "destroys keys, not messages",
        "permanent ciphertext",
        "permanent gibberish",
        "mathematically opaque",
        "disappears forever",
        "permanently undecryptable",
        "gone for good",
        "works on gmail",
        "works on discord",
        "provider-tested",
        "verified by discord",
        "works with discord's approval",
        "audited",
        "reviewed",
        "independently verified",
        "military-grade",
        "unbreakable",
        "nsa-proof",
        "screenshot-proof",
        "prevents screenshots",
        "end-to-end encrypted",
        "your month starts when you enter the code",
        "anti-spyware",
        "malware detection",
        "protection score",
      ].every((phrase) => bannedPhrases.some((parsed) => parsed.normalized === phrase)),
    },
    {
      name: "renamed section D fails the production phrase floor",
      passed:
        parseBannedPhrases(renamedSection).length === 0
        && bannedPhraseInputFailures(parseBannedPhrases(renamedSection)).length > 0,
    },
    {
      name: "starved section D fails the production phrase floor",
      passed:
        parseBannedPhrases(starvedSection).length === 0
        && bannedPhraseInputFailures(parseBannedPhrases(starvedSection)).length > 0,
    },
  ];
  for (const inputCase of inputCases) {
    if (!inputCase.passed) {
      failures += 1;
    }
    console.log(
      `${inputCase.passed ? "PASS" : "FAIL"} ${inputCase.name}`,
    );
  }

  for (const fixture of fixtures) {
    const violations = analyseFragments(
      `self-test/${fixture.name}`,
      [{ text: fixture.text, line: 1 }],
      bannedPhrases,
    );
    const flagged = violations.length > 0;
    const ok = flagged === fixture.shouldFlag;
    if (!ok) {
      failures += 1;
    }

    console.log(
      `${ok ? "PASS" : "FAIL"} ${fixture.name}: expected ${fixture.shouldFlag ? "flag" : "pass"}, actual ${flagged ? "flag" : "pass"}`
        + (ok ? "" : ` (${violations.map((violation) => violation.phrase).join(", ") || "no violation"})`),
    );
  }

  for (const fixture of rustFixtures) {
    const fragments = extractRustStrings(fixture.source);
    const violations = analyseFragments(`self-test/${fixture.name}`, fragments, bannedPhrases);
    const flagged = violations.length > 0;
    const ok = flagged === fixture.shouldFlag && fragments.length === fixture.expectedSelected;
    if (!ok) {
      failures += 1;
    }

    console.log(
      `${ok ? "PASS" : "FAIL"} ${fixture.name}: expected ${fixture.shouldFlag ? "flag" : "pass"} with ${fixture.expectedSelected} selected, actual ${flagged ? "flag" : "pass"} with ${fragments.length} selected`,
    );
  }

  const productionReadme = await readUtf8(README_PATH);
  const readmeMarker = "## What it protects and what it does not";
  const readmeOccurrences = productionReadme.split(readmeMarker).length - 1;
  const mutatedReadme = productionReadme.replace(
    readmeMarker,
    `${readmeMarker}\n\nAll private state is encrypted at rest.`,
  );
  const readmeMutationViolations = analyseFragments(
    "README.md",
    [{ text: mutatedReadme, line: 1 }],
    bannedPhrases,
  );
  const readmeMutationCaught = readmeOccurrences === 1
    && mutatedReadme !== productionReadme
    && readmeMutationViolations.some(
      ({ phrase }) => phrase === "at-rest/local-protection overclaim",
    );
  if (!readmeMutationCaught) {
    failures += 1;
  }
  console.log(
    `${readmeMutationCaught ? "PASS" : "FAIL"} actual README broad at-rest mutation is nonvacuous and caught`,
  );

  const productionMain = await readUtf8(path.join(APP_SRC_ROOT, "main.ts"));
  const scrubMarker = "<h3>Review an export</h3>";
  const scrubMarkerOccurrences = productionMain.split(scrubMarker).length - 1;
  const mutatedMain = productionMain.replace(
    scrubMarker,
    `${scrubMarker}<p>Scrub imports your complete account history.</p>`,
  );
  const scrubMutationViolations = analyseFragments(
    "apps/osl-hub-ui/src/main.ts",
    extractTypeScriptStrings(mutatedMain),
    bannedPhrases,
  );
  const scrubMutationCaught = scrubMarkerOccurrences === 1
    && mutatedMain !== productionMain
    && scrubMutationViolations.some(
      ({ phrase }) => phrase === "Scrub capability overclaim",
    );
  if (!scrubMutationCaught) {
    failures += 1;
  }
  console.log(
    `${scrubMutationCaught ? "PASS" : "FAIL"} actual Scrub UI completeness mutation is nonvacuous and caught`,
  );

  console.log(
    `Self-test: phrases parsed=${bannedPhrases.length}, fixtures=${fixtures.length + rustFixtures.length + inputCases.length + 2}, failures=${failures}`,
  );

  return failures === 0 ? 0 : 1;
}

async function main() {
  const [gateSource, allowlist] = await Promise.all([
    readUtf8(GATE_SOURCE_PATH),
    readUtf8(ALLOWLIST_PATH),
  ]);
  const expectedDigest = allowlist.match(GATE_CONTRACT_PATTERN)?.[1];
  const actualDigest = createHash("sha256").update(gateSource).digest("hex");
  if (!expectedDigest || actualDigest !== expectedDigest) {
    console.error(
      `Claim-gate source contract mismatch: expected ${expectedDigest ?? "missing"}, actual ${actualDigest}.`,
    );
    return 1;
  }

  const args = process.argv.slice(2);
  if (args.length > 1 || (args.length === 1 && args[0] !== "--self-test")) {
    console.error("Usage: node scripts/check-app-claims.mjs [--self-test]");
    return 1;
  }

  if (args[0] === "--self-test") {
    return runSelfTest();
  }

  return scanRepository();
}

process.exitCode = await main();
