// TASK 4760 - the search behind "exactly 1 code path can move the setting to anyone".
//
// It reads every TypeScript and JavaScript file the hub UI ships (src, its
// screenshot fixtures and these scripts) and finds every place that writes the
// `setting` field of a discovery-setting store. Two shapes count as a write:
//
//   * `<something>.setting = <value>;`
//   * an object literal typed `: DiscoverySettingStore = { setting: <value> ...`
//
// Each write is then judged:
//
//   ANYONE      the value written is the `anyone` value itself. There must be
//               exactly one of these in the whole tree.
//   LITERAL     the value written is one of the other three values, spelled
//               out. It cannot become `anyone`.
//   GUARDED     the value written is a variable, and the function it sits in
//               branches on `=== DISCOVERY_ANYONE` first, so `anyone` never
//               reaches that line.
//   UNGUARDED   anything else. One of these fails the search.
//
// Exit 0 only when there is exactly one ANYONE write, it lives in a
// module-private function, and no write is UNGUARDED. Anything else exits 1
// and prints what it found.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const UI_ROOT = resolve(new URL("..", import.meta.url).pathname);
const SEARCH_ROOTS = ["src", "screenshots", "scripts"].map((dir) => join(UI_ROOT, dir));
const SKIP_DIRS = new Set(["node_modules", "dist", "evidence", "assets"]);
const CODE = /\.(ts|tsx|mjs|js)$/u;

const ANYONE_VALUE = "anyone";
const ANYONE_CONSTANT = "DISCOVERY_ANYONE";

function walk(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return out;
  }
  for (const entry of entries) {
    if (SKIP_DIRS.has(entry)) continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) walk(path, out);
    else if (CODE.test(entry)) out.push(path);
  }
  return out;
}

/** Comments and doc blocks describe code, they do not run it. */
function isComment(line) {
  return /^\s*(\/\/|\/\*|\*)/u.test(line);
}

/** The function a line sits in: the nearest function header above it. */
function enclosingFunction(lines, index) {
  for (let cursor = index; cursor >= 0; cursor -= 1) {
    const header = lines[cursor].match(/^(export\s+)?function\s+(\w+)/u);
    if (header) return { name: header[2], exported: Boolean(header[1]), startsAt: cursor };
  }
  return { name: "<file scope>", exported: true, startsAt: 0 };
}

function functionBody(lines, startsAt) {
  const body = [];
  for (let cursor = startsAt; cursor < lines.length; cursor += 1) {
    body.push(lines[cursor]);
    if (cursor > startsAt && /^\}/u.test(lines[cursor])) break;
  }
  return body.join("\n");
}

const writes = [];
for (const root of SEARCH_ROOTS) {
  for (const file of walk(root)) {
    const text = readFileSync(file, "utf8");
    if (!text.includes("setting")) continue;
    const lines = text.split("\n");
    lines.forEach((line, index) => {
      if (isComment(line)) return;
      // `=` and not `==`, `===`, `!==`, `<=`, `>=`.
      const assignment = line.match(/(\w+)\.setting\s*(?<![=!<>])=(?!=)\s*([^;]+);/u);
      const typedLiteral = line.match(/:\s*DiscoverySettingStore\s*=\s*\{[^}]*\bsetting\s*:\s*([^,}]+)/u);
      const value = assignment ? assignment[2].trim() : typedLiteral ? typedLiteral[1].trim() : null;
      if (value === null) return;
      const where = enclosingFunction(lines, index);
      const body = functionBody(lines, where.startsAt);
      const writesAnyone = value === ANYONE_CONSTANT || value === `"${ANYONE_VALUE}"` || value === `'${ANYONE_VALUE}'`;
      const isLiteral = /^["'][a-z-]+["']$/u.test(value);
      const guarded = body.includes(`=== ${ANYONE_CONSTANT}`) || body.includes(`=== "${ANYONE_VALUE}"`);
      const verdict = writesAnyone ? "ANYONE" : isLiteral ? "LITERAL" : guarded ? "GUARDED" : "UNGUARDED";
      writes.push({
        file: relative(UI_ROOT, file),
        line: index + 1,
        value,
        fn: where.name,
        exported: where.exported,
        verdict,
      });
    });
  }
}

const anyoneWrites = writes.filter((write) => write.verdict === "ANYONE");
const unguarded = writes.filter((write) => write.verdict === "UNGUARDED");

console.log(`TASK4760_SEARCH_FILES roots=${SEARCH_ROOTS.map((root) => relative(UI_ROOT, root)).join(",")} files_read=${SEARCH_ROOTS.reduce((sum, root) => sum + walk(root).length, 0)}`);
for (const write of writes) {
  console.log(
    `TASK4760_SETTING_WRITE ${write.verdict} ${write.file}:${write.line}`
    + ` fn=${write.fn} exported=${write.exported} value=${write.value}`,
  );
}
console.log(`TASK4760_ANYONE_WRITE_SITES ${anyoneWrites.length}`);
console.log(`TASK4760_UNGUARDED_WRITE_SITES ${unguarded.length}`);

// Who can call the one write site, and is it reachable from outside its module?
let callerReport = "";
if (anyoneWrites.length === 1) {
  const owner = join(UI_ROOT, anyoneWrites[0].file);
  const lines = readFileSync(owner, "utf8").split("\n");
  const writerName = anyoneWrites[0].fn;
  const definition = lines.findIndex((line) => new RegExp(`function\\s+${writerName}\\b`, "u").test(line));
  const exportedWriter = /^export\s+function/u.test(lines[definition] ?? "");
  const callers = [];
  lines.forEach((line, index) => {
    if (index === definition || isComment(line)) return;
    if (!line.includes(`${writerName}(`)) return;
    callers.push(`${enclosingFunction(lines, index).name}:${index + 1}`);
  });
  callerReport = `TASK4760_ONE_WRITE_SITE fn=${writerName} exported=${exportedWriter}`
    + ` callers=${callers.length} via=${callers.join(",")}`;
  console.log(callerReport);
  if (exportedWriter) {
    console.error("the one anyone write site is exported, so other modules can call it directly");
    process.exit(1);
  }
}

if (anyoneWrites.length !== 1) {
  console.error(`the setting reached anyone from ${anyoneWrites.length} code paths, expected exactly 1`);
  process.exit(1);
}
if (unguarded.length > 0) {
  console.error(`${unguarded.length} discovery setting writes are not guarded against anyone`);
  process.exit(1);
}
console.log("TASK4760_SEARCH_RESULT exactly 1 code path can move the setting to anyone");
