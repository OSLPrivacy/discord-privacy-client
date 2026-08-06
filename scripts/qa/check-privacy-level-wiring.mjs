#!/usr/bin/env node

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "../..");
const DEFAULT_GUI_PLAN = path.join(REPO_ROOT, "docs/design/osl-gui-final-plan.md");

const LEVEL_RULE_LINKS = [
  {
    level: "Local protection",
    rule: "local protection",
    definitionPhrases: [
      "warn as the user types",
      "sanitize user-selected content",
      "encrypt locally",
      "show privacy state",
    ],
    requiredForNativeCompanion: true,
  },
  {
    level: "User-assisted action",
    rule: "user-assisted handoff",
    definitionPhrases: [
      "visible composer",
      "the user performs the final send or delete action",
    ],
    requiredForNativeCompanion: true,
  },
  {
    level: "Authorized automation",
    rule: "authorized automation",
    definitionPhrases: ["platform, account type and current policy allow it"],
    requiredForNativeCompanion: false,
  },
];

const NATIVE_COMPANION_SERVICES = new Set(["Discord", "Telegram", "Signal", "WhatsApp"]);

function parseArgs(argv) {
  const args = { guiPlan: DEFAULT_GUI_PLAN };
  for (let index = 2; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--gui-plan") {
      const value = argv[index + 1];
      if (!value) throw new Error("--gui-plan requires a path");
      args.guiPlan = path.resolve(value);
      index += 1;
      continue;
    }
    throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

function section(markdown, heading) {
  const marker = `\n${heading}\n`;
  const start = markdown.indexOf(marker);
  if (start < 0) throw new Error(`${heading} section is missing`);
  const body = markdown.slice(start + marker.length);
  const level = heading.match(/^#+/)?.[0].length ?? 0;
  const next = body.search(new RegExp(`\\n#{1,${level}}(?!#) `));
  return next >= 0 ? body.slice(0, next) : body;
}

function tableRows(markdownSection, expectedHeader) {
  const lines = markdownSection.split(/\r?\n/).filter((line) => line.startsWith("|"));
  const headerIndex = lines.findIndex((line) =>
    line.split("|").some((cell) => cleanCell(cell) === expectedHeader),
  );
  if (headerIndex < 0) throw new Error(`${expectedHeader} table is missing`);
  const headers = splitRow(lines[headerIndex]);
  const rows = [];
  for (const line of lines.slice(headerIndex + 2)) {
    const cells = splitRow(line);
    if (cells.length !== headers.length) break;
    rows.push(Object.fromEntries(headers.map((header, index) => [header, cells[index]])));
  }
  return rows;
}

function splitRow(line) {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map(cleanCell);
}

function cleanCell(cell) {
  return cell.trim().replace(/\*\*/g, "").replace(/`/g, "");
}

function normalize(text) {
  return cleanCell(text).replace(/\s+/g, " ").trim().toLowerCase();
}

function validate(markdown) {
  const errors = [];
  const platformBoundary = section(markdown, "## Platform compatibility boundary");
  const levels = new Map(
    tableRows(platformBoundary, "Level").map((row) => [cleanCell(row.Level), row["What OSL may do"]]),
  );

  for (const link of LEVEL_RULE_LINKS) {
    const levelRule = levels.get(link.level);
    if (!levelRule) {
      errors.push(`PRIVACY_LEVEL_MISSING: ${link.level}`);
      continue;
    }
    const normalizedRule = normalize(levelRule);
    for (const phrase of link.definitionPhrases) {
      if (!normalizedRule.includes(phrase)) {
        errors.push(`PRIVACY_LEVEL_DEFINITION_INCOMPLETE: ${link.level} missing "${phrase}"`);
      }
    }
  }

  const connections = section(markdown, "## Connections");
  const matrixRows = tableRows(connections, "Service");
  const nativeRows = matrixRows.filter((row) => NATIVE_COMPANION_SERVICES.has(cleanCell(row.Service)));
  for (const service of NATIVE_COMPANION_SERVICES) {
    if (!nativeRows.some((row) => cleanCell(row.Service) === service)) {
      errors.push(`PRIVACY_LEVEL_SERVICE_MISSING: ${service}`);
    }
  }

  for (const row of nativeRows) {
    const service = cleanCell(row.Service);
    const defaultLevel = normalize(row["Default action level"]);
    for (const link of LEVEL_RULE_LINKS.filter((item) => item.requiredForNativeCompanion)) {
      if (!defaultLevel.includes(link.rule)) {
        errors.push(
          `PRIVACY_LEVEL_RULE_MISSING: ${service} missing changed rule "${link.rule}" for ${link.level}`,
        );
      }
    }
  }

  return errors;
}

function main() {
  const { guiPlan } = parseArgs(process.argv);
  const markdown = readFileSync(guiPlan, "utf8");
  const errors = validate(markdown);
  if (errors.length > 0) {
    for (const error of errors) console.error(error);
    process.exit(1);
  }
  console.log("check-privacy-level-wiring: complete.");
}

main();
