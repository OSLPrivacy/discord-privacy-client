#!/usr/bin/env node
// LEDGER 2 -- `var(--x)` used vs `--x` defined.
//
// Absorbed from B0-03's apps/osl-hub-ui/scripts/check-css-vars.mjs, whose
// comment/fallback/nesting handling is kept verbatim in spirit. Three
// extensions, each of which changes the answer:
//
//  * B0-03 reads only src/styles.css. There are 19 other .css files in src/
//    and four .html entry pages, and a var used in overlay.css is just as dead.
//  * A custom property can be DEFINED FROM TYPESCRIPT via
//    `element.style.setProperty("--x", ...)`. `--osl-composer-width`
//    (apps/osl-hub-ui/src/overlay.css:455) is set that way and is NOT a defect;
//    a ledger that only reads CSS would file it and be wrong. This is the
//    over-reporting direction the project has already been burned by.
//  * A `var()` with a fallback (`var(--x, #fff)`) does not kill its
//    declaration, so it is reported as a soft finding, not a violation.
//
//   node scripts/ledger/css-vars.mjs [--root=<dir>]

import { resolve } from "node:path";
import { repoRoot, read, walk, blankComments, lineIndex, lineOf, isTest, inputProblems } from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";

const blankCssComments = (css) => css.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " "));

/** Pull every `<style>...</style>` body out of an HTML page, position-preserving. */
function styleBlocksOnly(html) {
  const blank = (s) => s.replace(/[^\n]/g, " ");
  let out = blank(html);
  for (const m of html.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/gi)) {
    const start = m.index + m[0].indexOf(">") + 1;
    out = out.slice(0, start) + m[1] + out.slice(start + m[1].length);
  }
  return out;
}

function scanCss(css, rel, defs, uses) {
  const src = blankCssComments(css);
  const starts = lineIndex(src);
  for (const m of src.matchAll(/(^|[\s{;])(--[A-Za-z0-9_-]+)\s*:/gm)) {
    const name = m[2];
    if (!defs.has(name)) defs.set(name, []);
    defs.get(name).push(`${rel}:${lineOf(starts, m.index + m[1].length)}`);
  }
  for (const m of src.matchAll(/var\(/g)) {
    const use = readVarUse(src, m.index);
    if (use) uses.push({ ...use, site: `${rel}:${lineOf(starts, m.index)}` });
  }
}

/** Walk a `var(` forward so nested `var(--a, var(--b))` fallbacks are counted once. */
function readVarUse(css, start) {
  let i = start + 4;
  while (/\s/.test(css[i] ?? "")) i += 1;
  const nameMatch = /^--[A-Za-z0-9_-]+/.exec(css.slice(i));
  if (!nameMatch) return null;
  const name = nameMatch[0];
  i += name.length;
  let depth = 1;
  let hasFallback = false;
  for (; i < css.length; i += 1) {
    const c = css[i];
    if (c === "(") depth += 1;
    else if (c === ")") {
      depth -= 1;
      if (depth === 0) break;
    } else if (c === "," && depth === 1) hasFallback = true;
  }
  return { name, hasFallback };
}

/** Custom properties written from TypeScript are real definitions. */
function scanTsDefinitions(ts, rel, defs) {
  const src = blankComments(ts);
  const starts = lineIndex(src);
  const patterns = [
    /setProperty\(\s*["'`](--[A-Za-z0-9_-]+)["'`]/g, // el.style.setProperty("--x", v)
    /["'`](--[A-Za-z0-9_-]+)["'`]\s*:/g, //            { "--x": v } style objects
    /(--[A-Za-z0-9_-]+)\s*:\s*\$\{/g, //               `--x: ${v}` in a style string
  ];
  for (const re of patterns) {
    for (const m of src.matchAll(re)) {
      const name = m[1];
      if (!defs.has(name)) defs.set(name, []);
      defs.get(name).push(`${rel}:${lineOf(starts, m.index)}`);
    }
  }
}

export function collect(root) {
  const defs = new Map();
  const uses = [];
  const cssFiles = walk(root, "apps/osl-hub-ui/src", (r) => r.endsWith(".css"));
  const htmlFiles = walk(root, "apps/osl-hub-ui", (r) => r.endsWith(".html") && !r.includes("/node_modules/"));
  const tsFiles = walk(root, "apps/osl-hub-ui/src", (r) => r.endsWith(".ts") && !isTest(r) && !r.endsWith(".d.ts"));
  for (const rel of cssFiles) scanCss(read(root, rel), rel, defs, uses);
  for (const rel of htmlFiles) scanCss(styleBlocksOnly(read(root, rel)), rel, defs, uses);
  for (const rel of tsFiles) scanTsDefinitions(read(root, rel), rel, defs);
  return { defs, uses, counts: { cssFiles: cssFiles.length, htmlFiles: htmlFiles.length, tsFiles: tsFiles.length } };
}

export function analyse({ defs, uses }) {
  const bad = new Map();
  for (const u of uses) {
    if (defs.has(u.name)) continue;
    const kind = u.hasFallback ? "undefined-but-has-fallback" : "undefined-declaration-dropped";
    const key = `${u.name}`;
    if (!bad.has(key)) bad.set(key, { id: key, kind, detail: "", sites: [] });
    bad.get(key).sites.push(u.site);
  }
  for (const v of bad.values()) {
    v.detail =
      v.kind === "undefined-declaration-dropped"
        ? "used with no fallback and never defined in CSS or set from TypeScript -- the browser drops the whole declaration"
        : "never defined, but the use carries a fallback so the declaration survives";
  }
  return [...bad.values()];
}

export function main(argv = process.argv) {
  const root = repoRoot(argv);
  const input = inputProblems(root, ["apps/osl-hub-ui/src", "apps/osl-hub-ui/index.html"]).map((p) => ({ ...p, kind: "ledger-input-missing" }));
  if (input.length) {
    return finish(report({ id: "css-vars", title: "custom properties used vs defined, ledger 2 of 7", violations: input }));
  }
  const collected = collect(root);
  const violations = analyse(collected);
  return finish(
    report({
      id: "css-vars",
      title: "custom properties used vs defined, ledger 2 of 7",
      violations,
      stats: {
        "css files scanned": collected.counts.cssFiles,
        "html pages scanned": collected.counts.htmlFiles,
        "ts files scanned for setProperty definitions": collected.counts.tsFiles,
        "distinct properties defined": collected.defs.size,
        "var() uses": collected.uses.length,
      },
    }),
  );
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) main();
