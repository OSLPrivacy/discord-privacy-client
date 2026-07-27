#!/usr/bin/env node

import { promises as fs } from "node:fs";
import path from "node:path";
import process from "node:process";

const REPO_ROOT = path.resolve(import.meta.dirname, "..");
const ALLOWLIST_PATH = path.join(
  REPO_ROOT,
  "docs/design/osl-public-claim-allowlist.md",
);
const APP_SRC_ROOT = path.join(REPO_ROOT, "apps/osl-hub-ui/src");
const README_PATH = path.join(REPO_ROOT, "README.md");

const MIN_BANNED_PHRASES = 8; // Prevents a malformed section-D parse from approving everything.
const MIN_TS_STRING_LITERALS = 300; // Ensures the app copy scan cannot pass after extracting nothing.
const MIN_README_BYTES = 1; // Ensures the public README claim surface was actually scanned.

const NEGATOR_LOOKBACK_CHARS = 60;
const NEGATOR_RE =
  /(?:\bdoes\s+not\b|\bis\s+not\b|\bcannot\b|\bwithout\b|\bnever\b|\bnot\b|\bno\s|n't\b)/i;

function repoRelative(filePath) {
  return path.relative(REPO_ROOT, filePath).split(path.sep).join("/");
}

async function readUtf8(filePath) {
  return fs.readFile(filePath, "utf8");
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
        const normalized = alternative.trim().toLowerCase();
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

function excerptAround(text, index, length) {
  const start = Math.max(0, index - 50);
  const end = Math.min(text.length, index + length + 50);
  return text
    .slice(start, end)
    .replace(/\s+/g, " ")
    .trim();
}

function hasNegatorBefore(text, index) {
  const before = text.slice(Math.max(0, index - NEGATOR_LOOKBACK_CHARS), index);
  return NEGATOR_RE.test(before);
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
    const lower = fragment.text.toLowerCase();

    for (const phrase of bannedPhrases) {
      const contextGated = CONTEXT_GATED_TERMS.has(phrase.normalized);
      let index = lower.indexOf(phrase.normalized);
      while (index !== -1) {
        const gatedOut =
          contextGated && !inSecurityContext(lower, index, phrase.normalized.length);
        if (!gatedOut && !hasNegatorBefore(lower, index)) {
          violations.push({
            file,
            line: fragment.line + countNewlinesBefore(fragment.text, index),
            phrase: phrase.display,
            excerpt: excerptAround(fragment.text, index, phrase.normalized.length),
          });
        }

        index = lower.indexOf(phrase.normalized, index + phrase.normalized.length);
      }
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

async function scanRepository() {
  const bannedPhrases = await loadBannedPhrases();
  const rows = [];
  const allViolations = [];
  const floorFailures = [];
  let tsStringCount = 0;
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

  if (bannedPhrases.length < MIN_BANNED_PHRASES) {
    floorFailures.push({
      name: "banned phrases parsed from section D",
      expected: MIN_BANNED_PHRASES,
      actual: bannedPhrases.length,
    });
  }

  if (tsStringCount < MIN_TS_STRING_LITERALS) {
    floorFailures.push({
      name: "TypeScript string literals extracted",
      expected: MIN_TS_STRING_LITERALS,
      actual: tsStringCount,
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
    units: tsStringCount + readmeFragments.length,
    violations: allViolations.length,
  });
  console.log(
    `\nCounts: phrases parsed=${bannedPhrases.length}, strings extracted=${tsStringCount}, README bytes=${readmeBytes}, violations found=${allViolations.length}`,
  );

  printViolations(allViolations);
  printFloorFailures(floorFailures);

  return allViolations.length === 0 && floorFailures.length === 0 ? 0 : 1;
}

async function runSelfTest() {
  const bannedPhrases = await loadBannedPhrases();
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
  ];

  let failures = 0;
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
      `${ok ? "PASS" : "FAIL"} ${fixture.name}: expected ${fixture.shouldFlag ? "flag" : "pass"}, actual ${flagged ? "flag" : "pass"}`,
    );
  }

  console.log(
    `Self-test: phrases parsed=${bannedPhrases.length}, fixtures=${fixtures.length}, failures=${failures}`,
  );

  return failures === 0 ? 0 : 1;
}

async function main() {
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
