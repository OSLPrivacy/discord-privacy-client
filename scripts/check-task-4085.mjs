import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url);
const todoRoot = process.env.TASK4085_TODO_ROOT || "/home/liamw/osl-plan/OSL-AUDITS/todo";
const stalePhrase = "Only if 4085 said back in";
const expectedDate = "2026-08-07";
const expectedWords = "add all three back into the scope";
const expectedSurfaces = ["instagram", "messenger", "x"];

function readJson(path) {
  return JSON.parse(readFileSync(new URL(path, root), "utf8"));
}

function normalize(value) {
  if (Array.isArray(value)) return value.map((entry) => normalize(entry));
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, normalize(value[key])]));
  }
  return value;
}

function stable(value) {
  return JSON.stringify(normalize(value));
}

function sameJson(left, right) {
  return stable(left) === stable(right);
}

function sorted(values) {
  return [...values].sort();
}

function fail(message) {
  console.error(`TASK4085_FAIL ${message}`);
  process.exitCode = 1;
}

function* filesUnder(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      yield* filesUnder(path);
    } else if (stat.isFile()) {
      yield path;
    }
  }
}

function staleMatches(dir) {
  const matches = [];
  for (const path of filesUnder(dir)) {
    const rel = relative(dir, path);
    const lines = readFileSync(path, "utf8").split(/\r?\n/u);
    let task = "(no TASK header before match)";
    for (const [index, line] of lines.entries()) {
      if (/^TASK \d+[a-z]? - /u.test(line)) task = line;
      if (line.includes(stalePhrase)) {
        matches.push({ rel, line: index + 1, task, text: line.trim() });
      }
    }
  }
  return matches;
}

const ruling = readJson("data/surface-ruling-2026-08-05.json");
const supportMatrix = readJson("docs/status/support-matrix.json").surface_ruling;
const pricing = readJson("data/pricing.json").surface_policy.surface_ruling;

if (!sameJson(supportMatrix, ruling)) fail("docs/status/support-matrix.json surface_ruling differs from data/surface-ruling-2026-08-05.json");
if (!sameJson(pricing, ruling)) fail("data/pricing.json surface_policy.surface_ruling differs from data/surface-ruling-2026-08-05.json");

const matchingRuling = (ruling.owner_rulings || []).find((entry) =>
  entry.ruled_on === expectedDate
  && entry.ruled_by === "Liam"
  && entry.words === expectedWords
  && JSON.stringify(sorted(entry.surfaces || [])) === JSON.stringify(expectedSurfaces)
);

if (!matchingRuling) fail(`missing owner ruling date=${expectedDate} words=${JSON.stringify(expectedWords)} surfaces=${expectedSurfaces.join(",")}`);

console.log(`TASK4085_RULING_DATE=${expectedDate}`);
console.log(`TASK4085_RULING_WORDS=${JSON.stringify(expectedWords)}`);
console.log(`TASK4085_RULING_SURFACES=${expectedSurfaces.join(",")}`);
console.log(`TASK4085_TODO_ROOT=${todoRoot}`);

const matches = staleMatches(todoRoot);
for (const match of matches) {
  console.error(`TASK4085_WAITING_MATCH ${match.rel}:${match.line} ${JSON.stringify(match.task)} ${JSON.stringify(match.text)}`);
}
console.log(`TASK4085_WAITING_AFTER_COUNT=${matches.length}`);

if (matches.length !== 0) fail(`stale ${JSON.stringify(stalePhrase)} wording remains`);
