import { readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const cssPath = join(scriptDir, "../src/styles.css");
const source = readFileSync(cssPath, "utf8");

function blankComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, (comment) => comment.replace(/[^\n]/g, " "));
}

const css = blankComments(source);
const lineStarts = [0];
for (let index = 0; index < css.length; index += 1) {
  if (css[index] === "\n") lineStarts.push(index + 1);
}

function lineFor(index) {
  let low = 0;
  let high = lineStarts.length - 1;
  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    if (lineStarts[mid] <= index) low = mid + 1;
    else high = mid - 1;
  }
  return high + 1;
}

const definitions = new Map();
const definitionPattern = /(^|[\s{;])(--[A-Za-z0-9_-]+)\s*:/gm;
for (const match of css.matchAll(definitionPattern)) {
  const name = match[2];
  const line = lineFor(match.index + match[1].length);
  if (!definitions.has(name)) definitions.set(name, []);
  definitions.get(name).push(line);
}

function readVarUse(start) {
  let index = start + "var(".length;
  while (/\s/.test(css[index] ?? "")) index += 1;

  const nameMatch = /^--[A-Za-z0-9_-]+/.exec(css.slice(index));
  if (!nameMatch) return null;

  const name = nameMatch[0];
  index += name.length;

  let depth = 1;
  let hasFallback = false;
  for (; index < css.length; index += 1) {
    const char = css[index];
    if (char === "(") {
      depth += 1;
    } else if (char === ")") {
      depth -= 1;
      if (depth === 0) {
        return { name, line: lineFor(start), hasFallback };
      }
    } else if (char === "," && depth === 1) {
      hasFallback = true;
    }
  }

  return { name, line: lineFor(start), hasFallback };
}

const uses = [];
for (const match of css.matchAll(/var\(/gm)) {
  const use = readVarUse(match.index);
  if (use) uses.push(use);
}

const missing = new Map();
for (const use of uses) {
  if (definitions.has(use.name)) continue;
  if (use.hasFallback) continue;
  if (!missing.has(use.name)) missing.set(use.name, []);
  missing.get(use.name).push(use.line);
}

const relativeCssPath = relative(process.cwd(), cssPath);
if (missing.size > 0) {
  console.error(`CSS variable resolution check failed for ${relativeCssPath}`);
  console.error(`Unresolved variables: ${[...missing.keys()].sort().join(", ")}`);
  for (const [name, lines] of [...missing.entries()].sort(([left], [right]) => left.localeCompare(right))) {
    console.error(`  ${name}: used at ${[...new Set(lines)].join(", ")}`);
  }
  process.exit(1);
}

console.log(
  `CSS variable resolution check passed: ${uses.length} var() references resolved by ${definitions.size} custom property definitions in ${relativeCssPath}.`,
);
