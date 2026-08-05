#!/usr/bin/env node
// LEDGER 10 -- THE PIN CENSUS. Every assertion that reads SOURCE TEXT instead of
// executing behaviour, enumerated by identity and ratcheted in both directions.
//
// WHY THIS EXISTS
//
// Source-text pins are the only recurring defect shape in this project that no
// mechanism can see. PLAN.md r4-3 item 3 and audit-results/PLAN-AUDIT-method.md
// §3.1 name the tally:
//
//   D-190  a `Set`'s size changed 200 lines away from the pin and every pin
//          stayed green -- the pin was measuring spelling, not the set.
//   D-192  a count was pinned where a set was meant; the correct implementation
//          (scripts/ledger/state-baseline.json) sat 30 lines away.
//   D-221  osl-mail-integration.test.ts pinned the literal source line, so the
//          pin would have gone RED on the HONEST fix. Its own words: "Third
//          source-text pin to hide a real defect in two days."
//   D-225  a pin asserting a file that is not compiled -- the assertion could
//          not have been wrong about behaviour, because no behaviour ran.
//   the OSL Chat label, same shape again.
//
// Nothing in the tree distinguishes "this test asserts a string appears in a
// file" from "this test executes the behaviour". A human auditor was asked to
// notice, five times, and did not.
//
// WHAT THIS IS NOT
//
// This ledger does NOT ban pins and does NOT delete them. Some are legitimate:
// asserting a value in a config file, asserting a claim in a document, asserting
// that a workflow still names a job. A pin over a file that has no behaviour to
// execute is the right tool. The census makes the population VISIBLE and BOUNDED
// so that adding one is a deliberate, recorded decision instead of the default
// way to make a red test green.
//
// THE RATCHET -- both directions, exactly as scripts/ledger/all.mjs:20-29
//
//   live count > baseline      -> FAIL. A new pin was written. Record it in
//                                 scripts/ledger/pin-baseline.json deliberately,
//                                 or write a test that executes the behaviour.
//   live count < baseline      -> FAIL, "the baseline is stale". A pin was
//                                 removed and the baseline was not lowered in
//                                 the same change. A ratchet checked in one
//                                 direction rots into a floor nobody lowers.
//   same count, different ids  -> FAIL. One was removed and one added, and the
//                                 number alone hid the swap. all.mjs:28.
//
// PIN IDENTITY -- why it is not `file:line`
//
// A `file:line` id would churn on every unrelated edit above it, and a ratchet
// that fires on noise is a ratchet that gets deleted. An id is
//
//     <repo-relative path>#<subject>~<ordinal among that subject in that file>
//
// where `subject` is the *expression being asserted on* (`source`,
// `mainSource`, `<inline>`), never the literal it is compared against. So:
//
//   * editing the string a pin asserts changes NOTHING here -- that is the
//     pin's own business, and it already goes red on its own;
//   * adding or removing a pin changes exactly one id;
//   * moving code up or down a file changes nothing.
//
// The FILE AND LINE OF EVERY PIN is still printed in the report, computed live
// from the tree, so the next person reads the list rather than reconstructing it.
//
// USAGE
//
//   node scripts/ledger/pins.mjs                 # report + ratchet
//   node scripts/ledger/pins.mjs --list          # every pin with file:line
//   node scripts/ledger/pins.mjs --write-baseline  # regenerate (review the diff)
//   node scripts/ledger/pins.mjs --root=<dir>    # scan a mutated copy of a tree

import { readFileSync, writeFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { repoRoot, read, walk, blankComments, blankRustComments, lineIndex, lineOf } from "./lib/io.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
export const PIN_BASELINE_PATH = join(HERE, "pin-baseline.json");
const BASELINE_REL = "scripts/ledger/pin-baseline.json";

/** Directories that are not this project's source. */
const SKIP = /(^|\/)(node_modules|dist|target|\.git|coverage|\.vite|build)(\/|$)/;

// ---------------------------------------------------------------------------
// HOW FILE TEXT ENTERS A SPEC -- the two doors, D-293
//
// This census used to model ONE door: a CALL that hands back a file's bytes
// (`TEXT_READERS` below). That is why `import migration from "./0004.sql?raw"`
// was invisible -- an import is not a call, so no entry in any call list could
// ever have caught it, and adding `?raw` to the list would have been the same
// defect with a longer list (D-242, D-270, D-271: "the census sees what its
// recogniser was written to look for").
//
// There are exactly two syntactic doors in JavaScript/TypeScript through which
// the bytes of a file on disk can reach a variable:
//
//   1. a CALL that reads the file at run time            -> TEXT_READERS
//   2. an IMPORT whose SPECIFIER names a file the module
//      loader hands over as bytes rather than executing   -> importYieldsFileText
//
// Door 2 is classified by a decision procedure over the specifier, not by an
// enumeration of importable things:
//
//   * an explicit "give me the bytes" query -- Vite's `?raw`, in any of its
//     composed forms (`?raw`, `?foo&raw`) -- is decisive whatever the file is,
//     so one rule covers .sql, .toml, .ts, .md and anything added later;
//   * otherwise a RELATIVE specifier is resolved AGAINST THE TREE. If it names
//     a file that exists and whose extension is not an executable module
//     extension, a loader can only hand it over as an asset. The set below is
//     closed by the language (what may be `import`ed AS CODE) rather than open
//     like a reader list, so this rule does not rot as file types are added.
//
// The second rule asks the disk rather than trusting the spelling, and the
// TypeScript-parser oracle is what showed why: `import viteConfig from
// "../vite.config"` in apps/osl-hub-ui/src/whatsapp-shipping-surface.test.ts
// has a trailing dotted segment that reads as the extension `config`, and it is
// nothing of the kind -- `apps/osl-hub-ui/vite.config` does not exist, and the
// loader resolves it onto `vite.config.ts`, a MODULE. A purely syntactic
// complement rule called that file text. Resolution says it is not.
//
// `.json` sits with the module extensions ON PURPOSE: an imported JSON module
// is PARSED data, and asserting on parsed data is not a source-text pin --
// exactly the judgement `scanTs` already makes when it skips `JSON.parse`.
//
// Everything downstream is unchanged. An import-origin binding enters the same
// `bindings` set as a reader-origin one and flows through the same
// text-preserving fixed point, so a `?raw` import sliced, split or handed to a
// `: string` helper is followed for free. That the rest of `scanTs` needed no
// edit is the evidence that this is a missing ORIGIN and not a missing entry.
// ---------------------------------------------------------------------------

/**
 * A call that hands back the raw text of a file on disk, or of a subprocess's
 * captured output. Door 1.
 *
 * `execFileSync` was MISSING and its absence hid 13 assertions, measured. That
 * is not a list needing another entry -- Node's synchronous output-capturing
 * APIs are a CLOSED set of exactly three (`execSync`, `execFileSync`,
 * `spawnSync`) and two of the three were written down, so the census counted
 * `expect(execSync(...))` and scored the identical `expect(execFileSync(...))`
 * as absent. The async forms are deliberately NOT here: `exec`, `execFile` and
 * `spawn` hand back a ChildProcess, not text.
 *
 * Matching is by substring, so `readFile` also covers `readFileSync` and
 * `fs.promises.readFile`.
 */
const TEXT_READERS = [
  "readFileSync",
  "readFile",
  "readTextFile",
  "execSync",
  "execFileSync",
  "spawnSync",
  "include_str!",
  "read_to_string",
];

/**
 * Extensions a module loader EXECUTES or PARSES rather than handing over as
 * bytes. The complement of this set is "a file, not a module".
 */
const MODULE_EXTENSIONS = new Set([
  "js", "mjs", "cjs", "jsx", "ts", "tsx", "mts", "cts", "json", "node", "wasm",
]);

/**
 * Does importing `spec` from a module in `fromDir` bind the TEXT of a file on
 * disk? Door 2.
 *
 * `fromDir` is the absolute directory of the IMPORTING file. Without it only
 * the explicit `?raw` contract can be decided, and the complement rule -- which
 * has to ask whether a path names a real non-module file -- answers "no",
 * which is the pre-D-293 behaviour and therefore never inflates a census.
 *
 * Exported so the decision can be tested directly, and so the next reader can
 * see that it is a rule and not a list.
 */
export function importYieldsFileText(spec, fromDir = null) {
  if (typeof spec !== "string" || !spec) return false;
  const q = spec.indexOf("?");
  const path = q === -1 ? spec : spec.slice(0, q);
  const query = q === -1 ? "" : spec.slice(q + 1);
  // Vite's `?raw` is the explicit contract: hand me this file as a string.
  if (query.split("&").some((p) => p === "raw" || p.startsWith("raw="))) return true;
  // Only a path INTO THIS TREE can be a file; a bare specifier is a package.
  if (!/^[./]/.test(path)) return false;
  const ext = (/\.([A-Za-z0-9]+)$/.exec(path)?.[1] ?? "").toLowerCase();
  if (!ext || MODULE_EXTENSIONS.has(ext)) return false;
  if (!fromDir) return false;
  // Ask the tree, not the spelling. `../vite.config` "ends in .config" and is
  // extensionless module resolution onto vite.config.ts.
  try {
    return statSync(join(fromDir, path)).isFile();
  } catch {
    return false;
  }
}

/** The local names an import clause introduces. `type` imports bind nothing. */
function importClauseBindings(clause) {
  const names = [];
  const text = clause.trim();
  if (/^type\b/.test(text)) return names;
  const braced = /\{([\s\S]*)\}/.exec(text);
  const head = text.replace(/\{[\s\S]*\}/, "").replace(/,\s*$/, "").trim();
  if (head) {
    const ns = /^\*\s*as\s+([A-Za-z_$][\w$]*)$/.exec(head);
    if (ns) names.push(ns[1]);
    else {
      const def = /^([A-Za-z_$][\w$]*)$/.exec(head);
      if (def) names.push(def[1]);
    }
  }
  if (braced) {
    for (const part of braced[1].split(",")) {
      const t = part.trim();
      if (!t || /^type\b/.test(t)) continue;
      const as = /\bas\s+([A-Za-z_$][\w$]*)$/.exec(t);
      if (as) names.push(as[1]);
      else if (/^[A-Za-z_$][\w$]*$/.test(t)) names.push(t);
    }
  }
  return names;
}

/**
 * Bindings introduced by `import x from "<a file, not a module>"`.
 *
 * `import(...)` and `import.meta` are excluded by the lookahead; the dynamic
 * form is picked up in the declaration loop instead, where its binding is a
 * declaration like any other.
 */
export function tsImportTextBindings(src, fromDir = null) {
  const names = new Set();
  const re = /\bimport\s+(?![(.])([\s\S]{0,400}?)\s+from\s*(["'])([^"']+)\2/g;
  for (const m of src.matchAll(re)) {
    if (!importYieldsFileText(m[3], fromDir)) continue;
    for (const name of importClauseBindings(m[1])) names.add(name);
  }
  return names;
}

/** `await import("./x.sql?raw")` -- the same door, spelled as an expression. */
function initImportsFileText(init, fromDir = null) {
  for (const m of init.matchAll(/\bimport\s*\(\s*(["'`])([^"'`]+)\1/g)) {
    if (importYieldsFileText(m[2], fromDir)) return true;
  }
  return false;
}

/** Matchers that judge TEXT. `toBe`/`toEqual` count when the subject is text. */
const TS_MATCHERS = [
  "toContain",
  "toMatch",
  "toBe",
  "toEqual",
  "toStrictEqual",
  "toHaveLength",
  "toBeTruthy",
  "toBeFalsy",
  "toBeGreaterThan",
  "toBeGreaterThanOrEqual",
  "toBeLessThan",
  "toBeLessThanOrEqual",
  "toContainEqual",
];

/** Rust text operations. A pin is an assertion built out of one of these. */
const RUST_TEXT_OPS = [".contains(", ".starts_with(", ".ends_with(", ".find(", ".matches(", ".lines()", ".split("];

const isTsLike = (rel) => /\.(?:[cm]?[jt]sx?)$/.test(rel);
const isTsTest = (rel) => /\.test\.[cm]?[jt]sx?$/.test(rel) || /(^|\/)__tests__\//.test(rel);

/**
 * Walk balanced delimiters from an opening bracket, respecting string and
 * template literals. Returns the index of the matching closer, or -1.
 *
 * This is the difference between a census and a grep: `expect(source).toContain("(")`
 * must not desynchronise the scanner, and a regex cannot see that.
 */
function matchDelimiter(src, open) {
  const pairs = { "(": ")", "[": "]", "{": "}" };
  const closer = pairs[src[open]];
  if (!closer) return -1;
  let depth = 0;
  let quote = null;
  for (let i = open; i < src.length; i += 1) {
    const c = src[i];
    if (quote) {
      if (c === "\\") {
        i += 1;
        continue;
      }
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      quote = c;
      continue;
    }
    if (c === src[open]) depth += 1;
    else if (c === closer) {
      depth -= 1;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/** The `.foo(...).bar(...)` chain that follows a closing paren, as raw text. */
function chainAfter(src, closeIndex) {
  let i = closeIndex + 1;
  const start = i;
  while (i < src.length) {
    const c = src[i];
    if (c === "(") {
      const end = matchDelimiter(src, i);
      if (end === -1) break;
      i = end + 1;
      continue;
    }
    if (/[\w.$?!]/.test(c) || c === " " || c === "\n") {
      i += 1;
      continue;
    }
    break;
  }
  return src.slice(start, i);
}

// ---------------------------------------------------------------------------
// TypeScript / JavaScript
// ---------------------------------------------------------------------------

/**
 * Identifiers in this file that hold the TEXT of a file on disk.
 *
 * Two hops, because every test in this tree uses one of them:
 *   const source = readFileSync(...)                     -- direct
 *   const readSrc = (p) => readFileSync(...); const s = readSrc("./main.ts")
 */
function tsTextBindings(src, fromDir = null) {
  const readers = new Set();
  // Door 2 seeds the SAME set door 1 fills. Everything after this line treats
  // an import-origin binding and a reader-origin binding identically, which is
  // the whole point of fixing this as an origin rather than as a list entry.
  const bindings = tsImportTextBindings(src, fromDir);
  const textHelpers = new Set();

  const declaration = /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=]{0,80})?=\s*/g;
  const decls = [];
  for (const m of src.matchAll(declaration)) {
    const bodyStart = m.index + m[0].length;
    // Take the initialiser to the end of the statement, brace-aware.
    let end = bodyStart;
    let depth = 0;
    let quote = null;
    for (; end < src.length; end += 1) {
      const c = src[end];
      if (quote) {
        if (c === "\\") end += 1;
        else if (c === quote) quote = null;
        continue;
      }
      if (c === '"' || c === "'" || c === "`") quote = c;
      else if ("([{".includes(c)) depth += 1;
      else if (")]}".includes(c)) {
        if (depth === 0) break;
        depth -= 1;
      } else if ((c === ";" || c === "\n") && depth === 0) {
        if (c === ";") break;
        // a newline only ends the statement when the next line starts a new one
        const rest = src.slice(end + 1, end + 40);
        if (/^\s*(?:const|let|var|function|export|import|\})/.test(rest)) break;
      }
    }
    decls.push({ name: m[1], init: src.slice(bodyStart, end) });
  }

  // Every named function body, so a helper can be re-judged once we know what
  // the module-level bindings are.
  const functions = [];
  for (const m of src.matchAll(/\bfunction\s+([A-Za-z_$][\w$]*)\s*\(/g)) {
    const open = src.indexOf("{", m.index);
    if (open === -1) continue;
    const close = matchDelimiter(src, open);
    if (close === -1) continue;
    const signature = src.slice(m.index, open);
    functions.push({ name: m[1], body: src.slice(open, close), returnsString: /\)\s*:\s*string\b/.test(signature) });
  }
  for (const f of functions) if (TEXT_READERS.some((r) => f.body.includes(r))) readers.add(f.name);
  for (const d of decls) {
    if (initImportsFileText(d.init, fromDir) && !d.init.includes("JSON.parse")) {
      bindings.add(d.name);
      continue;
    }
    if (TEXT_READERS.some((r) => d.init.includes(r))) {
      // An arrow/function value that READS is a reader; anything else is text.
      if (/^\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*=>/.test(d.init) || /^\s*(?:async\s+)?function\b/.test(d.init)) {
        readers.add(d.name);
      } else if (!d.init.includes("JSON.parse")) {
        bindings.add(d.name);
      }
    }
  }
  // Helpers DECLARED to return a string. `function slice(src: string, from:
  // string): string` hands back file text when it is given file text; a
  // helper that returns anything else has turned the text into behaviour and
  // is not a pin. The return annotation is the discriminator, not a guess.
  for (const m of src.matchAll(/\bfunction\s+([A-Za-z_$][\w$]*)\s*\([^)]*\)\s*:\s*string\b/g)) textHelpers.add(m[1]);
  for (const m of src.matchAll(/\b(?:const|let)\s+([A-Za-z_$][\w$]*)\s*(?::[^=]{0,120})?=\s*(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*:\s*string\s*=>/g)) {
    textHelpers.add(m[1]);
  }

  // Propagate ONLY through text-preserving steps, to a fixed point.
  //
  // An unbounded fixed point is wrong and was measured to be wrong: with
  // `const app = createApp(source)` treated as text, every assertion about the
  // running app became a "pin" and the census tripled. Feeding source text into
  // something that RUNS is the opposite of a pin.
  const stringMethod = /\.(?:slice|substring|substr|split|replace|replaceAll|trim|trimStart|trimEnd|toLowerCase|toUpperCase|normalize|padStart|padEnd|repeat|concat|join|match|matchAll|search|at|charAt|indexOf|lastIndexOf|includes|startsWith|endsWith|length)\b/;

  for (let pass = 0; pass < 6; pass += 1) {
    const before = bindings.size + readers.size;

    // A helper that CLOSES OVER file text and is declared to hand text back is
    // a reader, whatever its arguments are. This is not academic: every
    // `expect(presetContent)` in onboarding-presets.test.ts flows through
    //     function functionSource(name, nextName): string { ... source.slice(..) }
    // whose only arguments are literals. Without this rule the census read that
    // file as TWO pins instead of twenty-two, and deleting one of them did not
    // move the count -- which is a ratchet that cannot fail, measured.
    for (const f of functions) {
      if (readers.has(f.name) || !f.returnsString) continue;
      const overText =
        [...bindings].some((b) => new RegExp(`\\b${b}\\b`).test(f.body)) ||
        [...readers].some((r) => new RegExp(`\\b${r}\\s*\\(`).test(f.body));
      if (overText) readers.add(f.name);
    }

    for (const d of decls) {
      if (bindings.has(d.name) || d.init.includes("JSON.parse")) continue;
      if (readers.has(d.name)) continue;
      const overText = [...bindings].some((b) => new RegExp(`\\b${b}\\b`).test(d.init));
      const derived =
        [...readers].some((r) => new RegExp(`\\b${r}\\s*\\(`).test(d.init)) ||
        (overText && stringMethod.test(d.init)) ||
        (overText && [...textHelpers].some((h) => new RegExp(`\\b${h}\\s*\\(`).test(d.init)));
      if (derived) bindings.add(d.name);
    }

    if (bindings.size + readers.size === before) break;
  }
  return { readers, bindings };
}

/** The subject a pin is about: the binding it reads, or `<inline>`. */
function subjectOf(expr, bindings, readers) {
  for (const name of [...bindings].sort((a, b) => b.length - a.length)) {
    if (new RegExp(`\\b${name}\\b`).test(expr)) return name;
  }
  for (const name of [...readers].sort((a, b) => b.length - a.length)) {
    if (new RegExp(`\\b${name}\\s*\\(`).test(expr)) return `<${name}()>`;
  }
  return "<inline>";
}

function scanTs(rel, raw, root) {
  const src = blankComments(raw);
  const starts = lineIndex(src);
  const fromDir = root ? dirname(join(root, rel)) : null;
  const { readers, bindings } = tsTextBindings(src, fromDir);
  const pins = [];

  const push = (index, subject, kind, detail) =>
    pins.push({ file: rel, line: lineOf(starts, index), subject, kind, detail });

  // `expect(<expr>)` + a text matcher.
  for (const m of src.matchAll(/\bexpect\s*\(/g)) {
    const open = m.index + m[0].length - 1;
    const close = matchDelimiter(src, open);
    if (close === -1) continue;
    const expr = src.slice(open + 1, close);
    if (expr.includes("JSON.parse")) continue;
    const chain = chainAfter(src, close);
    const matcher = TS_MATCHERS.find((name) => new RegExp(`\\.${name}\\s*\\(`).test(chain));
    if (!matcher) continue;
    const touchesText =
      TEXT_READERS.some((r) => expr.includes(`${r}(`)) ||
      initImportsFileText(expr, fromDir) ||
      [...bindings].some((b) => new RegExp(`\\b${b}\\b`).test(expr)) ||
      [...readers].some((r) => new RegExp(`\\b${r}\\s*\\(`).test(expr));
    if (!touchesText) continue;
    push(m.index, subjectOf(expr, bindings, readers), "expect-over-source-text", `.${matcher}()`);
  }

  // `assert(x.includes("..."))`, `assert.ok(...)`, `if (!x.includes(...)) throw`
  for (const m of src.matchAll(/\b(?:assert|assert\.ok|assert\.match|invariant)\s*\(/g)) {
    const open = m.index + m[0].length - 1;
    const close = matchDelimiter(src, open);
    if (close === -1) continue;
    const expr = src.slice(open + 1, close);
    if (expr.includes("JSON.parse")) continue;
    const touchesText =
      TEXT_READERS.some((r) => expr.includes(`${r}(`)) ||
      initImportsFileText(expr, fromDir) ||
      [...bindings].some((b) => new RegExp(`\\b${b}\\b`).test(expr));
    if (!touchesText) continue;
    push(m.index, subjectOf(expr, bindings, readers), "assert-over-source-text", "assert()");
  }

  // A bare guard over file text: `if (!source.includes("x")) throw new Error(...)`
  for (const m of src.matchAll(/\bif\s*\(/g)) {
    const open = m.index + m[0].length - 1;
    const close = matchDelimiter(src, open);
    if (close === -1) continue;
    const expr = src.slice(open + 1, close);
    if (!/\.(?:includes|match|test|indexOf)\s*\(/.test(expr)) continue;
    if (expr.includes("JSON.parse")) continue;
    if (![...bindings].some((b) => new RegExp(`\\b${b}\\b`).test(expr))) continue;
    const after = src.slice(close + 1, close + 200);
    if (!/\b(?:throw|process\.exit|fail\s*\()/.test(after)) continue;
    push(m.index, subjectOf(expr, bindings, readers), "guard-over-source-text", "throws on file text");
  }

  return pins;
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

const RUST_READS = (text) => text.includes("include_str!") || text.includes("read_to_string");

/**
 * Balanced-delimiter walk for RUST text that `blankRustComments` has already
 * been through. Returns the index of the matching closer, or -1.
 *
 * It deliberately does NOT track quotes, and that is the whole point. The
 * shared `matchDelimiter` treats `'` as opening a literal, which is true in
 * JavaScript and false in Rust: after blanking, every string, byte string, raw
 * string and char literal is gone, and the only `'` left in the text is a
 * LIFETIME or a loop label -- `&'static str`, `Vec<&'a [u8]>`, `'outer: loop`.
 * Quote-tracking those swallows everything up to the next lifetime, which is
 * usually thousands of lines away.
 *
 * Measured on this tree: `apps/osl-hub/src/broker.rs:9992` binds
 * `Rc::new(RefCell::new(Vec::<&'static str>::new()))`, and quote-tracking that
 * tick ran the scope of `ack_delete_ordering_after_restart_drain` from its real
 * end at line 10057 to line 14382 -- 4,325 lines of other people's functions,
 * whose locals then leaked in as "source text" bindings.
 */
function matchRustDelimiter(src, open) {
  const pairs = { "(": ")", "[": "]", "{": "}" };
  const closer = pairs[src[open]];
  if (!closer) return -1;
  let depth = 0;
  for (let i = open; i < src.length; i += 1) {
    const c = src[i];
    if (c === src[open]) depth += 1;
    else if (c === closer) {
      depth -= 1;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/** `let X = ...;` declarations inside one span, with their initialisers. */
function rustDeclarations(src, from, to, kinds = "let|const|static") {
  const out = [];
  const re = new RegExp(`\\b(?:${kinds})\\s+(?:mut\\s+)?([A-Za-z_][\\w]*)\\s*(?::[^=;{]{0,160})?=\\s*`, "g");
  re.lastIndex = from;
  let m;
  while ((m = re.exec(src)) && m.index < to) {
    const start = m.index + m[0].length;
    let end = start;
    let depth = 0;
    // No quote tracking, for the reason given on matchRustDelimiter: the input
    // is already blanked, so a `'` here is a lifetime and not a literal.
    for (; end < to; end += 1) {
      const c = src[end];
      if ("([{".includes(c)) depth += 1;
      else if (")]}".includes(c)) depth -= 1;
      else if (c === ";" && depth === 0) break;
    }
    out.push({ name: m[1], init: src.slice(start, end) });
  }
  return out;
}

/**
 * Every `fn` body in the file, with the set of locals inside it that hold
 * SOURCE TEXT.
 *
 * Scoped per function on purpose. `native_discord_overlay.rs` binds `hook` to
 * `include_str!` in one test and to something unrelated 800 lines away; a
 * file-wide name set would call the second one a pin because the first one is.
 * Bindings are resolved to a FIXED POINT within each scope, so
 * `let source = overlay_source(); let hook = function_body(source, ..)` is text
 * at both steps -- which is how every real pin in this tree is actually written.
 */
function rustScopes(src) {
  const readers = new Set();
  const bodies = [];
  for (const m of src.matchAll(/\bfn\s+([A-Za-z_][\w]*)\s*[(<]/g)) {
    const open = src.indexOf("{", m.index);
    if (open === -1) continue;
    // A trait method has no body: `fn value_of(&self) -> Option<String>;`.
    // Without this the next unrelated block became its "scope" and its
    // bindings leaked across half the file.
    const semi = src.indexOf(";", m.index);
    if (semi !== -1 && semi < open) continue;
    const close = matchRustDelimiter(src, open);
    if (close === -1) continue;
    bodies.push({ name: m[1], start: open, end: close });
    // A reader is a function that reads a file AND HANDS BACK ITS TEXT. A
    // function that reads a file and returns a parsed value has turned the
    // text into data; asserting on that data is not a pin.
    const signature = src.slice(m.index, open);
    const returnsText = /->\s*(?:&(?:'[a-z_]+\s+)?(?:str\b)|String\b|Cow<[^>]*str[^>]*>)/.test(signature);
    if (returnsText && RUST_READS(src.slice(open, close))) readers.add(m[1]);
  }

  const scopes = bodies.map((body) => {
    const decls = rustDeclarations(src, body.start, body.end);
    const bindings = new Set();
    for (let pass = 0; pass < 4; pass += 1) {
      const before = bindings.size;
      for (const d of decls) {
        if (bindings.has(d.name)) continue;
        if (d.init.includes("serde_json::from_str") || d.init.includes("from_slice")) continue;
        const readsFile =
          RUST_READS(d.init) ||
          [...readers].some((r) => new RegExp(`\\b${r}\\s*\\(`).test(d.init)) ||
          [...bindings].some((b) => new RegExp(`\\b${b}\\b`).test(d.init));
        if (readsFile) bindings.add(d.name);
      }
      if (bindings.size === before) break;
    }
    return { ...body, bindings };
  });

  // File-level `const NAME: &str = include_str!(...)` is in scope everywhere.
  //
  // `let` is deliberately NOT read here. It was, and it was a bug worth
  // recording: `let source = include_str!(..)` inside one test at line 22563 of
  // native_discord_adapter.rs made every `source` in that 28,000-line file a
  // pin, and the census over-reported it 14x. Locals are scoped; only `const`
  // and `static` are not.
  const fileLevel = new Set();
  for (const d of rustDeclarations(src, 0, src.length, "const|static")) {
    if (RUST_READS(d.init)) fileLevel.add(d.name);
  }
  return { scopes, readers, fileLevel };
}

function scanRust(rel, raw) {
  const src = blankRustComments(raw);
  const starts = lineIndex(src);
  const { scopes, readers, fileLevel } = rustScopes(src);
  const pins = [];

  for (const m of src.matchAll(/\b(assert|assert_eq|assert_ne|debug_assert)!\s*\(/g)) {
    const open = m.index + m[0].length - 1;
    const close = matchRustDelimiter(src, open);
    if (close === -1) continue;
    const expr = src.slice(open + 1, close);
    const usesTextOp = RUST_TEXT_OPS.some((op) => expr.includes(op)) || RUST_READS(expr);
    if (!usesTextOp) continue;

    const inScope = new Set(fileLevel);
    for (const scope of scopes) {
      if (m.index > scope.start && m.index < scope.end) for (const b of scope.bindings) inScope.add(b);
    }
    const inline = RUST_READS(expr);
    const named = [...inScope]
      .sort((a, b) => b.length - a.length)
      .find((b) => new RegExp(`\\b${b}\\b`).test(expr));
    const viaReader = [...readers].find((r) => new RegExp(`\\b${r}\\s*\\(`).test(expr));
    if (!inline && !named && !viaReader) continue;

    pins.push({
      file: rel,
      line: lineOf(starts, m.index),
      subject: named ?? (inline ? "<inline>" : `<${viaReader}()>`),
      kind: "rust-assert-over-source-text",
      detail: `${m[1]}!`,
    });
  }
  return pins;
}

// ---------------------------------------------------------------------------
// Census
// ---------------------------------------------------------------------------

export function collect(root) {
  const files = walk(root, ".", (rel) => {
    if (SKIP.test(rel)) return false;
    if (rel.endsWith(".rs")) return true;
    return isTsLike(rel) && (isTsTest(rel) || rel.startsWith("scripts/"));
  });

  const pins = [];
  const counts = { tsFiles: 0, rustFiles: 0 };
  for (const rel of files) {
    let text;
    try {
      text = read(root, rel);
    } catch {
      continue;
    }
    if (rel.endsWith(".rs")) {
      counts.rustFiles += 1;
      pins.push(...scanRust(rel, text));
    } else {
      counts.tsFiles += 1;
      pins.push(...scanTs(rel, text, root));
    }
  }

  // Assign the stable ordinal per (file, subject).
  const seen = new Map();
  for (const pin of pins.sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line)) {
    const key = `${pin.file}#${pin.subject}`;
    const n = seen.get(key) ?? 0;
    seen.set(key, n + 1);
    pin.id = `${pin.file}#${pin.subject}~${n}`;
    pin.key = key;
  }
  return { pins, counts, filesScanned: files.length };
}

// ---------------------------------------------------------------------------
// Baseline + the two-way ratchet. Lifted from scripts/ledger/all.mjs:71-173 so
// the two ratchets in this repository cannot drift apart in behaviour.
// ---------------------------------------------------------------------------

export function loadBaseline(path = PIN_BASELINE_PATH) {
  let doc;
  try {
    doc = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    return { ids: [], pins: null, problems: [`${BASELINE_REL}: unreadable (${error.message})`] };
  }
  const problems = [];
  const ids = Array.isArray(doc.ids) ? doc.ids : null;
  if (!ids) problems.push(`${BASELINE_REL}: missing "ids" array`);
  if (doc.schema !== "osl-pin-census-v1") {
    problems.push(`${BASELINE_REL}: schema is ${JSON.stringify(doc.schema ?? null)}, expected "osl-pin-census-v1"`);
  }
  if (typeof doc.pins !== "number") {
    problems.push(`${BASELINE_REL}: missing numeric "pins"`);
  } else if (ids && doc.pins !== ids.length) {
    problems.push(
      `${BASELINE_REL}: RATCHET -- "pins" is ${doc.pins} but "ids" lists ${ids.length}. ` +
        `The number and the list are the same fact stated twice on purpose; D-192 is what happens ` +
        `when only the number is written down.`,
    );
  }
  const dupes = (ids ?? []).filter((id, i) => (ids ?? []).indexOf(id) !== i);
  for (const d of new Set(dupes)) problems.push(`${BASELINE_REL}: duplicate id ${JSON.stringify(d)}`);
  return { ids: ids ?? [], pins: typeof doc.pins === "number" ? doc.pins : null, problems };
}

export function ratchet(livePins, baseline) {
  const liveIds = livePins.map((p) => p.id).sort();
  const base = [...baseline.ids].sort();
  const where = new Map(livePins.map((p) => [p.id, `${p.file}:${p.line}`]));
  const lines = [];

  const introduced = liveIds.filter((id) => !base.includes(id));
  const removed = base.filter((id) => !liveIds.includes(id));

  if (baseline.problems.length) {
    lines.push(`  RATCHET: ${BASELINE_REL} IS NOT USABLE.`);
    for (const p of baseline.problems) lines.push(`    - ${p}`);
    return { ok: false, lines, introduced, removed };
  }

  if (!introduced.length && !removed.length) {
    lines.push(`  RATCHET: the pin census is at its recorded baseline of ${base.length} (${BASELINE_REL}).`);
    lines.push(`    These pins are not banned and must not be deleted wholesale -- some are the right tool.`);
    lines.push(`    The number may only go DOWN, and a new pin must be a recorded decision.`);
    return { ok: true, lines, introduced, removed };
  }

  if (introduced.length && removed.length) {
    lines.push(
      `  RATCHET: the pin census DRIFTED -- ${removed.length} removed and ${introduced.length} added, ` +
        `so the count alone hid the swap (scripts/ledger/all.mjs:28).`,
    );
  } else if (introduced.length) {
    lines.push(`  RATCHET: the pin census REGRESSED -- baseline ${base.length}, now ${liveIds.length}.`);
    lines.push(`    A new source-text pin was written. Either replace it with a test that EXECUTES the`);
    lines.push(`    behaviour, or add its id to ${BASELINE_REL} deliberately and say why in "note".`);
  } else {
    lines.push(`  RATCHET: the pin census BASELINE IS STALE -- baseline ${base.length}, now ${liveIds.length}.`);
    lines.push(`    A pin was removed and ${BASELINE_REL} was not lowered in the same change.`);
    lines.push(`    A ratchet checked in only one direction rots into a floor nobody ever lowers.`);
  }

  if (introduced.length) {
    lines.push(`    NEW pins, absent from the baseline:`);
    for (const id of introduced) lines.push(`      + ${id}   ${where.get(id) ?? "?"}`);
  }
  if (removed.length) {
    lines.push(`    Pins in the baseline that no longer exist -- delete these ids from "ids":`);
    for (const id of removed) lines.push(`      - ${id}`);
  }
  lines.push(`    ${BASELINE_REL} must then read "pins": ${liveIds.length}`);
  return { ok: false, lines, introduced, removed };
}

/** Every pin with its file and line, so the list is readable rather than inferred. */
export function renderCensus(pins) {
  const byFile = new Map();
  for (const pin of pins) {
    if (!byFile.has(pin.file)) byFile.set(pin.file, []);
    byFile.get(pin.file).push(pin);
  }
  const out = [`  THE CENSUS -- ${pins.length} pin(s) across ${byFile.size} file(s):`];
  for (const [file, list] of [...byFile.entries()].sort()) {
    out.push(`    ${file}  (${list.length})`);
    for (const pin of list) out.push(`      ${file}:${pin.line}  ${pin.subject}  ${pin.kind} ${pin.detail}`);
  }
  return out;
}

export function main(argv = process.argv) {
  const root = repoRoot(argv);
  const { pins, counts, filesScanned } = collect(root);

  if (argv.includes("--write-baseline")) {
    const doc = {
      schema: "osl-pin-census-v1",
      ledger: "pins",
      note:
        "THE PIN CENSUS RATCHET. Every id below is an assertion that reads SOURCE TEXT rather than " +
        "executing behaviour (PLAN.md r4-3 item 3; audit-results/PLAN-AUDIT-method.md 3.1). This is NOT " +
        "an exception file and NOT a ban list -- some pins are the right tool, and none of them may be " +
        "deleted wholesale to make this number fall. It is a bound. node scripts/ledger/pins.mjs FAILS " +
        "if the live count is HIGHER (a new pin was written where behaviour should have been executed) " +
        "and also if it is LOWER (a pin was removed without lowering the baseline in the same change), " +
        "and it FAILS on the same count with different ids, because one removed and one added is exactly " +
        "how D-190 and D-221 hid. Run `node scripts/ledger/pins.mjs --list` for the file and line of every id.",
      measured: `node scripts/ledger/pins.mjs, ${new Date().toISOString().slice(0, 10)}`,
      pins: pins.length,
      ids: pins.map((p) => p.id).sort(),
    };
    writeFileSync(PIN_BASELINE_PATH, `${JSON.stringify(doc, null, 2)}\n`);
    console.log(`wrote ${BASELINE_REL}: ${pins.length} pins`);
    return { failed: false, live: pins, stale: [], text: "" };
  }

  const out = [];
  out.push(`LEDGER pins -- source-text pins vs behaviour, ledger 10`);
  out.push(`  files scanned: ${filesScanned} (${counts.tsFiles} ts/js, ${counts.rustFiles} rust)`);
  out.push(`  pins found: ${pins.length}`);

  const baseline = loadBaseline();
  const verdict = ratchet(pins, baseline);
  out.push(...verdict.lines);

  if (!verdict.ok || argv.includes("--list")) out.push(...renderCensus(pins));

  // The delta again, after the census, so a 3,000-line list cannot bury the
  // one thing a reader has to act on.
  if (!verdict.ok) {
    out.push(`  RATCHET, restated: +${verdict.introduced.length} new pin(s), -${verdict.removed.length} removed, live ${pins.length} vs baseline ${baseline.pins ?? "?"}.`);
  }

  out.push(verdict.ok ? "  RESULT: GREEN" : "  RESULT: RED");
  console.log(out.join("\n"));

  const failed = !verdict.ok;
  process.exitCode = failed ? 1 : 0;
  return { failed, live: pins, stale: [], text: out.join("\n"), introduced: verdict.introduced, removed: verdict.removed };
}

if (resolve(process.argv[1] ?? "") === resolve(fileURLToPath(import.meta.url))) main();
