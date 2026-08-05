#!/usr/bin/env node
// D-297 -- THE PROJECTION RECOGNISER, CHECKED AGAINST THE TYPESCRIPT COMPILER.
//
// Ledger 10 has to decide where a name is BOUND TO A MEMBER of something else:
// `for (const line of lines)`, `const [a, b] = parts`, `lines.map((line) => ...)`.
// It decides that with regexes and a balanced-delimiter walk, because the ledger
// must run with no dependency outside node:*. A regex that is wrong about the
// SYNTAX would be wrong silently and the census would under-report exactly the
// way D-293 did.
//
// So the same question is put to a parser that shares no code with the census:
// `typescript` out of apps/osl-hub-ui/node_modules, over exactly the .ts/.js
// files `collect()` scans.
//
//   node scripts/ledger/mutants/pins-loop-propagation-oracle.mjs
//
// WHAT THIS PROVES: that the census sees the same `for…of` statements, the same
// destructuring declarations and the same element-callback parameters the
// compiler sees, and binds the same names -- including array and object
// patterns, nested patterns, `key: target` renames, rest elements and defaults.
//
// WHAT IT CANNOT PROVE: whether a value HOLDS TEXT. That is a dataflow question
// about `readFileSync`, `?raw` and what a helper hands back, and no parser
// answers it -- exactly as the D-293 oracle could validate which imports exist
// but not which of them yield bytes. The census's answer to that half is the
// path condition, and it is graded by the mutants, not here.
//
// Exit 0 only when there are no disagreements.

import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { repoRoot, read, walk, blankComments, lineIndex, lineOf } from "../lib/io.mjs";
import { projectionSites, elementCallbackBinding, blankStringLiterals } from "../pins.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = repoRoot(process.argv);
const require = createRequire(join(ROOT, "apps/osl-hub-ui/package.json"));
let ts;
try {
  ts = require("typescript");
} catch (error) {
  console.error("ORACLE UNAVAILABLE: typescript is not installed.");
  console.error("Run `npm ci` in apps/osl-hub-ui first -- this oracle refuses to infer.");
  console.error(String(error.message));
  process.exit(9);
}

const SKIP = /(^|\/)(node_modules|dist|target|\.git|coverage|\.vite|build)(\/|$)/;
const isTsLike = (rel) => /\.(?:[cm]?[jt]sx?)$/.test(rel);
const isTsTest = (rel) => /\.test\.[cm]?[jt]sx?$/.test(rel) || /(^|\/)__tests__\//.test(rel);

/** Index of the callback parameter that is the INDEX, never an element. */
const ELEMENT_METHODS = new Map([
  ["map", 1], ["forEach", 1], ["filter", 1], ["find", 1], ["findLast", 1],
  ["findIndex", 1], ["findLastIndex", 1], ["some", 1], ["every", 1], ["flatMap", 1],
  ["reduce", 2], ["reduceRight", 2],
  ["sort", -1], ["flat", -1], ["reverse", -1], ["entries", -1], ["values", -1],
]);

/** Every identifier a binding name/pattern introduces, per the compiler. */
function boundNames(node, out = []) {
  if (!node) return out;
  if (ts.isIdentifier(node)) out.push(node.text);
  else if (ts.isArrayBindingPattern(node) || ts.isObjectBindingPattern(node)) {
    for (const el of node.elements) {
      if (ts.isOmittedExpression(el)) continue;
      boundNames(el.name, out);
    }
  }
  return out;
}

const files = walk(ROOT, ".", (rel) => {
  if (SKIP.test(rel)) return false;
  if (rel.endsWith(".rs")) return false;
  return isTsLike(rel) && (isTsTest(rel) || rel.startsWith("scripts/"));
});

const totals = { files: 0, forOf: 0, forIn: 0, forAwait: 0, destructuring: 0, callbacks: 0 };
const disagreements = [];

for (const rel of files) {
  let raw;
  try {
    raw = read(ROOT, rel);
  } catch {
    continue;
  }
  totals.files += 1;
  const src = blankComments(raw);
  const starts = lineIndex(src);
  // The COMPILER is given the file as written. `blankComments` preserves length
  // and line structure, which is all the census needs, but a blanked comment
  // inside an argument list is not always valid TypeScript -- feeding it to the
  // parser produced 44 parse errors in one file and silently emptied its tree,
  // which is a broken ORACLE reported as a census disagreement.
  const sf = ts.createSourceFile(
    rel,
    raw,
    ts.ScriptTarget.Latest,
    true,
    rel.endsWith(".tsx") ? ts.ScriptKind.TSX : /\.[cm]?ts$/.test(rel) ? ts.ScriptKind.TS : ts.ScriptKind.JS,
  );
  const line = (pos) => sf.getLineAndCharacterOfPosition(pos).line + 1;
  if (src.length !== raw.length) {
    disagreements.push(`${rel}: blankComments changed the file's length -- line numbers cannot be compared`);
    continue;
  }
  const parseErrors = (sf.parseDiagnostics ?? []).length;
  if (parseErrors) {
    disagreements.push(`${rel}: the compiler reports ${parseErrors} parse error(s) -- oracle cannot judge this file`);
    continue;
  }

  // ---- what the COMPILER says -------------------------------------------
  const oracleForOf = new Map();
  const oracleDestructuring = new Map();
  const oracleCallbacks = new Map();

  const visit = (node) => {
    if (ts.isForOfStatement(node) && ts.isVariableDeclarationList(node.initializer)) {
      if (node.awaitModifier) totals.forAwait += 1;
      totals.forOf += 1;
      const names = node.initializer.declarations.flatMap((d) => boundNames(d.name));
      if (names.length) oracleForOf.set(line(node.getStart(sf)), names.slice().sort());
    } else if (ts.isForInStatement(node)) {
      totals.forIn += 1;
    } else if (
      ts.isVariableDeclaration(node) &&
      (ts.isArrayBindingPattern(node.name) || ts.isObjectBindingPattern(node.name)) &&
      node.initializer &&
      !(node.parent.parent && ts.isForOfStatement(node.parent.parent))
    ) {
      totals.destructuring += 1;
      const names = boundNames(node.name);
      if (names.length) oracleDestructuring.set(line(node.getStart(sf)), names.slice().sort());
    } else if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression)) {
      const method = node.expression.name.text;
      const indexParam = ELEMENT_METHODS.get(method);
      if (indexParam !== undefined && node.arguments.length) {
        const cb = node.arguments[0];
        if ((ts.isArrowFunction(cb) || ts.isFunctionExpression(cb)) && cb.parameters.length) {
          const names = cb.parameters
            .filter((_, k) => k !== indexParam)
            .flatMap((param) => boundNames(param.name))
            .sort();
          if (names.length) {
            totals.callbacks += 1;
            oracleCallbacks.set(`${line(node.expression.name.getStart(sf))}:${method}`, names);
          }
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);

  // ---- what the CENSUS says ---------------------------------------------
  const censusForOf = new Map();
  const censusDestructuring = new Map();
  // The census finds projections in code, never inside a string literal, so the
  // oracle drives it on the same blanked text the census uses.
  const code = blankStringLiterals(src);
  for (const site of projectionSites(code)) {
    const target = site.form === "for-of" ? censusForOf : censusDestructuring;
    // A for-of site is keyed on its `for` keyword and a destructuring on its
    // pattern -- the two positions the compiler reports as the node's start.
    const at = site.form === "for-of" ? code.lastIndexOf("for", site.at) : site.nameAt;
    target.set(lineOf(starts, at), site.names.slice().sort());
  }

  const censusCallbacks = new Map();
  for (const m of code.matchAll(/\.([A-Za-z_$][\w$]*)\s*\(/g)) {
    const method = m[1];
    if (!ELEMENT_METHODS.has(method)) continue;
    const open = m.index + m[0].length - 1;
    let depth = 0;
    let close = -1;
    let quote = null;
    for (let i = open; i < code.length; i += 1) {
      const c = code[i];
      if (quote) {
        if (c === "\\") i += 1;
        else if (c === quote) quote = null;
        continue;
      }
      if (c === '"' || c === "'" || c === "`") quote = c;
      else if (c === "(") depth += 1;
      else if (c === ")") {
        depth -= 1;
        if (depth === 0) {
          close = i;
          break;
        }
      }
    }
    if (close === -1) continue;
    const site = elementCallbackBinding(code, method, open, close);
    if (site) censusCallbacks.set(`${lineOf(starts, m.index + 1)}:${method}`, site.names.slice().sort());
  }

  // ---- compare -----------------------------------------------------------
  const compare = (kind, oracle, census) => {
    for (const [key, names] of oracle) {
      const mine = census.get(key);
      if (!mine) {
        disagreements.push(`${rel}  ${kind} ${key}: compiler binds ${JSON.stringify(names)}, census binds NOTHING`);
      } else if (JSON.stringify(mine) !== JSON.stringify(names)) {
        disagreements.push(`${rel}  ${kind} ${key}: compiler ${JSON.stringify(names)} vs census ${JSON.stringify(mine)}`);
      }
    }
    for (const [key, names] of census) {
      if (!oracle.has(key)) {
        disagreements.push(`${rel}  ${kind} ${key}: census binds ${JSON.stringify(names)}, compiler binds NOTHING`);
      }
    }
  };
  compare("for-of", oracleForOf, censusForOf);
  compare("destructuring", oracleDestructuring, censusDestructuring);
  compare("callback", oracleCallbacks, censusCallbacks);
}

console.log(`ORACLE -- the TypeScript compiler's parser vs ledger 10's projection recogniser`);
console.log(`  typescript version:                        ${ts.version}`);
console.log(`  files parsed by BOTH:                      ${totals.files}`);
console.log(`  for…of statements seen by the compiler:    ${totals.forOf}`);
console.log(`    of which for await…of:                   ${totals.forAwait}`);
console.log(`  for…in statements seen by the compiler:    ${totals.forIn}`);
console.log(`  destructuring declarations:                ${totals.destructuring}`);
console.log(`  element-callback parameter lists:          ${totals.callbacks}`);
console.log(`  DISAGREEMENTS:                             ${disagreements.length}`);
for (const d of disagreements.slice(0, 60)) console.log(`    ${d}`);
if (disagreements.length > 60) console.log(`    ... and ${disagreements.length - 60} more`);
console.log(disagreements.length ? "  RESULT: RED" : "  RESULT: GREEN");
process.exitCode = disagreements.length ? 1 : 0;
void HERE;
