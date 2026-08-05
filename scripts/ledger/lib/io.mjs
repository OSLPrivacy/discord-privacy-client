// Shared IO + text utilities for the Binding Ledger.
//
// Every ledger reads the tree through this module and nothing else, so a
// ledger can be pointed at a mutated copy of the tree (`--root`) without any
// of its analysis changing. That is what makes the starvation transcripts in
// scripts/ledger/transcripts/ real: the deletion happens in real source files,
// and the same code path that runs in CI reads them.

import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, relative, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

export const LEDGER_DIR = dirname(fileURLToPath(new URL("../lib/", import.meta.url)));

/** Repo root, overridable with --root=<dir> so starvation runs never touch the real tree. */
export function repoRoot(argv = process.argv) {
  const flag = argv.find((a) => a.startsWith("--root="));
  if (flag) return resolve(flag.slice("--root=".length));
  return resolve(LEDGER_DIR, "../..");
}

export function read(root, rel) {
  const text = readFileSync(join(root, rel), "utf8");
  rememberSourceLanguage(rel, text);
  return text;
}

export function inputProblems(root, rels) {
  const problems = [];
  for (const rel of rels) {
    const path = join(root, rel);
    if (!existsSync(path)) {
      problems.push({ id: `missing-input:${rel}`, detail: `required ledger input is missing: ${rel}`, sites: [`${rel}:1`] });
      continue;
    }
    const stat = statSync(path);
    if (stat.isFile() && readFileSync(path, "utf8").length === 0) {
      problems.push({ id: `empty-input:${rel}`, detail: `required ledger input is empty: ${rel}`, sites: [`${rel}:1`] });
    } else if (stat.isDirectory() && readdirSync(path).length === 0) {
      problems.push({ id: `empty-input:${rel}`, detail: `required ledger input directory is empty: ${rel}`, sites: [`${rel}:1`] });
    }
  }
  return problems;
}

export function tryRead(root, rel) {
  const p = join(root, rel);
  if (!existsSync(p)) return null;
  const text = readFileSync(p, "utf8");
  rememberSourceLanguage(rel, text);
  return text;
}

/** Recursively list files under `rel` matching `filter(relPath)`. */
export function walk(root, rel, filter) {
  const out = [];
  const base = join(root, rel);
  if (!existsSync(base)) return out;
  const stack = [base];
  while (stack.length) {
    const dir = stack.pop();
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name === "node_modules" || entry.name === "dist" || entry.name === ".git") continue;
        stack.push(full);
        continue;
      }
      if (!entry.isFile()) continue;
      const r = relative(root, full).split("\\").join("/");
      if (filter(r)) out.push(r);
    }
  }
  return out.sort();
}

export const isTest = (rel) => /\.test\.[cm]?[jt]sx?$/.test(rel) || rel.includes("/__tests__/");
export const isDecl = (rel) => rel.endsWith(".d.ts");

/** Production TypeScript under apps/osl-hub-ui/src. */
export function uiSources(root) {
  return walk(root, "apps/osl-hub-ui/src", (r) => r.endsWith(".ts") && !isTest(r) && !isDecl(r));
}

/** Every CSS file the UI owns, plus the HTML entry pages. */
export function uiStyleSources(root) {
  return walk(root, "apps/osl-hub-ui/src", (r) => r.endsWith(".css"));
}

export function uiHtmlPages(root) {
  return walk(root, "apps/osl-hub-ui", (r) => r.endsWith(".html") && !r.includes("/src/"));
}

export function rustSources(root) {
  return walk(root, "apps/osl-hub/src", (r) => r.endsWith(".rs"));
}

/* -------------------------------------------------------------------------
 * WHICH LANGUAGE IS THIS TEXT?
 *
 * `blankComments` is shared by ledgers 1-10 and it is NOT handed only
 * TypeScript. Instrumenting one full `node scripts/ledger/all.mjs --no-cache`
 * run showed 970 distinct sources flowing through it:
 *
 *     .ts 625   .rs 225   .mjs 98   .js 10   .css 7   .html 2   .cjs 1
 *     + one SVG and one PNG (read as utf8 by the bundle fingerprinter)
 *
 * That is why this module dispatches instead of simply becoming a JavaScript
 * lexer. A JS lexer turned loose on Rust re-creates, exactly, the second bug
 * the pin-census lane found while fixing its first: `&'static` is not a
 * character literal, and quote-tracking Rust swallows the rest of the file.
 * Rust also has raw strings (`r#"..."#`) that no JS lexer models, HTML has
 * `don't` in prose, and the PNG has whatever bytes it has.
 *
 * So this dispatches THREE ways. JavaScript/TypeScript gets the JS lexer; Rust
 * gets the Rust lexer that ledger 10 already had (`blankRustComments`, W2-7),
 * which is why `.rs` is no longer lumped in with everything else; and every
 * remaining input -- CSS, HTML, JSON, SVG, and one PNG -- keeps the previous
 * behaviour byte for byte. The language is taken from the path
 * the text was read from -- `read`/`tryRead` are the only doors into the tree
 * (see the header of this file), so the path is known for essentially every
 * call -- and only sniffed when a caller bypassed them with its own
 * `readFileSync` (`acl-diff.mjs` does this for the vendored @tauri-apps/api
 * `.js` files). The sniffer is positive-evidence-only and falls back to the
 * previous behaviour, so a source it cannot classify is never made worse.
 * ---------------------------------------------------------------------- */

const AMBIGUOUS = "\u0000ambiguous";
const JS_EXTENSIONS = new Set(["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"]);

/** len + head + tail: cheap, and `read` hands out a fresh string every call. */
function sourceFingerprint(source) {
  return `${source.length}\u0000${source.slice(0, 96)}\u0000${source.slice(-96)}`;
}

const sourceExtensions = new Map();

function rememberSourceLanguage(rel, text) {
  const ext = (/\.([A-Za-z0-9]+)$/.exec(rel)?.[1] ?? "").toLowerCase();
  const key = sourceFingerprint(text);
  const known = sourceExtensions.get(key);
  // Two different extensions with the same fingerprint: trust neither.
  if (known !== undefined && known !== ext) sourceExtensions.set(key, AMBIGUOUS);
  else sourceExtensions.set(key, ext);
}

/**
 * Positive evidence only. Anything unrecognised -- Rust, CSS, HTML, SVG,
 * binary -- returns "other", which is the pre-existing behaviour.
 */
export function sniffSourceLanguage(source) {
  const head = source.slice(0, 8192);
  if (head.includes("\u0000") || head.includes("\uFFFD")) return "other";
  if (/^[\s\uFEFF]*</.test(head)) return "other";
  // An ESM statement in the opening lines is decisive, and Rust has no such
  // form. It is checked FIRST because half a dozen TypeScript tests on this
  // tree quote Rust SOURCE inside string literals, and a Rust-shaped needle in
  // a TypeScript haystack must not win.
  const opening = source.split("\n", 60).join("\n");
  const esm =
    /^\s*import\s[\s\S]{0,200}?\sfrom\s+["']/m.test(opening) ||
    /^\s*import\s+["']/m.test(opening) ||
    /^\s*export\s+(?:const|let|var|function|class|default|async|type|interface|enum|\{|\*)/m.test(opening) ||
    /^\s*(?:const|let)\s+[\w{}[\], $]+\s*=\s*require\s*\(\s*["']/m.test(opening);
  if (esm) return "js";
  const looksRust =
    /^\s*(?:#!?\[|pub(?:\s*\([^)]*\))?\s+(?:fn|struct|enum|trait|mod|const|static|use|type)\b|impl\b|use\s+[a-z_]+(?:::|\s*;)|fn\s+[a-z_]|mod\s+[a-z_]+\s*[;{]|extern\s+crate\b)/m.test(source) ||
    /\blet\s+mut\s/.test(source) ||
    /\bimpl\s+[A-Za-z_<]/.test(source) ||
    /->\s*(?:Result|Option|Vec|String|bool|u\d|i\d|f\d|Self)\b/.test(source);
  // The sniffer already computed Rust evidence and used to throw it away. It is
  // only consulted when a caller bypassed `read`/`tryRead` with its own
  // `readFileSync`; on this tree that is ONE source and it is JavaScript, so
  // this branch is dead in production and is covered by a direct test that
  // feeds it all 225 real .rs files instead.
  if (looksRust) return "rust";
  const looksJs =
    /^\s*(?:import|export)\s/m.test(source) ||
    /\bfunction\s*[A-Za-z_$*(]/.test(source) ||
    /=>/.test(source) ||
    /\b(?:const|let|var)\s+[A-Za-z_$][\w$]*\s*=/.test(source) ||
    /\brequire\s*\(\s*["']/.test(source);
  return looksJs ? "js" : "other";
}

export function sourceLanguage(source) {
  const known = sourceExtensions.get(sourceFingerprint(source));
  if (known !== undefined && known !== AMBIGUOUS) {
    if (JS_EXTENSIONS.has(known)) return "js";
    return known === "rs" ? "rust" : "other";
  }
  return sniffSourceLanguage(source);
}

/**
 * Replace every comment and every regex-literal-looking span with spaces.
 *
 * Blanking rather than deleting keeps byte offsets stable, so line numbers
 * computed from the blanked text are the line numbers of the real file. Every
 * `file:line` this ledger prints comes from `lineOf` over blanked text.
 *
 * Pass `{ language: "js" | "rust" | "other" }` to override the language;
 * otherwise it is resolved by `sourceLanguage` above.
 */
export function blankComments(source, options = {}) {
  const language = options.language ?? sourceLanguage(source);
  if (language === "js") return blankJsComments(source);
  // Rust gets the real Rust lexer, with literals PRESERVED -- see
  // `blankRustComments` for why the shared path must not blank them.
  if (language === "rust") return blankRustComments(source, { blankLiterals: false });
  return blankTextComments(source);
}

/**
 * The behaviour every ledger had before regex literals were understood, kept
 * verbatim for CSS/HTML/JSON/SVG/binary. It reads `//` and `/*` inside string
 * literals as comment starts, which is wrong -- and for RUST it is no longer
 * used: `.rs` now goes to `blankRustComments`, which measurably recovered 828
 * characters of real code in 11 files that this function was destroying.
 *
 * It survives for the inputs where no better lexer exists here and where the
 * previous behaviour is what the ledgers are calibrated against. Extending the
 * Rust fix to CSS or HTML would be a different lane with its own measurement;
 * this one deliberately changed the `.rs` branch and nothing else.
 */
export function blankTextComments(source) {
  const blank = (s) => s.replace(/[^\n]/g, " ");
  return source
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/(^|[^:\\])\/\/[^\n]*/g, (m, p1) => p1 + blank(m.slice(p1.length)));
}

/**
 * THE ONE Rust lexer. Blank every Rust comment, keeping byte offsets so line
 * numbers stay true, and blank the whole of every literal too when asked.
 *
 * TWO CALLERS, TWO LITERAL POLICIES, ONE SCANNER. The scanner below is the only
 * code in this repo that knows where a Rust literal begins and ends; what the
 * two callers differ on is what to DO with the span it finds, which is a single
 * `if`. A second copy of this scanner is exactly the drift D-241 and D-266 were
 * -- and the dead `full_cleanup_manifest` before them -- so there is not one.
 *
 *   blankLiterals: true  (the DEFAULT, and what ledger 10's `scanRust` uses)
 *     The literal is blanked ENTIRELY, delimiters included. Ledger 10 walks
 *     BRACES to find `fn` scopes, so it needs no quote character left behind
 *     for the brace-matcher to resynchronise on. It never reads a literal's
 *     value.
 *
 *   blankLiterals: false (what the shared `blankComments` uses for `.rs`)
 *     The literal is PRESERVED and merely skipped over. Ledgers 3, 4, 5 and 8
 *     read Rust literal VALUES -- `stringConstants` resolves
 *     `const NAME: &str = "value"`, and the event, command, permission and
 *     persisted-key surfaces are all named by those strings. Blanking literals
 *     on that path would delete the very text those ledgers grade, which is a
 *     ledger WEAKENED, not fixed. Skipping rather than blanking still fixes the
 *     defect that matters there: a `//` or a `/*` INSIDE a literal is no longer
 *     read as the start of a comment, so it no longer eats the real code after
 *     it. This mirrors `blankJsComments`, which preserves JS string and
 *     template literals for the same reason.
 *
 * WHY THIS IS A LEXER AND NOT TWO REGEXES
 *
 * The regex version of this function (one regex for a block comment, then one
 * for everything after a `//` on a line -- and nothing else)
 * could not tell code from the inside of a string, and that is not a cosmetic
 * difference -- it is the same disease as D-242, where the claim gate's
 * tokenizer read a regex literal as a string and mis-parsed every literal after
 * it in 16 files. Measured here, it produced three distinct failures:
 *
 *   PHANTOM SCOPES. `apps/osl-hub/src/native_apps.rs:3469` contains the string
 *   "pub fn uia2_settle_plan(\n    budget_ms: u64,\n) -> Vec<u64> {" as a
 *   MUTANT INPUT to a refactor test. The old scanner read that as a function
 *   and brace-matched from the `{` INSIDE the string, inventing a 604-line
 *   scope (3469 -> 4073) whose end moved with any brace added anywhere after
 *   it. A pre-existing assert became newly "visible" purely because an
 *   unrelated lane's edit shifted that boundary -- so the census could
 *   attribute a pin to the wrong subject, and its count could move for reasons
 *   unrelated to any pin being written or deleted. In a TWO-WAY ratchet that
 *   makes both directions unreliable.
 *
 *   SWALLOWED CODE. A `//` inside a string literal -- `"https://..."`, or the
 *   `"// a comment added by a refactor"` mutants in this very tree -- blanked
 *   the rest of that REAL line, and a `/*` inside a string blanked everything
 *   up to the next `*` + `/` anywhere in the file. Assertions inside those
 *   spans were invisible to the census: a pin could be added, or deleted, with
 *   the ratchet silent in both directions.
 *
 *   MIS-ATTRIBUTION. A binding's name occurring inside a string literal matched
 *   `\bname\b` and named the subject of a pin that was not about it.
 *
 * Blanking the literal ENTIRELY (delimiters included) rather than only its
 * interior is deliberate: it leaves no quote characters behind, so the
 * brace-matcher cannot resynchronise on half a raw-string delimiter.
 *
 * Forms handled, all of which occur in this tree:
 *   //, ///, //!            line comments
 *   slash-star ... star-slash  block comments, NESTED, as Rust defines them
 *   "..."                   escapes, embedded braces and quotes
 *   r"...", r#"..."#, r##.. raw strings, arbitrary hash count, no escapes
 *   b"...", br#"..."#       byte strings; c"...", cr#"..."# C strings
 *   '{', '"', '\'', '\u{7}' char literals whose contents are braces or quotes
 *   b'{'                    byte chars
 *   'a, 'static, 'outer:    lifetimes and loop labels, which are NOT literals
 *                           and must not open one (this is the case a naive
 *                           quote-toggler gets wrong and then runs to EOF)
 */
export function blankRustComments(source, { blankLiterals = true } = {}) {
  const n = source.length;
  const out = source.split("");
  const blank = (from, to) => {
    for (let i = from; i < to && i < n; i += 1) if (out[i] !== "\n") out[i] = " ";
  };
  // What to do with a span the scanner has identified as a LITERAL. Comments
  // are always blanked; literals are blanked only when the caller asked for it.
  const literal = blankLiterals ? blank : () => {};
  // From the opening delimiter of an escaped literal to just past its close.
  const escaped = (start, delim) => {
    let i = start + 1;
    while (i < n) {
      const c = source[i];
      if (c === "\\") { i += 2; continue; }
      if (c === delim) return i + 1;
      i += 1;
    }
    return n; // unterminated: swallow to EOF rather than desynchronise
  };
  // `r`/`br`/`cr` + N hashes + `"` ... `"` + N hashes. No escapes inside.
  const rawFrom = (hashStart) => {
    let k = hashStart;
    let hashes = 0;
    while (k < n && source[k] === "#") { hashes += 1; k += 1; }
    if (source[k] !== '"') return -1;
    const term = `"${"#".repeat(hashes)}`;
    const end = source.indexOf(term, k + 1);
    return end === -1 ? n : end + term.length;
  };

  let i = 0;
  while (i < n) {
    const c = source[i];
    if (c === "/" && source[i + 1] === "/") {
      let j = i;
      while (j < n && source[j] !== "\n") j += 1;
      blank(i, j);
      i = j;
      continue;
    }
    if (c === "/" && source[i + 1] === "*") {
      let depth = 0;
      let j = i;
      while (j < n) {
        if (source[j] === "/" && source[j + 1] === "*") { depth += 1; j += 2; continue; }
        if (source[j] === "*" && source[j + 1] === "/") { depth -= 1; j += 2; if (depth === 0) break; continue; }
        j += 1;
      }
      blank(i, j);
      i = j;
      continue;
    }
    if (/[A-Za-z_]/.test(c)) {
      // Consume the whole identifier, so a `r` inside `for` or an `err` cannot
      // be mistaken for a raw-string prefix.
      let j = i;
      while (j < n && /\w/.test(source[j])) j += 1;
      const word = source.slice(i, j);
      if ((word === "r" || word === "br" || word === "rb" || word === "cr") && (source[j] === '"' || source[j] === "#")) {
        const end = rawFrom(j);
        if (end !== -1) { literal(i, end); i = end; continue; }
      }
      if ((word === "b" || word === "c") && source[j] === '"') {
        const end = escaped(j, '"');
        literal(i, end);
        i = end;
        continue;
      }
      if (word === "b" && source[j] === "'") {
        const end = escaped(j, "'");
        literal(i, end);
        i = end;
        continue;
      }
      i = j;
      continue;
    }
    if (c === '"') {
      const end = escaped(i, '"');
      literal(i, end);
      i = end;
      continue;
    }
    if (c === "'") {
      // `'ident` NOT followed by `'` is a lifetime or a loop label, not a char.
      if (/[A-Za-z_]/.test(source[i + 1] ?? "")) {
        let j = i + 1;
        while (j < n && /\w/.test(source[j])) j += 1;
        if (source[j] !== "'") { i = j; continue; }
      }
      const end = escaped(i, "'");
      literal(i, end);
      i = end;
      continue;
    }
    i += 1;
  }
  return out.join("");
}

/** Keywords after which a `/` opens a REGEX rather than dividing. */
const REGEX_AFTER_KEYWORD = new Set([
  "return", "typeof", "instanceof", "in", "of", "new", "delete", "void",
  "throw", "case", "do", "else", "yield", "await",
]);

/** Keywords whose `( ... )` is a statement head, so its `)` is a regex position. */
const CONTROL_KEYWORD = new Set(["if", "while", "for", "switch", "catch", "with"]);

const IDENT_START = /[A-Za-z_$\u00AA-\uFFFF]/;
const IDENT_PART = /[A-Za-z0-9_$\u00AA-\uFFFF]/;

/**
 * End index (exclusive) of the regex literal starting at `start`, or -1.
 *
 * The literal MUST terminate on the same line. A regex literal may not contain
 * an unescaped newline, so this is the property that makes the whole change
 * safe in the other direction: a `/` that was really division can never be
 * mistaken for a regex that runs away across the file. This is the same rule
 * the D-242 claim-gate lane settled on.
 */
export function regexLiteralEnd(source, start) {
  let i = start + 1;
  let inClass = false;
  let body = 0;
  while (i < source.length) {
    const c = source[i];
    if (c === "\n") return -1;
    if (c === "\\") {
      if (i + 1 >= source.length || source[i + 1] === "\n") return -1;
      i += 2;
      body += 1;
      continue;
    }
    if (inClass) {
      // Inside `[...]` a `/` does not terminate and a quote does not open a
      // string. `/[&<>"']/gu` is the literal D-242 was named after.
      if (c === "]") inClass = false;
      i += 1;
      body += 1;
      continue;
    }
    if (c === "[") {
      inClass = true;
      i += 1;
      body += 1;
      continue;
    }
    if (c === "/") {
      if (body === 0) return -1; // `//` is a comment; handled before we get here
      i += 1;
      while (i < source.length && /[A-Za-z]/.test(source[i])) i += 1; // flags
      return i;
    }
    i += 1;
    body += 1;
  }
  return -1;
}

/** End index (exclusive) of the `'`/`"` string starting at `start`. */
function stringLiteralEnd(source, start) {
  const quote = source[start];
  let i = start + 1;
  while (i < source.length) {
    const c = source[i];
    if (c === "\\") {
      i += 2; // covers \" \' \\ and the line continuation \<newline>
      continue;
    }
    if (c === quote) return i + 1;
    if (c === "\n") return i; // unterminated: resync at the newline rather than run away
    i += 1;
  }
  return source.length;
}

/**
 * Blank comments AND regex literals in JavaScript/TypeScript, leaving string
 * and template literals intact.
 *
 * String and template literals are PRESERVED, not blanked, because the ledgers
 * read their values (`stringConstants`, `resolveArg`, every `invoke("cmd")`
 * and `setAttribute("data-x")` match). They are still lexed, so that a `//`
 * inside `"https://..."` or a `/*` inside a CSS-in-a-template no longer eats
 * the real code that follows it.
 *
 * Regex literals are blanked whole: a `data-` or an `invoke(` inside `/.../`
 * is not a construct, it is a pattern, and counting it is a phantom.
 */
export function blankJsComments(source) {
  const out = source.split("");
  const n = source.length;
  const erase = (from, to) => {
    for (let k = from; k < to; k += 1) if (out[k] !== "\n") out[k] = " ";
  };

  let i = 0;
  // "value" means the previous significant token can end an expression, so a
  // following `/` is division. Anything else means `/` opens a regex.
  let prev = "operator";
  let prevWord = "";
  const parenIsControl = [];
  const templateBraceDepth = [];
  let braceDepth = 0;
  let inTemplate = false;

  // Hashbang. SKIPPED, not blanked: without this the lexer is handed a `/` in
  // a regex position and `/usr/` is a perfectly well-formed regex literal, but
  // blanking it would hide a line the previous tokenizer left visible, and no
  // ledger construct is ever weakened by leaving `#!/usr/bin/env node` alone.
  if (source.startsWith("#!")) {
    const nl = source.indexOf("\n");
    i = nl === -1 ? n : nl;
  }

  while (i < n) {
    if (inTemplate) {
      while (i < n) {
        const c = source[i];
        if (c === "\\") {
          i += 2;
          continue;
        }
        if (c === "`") {
          i += 1;
          inTemplate = false;
          prev = "value";
          prevWord = "";
          break;
        }
        if (c === "$" && source[i + 1] === "{") {
          i += 2;
          inTemplate = false;
          prev = "operator";
          prevWord = "";
          templateBraceDepth.push(braceDepth);
          break;
        }
        i += 1;
      }
      continue;
    }

    const c = source[i];

    if (c === "/" && source[i + 1] === "/") {
      const nl = source.indexOf("\n", i);
      const end = nl === -1 ? n : nl;
      erase(i, end);
      i = end;
      continue;
    }
    if (c === "/" && source[i + 1] === "*") {
      const close = source.indexOf("*/", i + 2);
      const end = close === -1 ? n : close + 2;
      erase(i, end);
      i = end;
      continue;
    }
    if (c === "/") {
      if (prev !== "value") {
        const end = regexLiteralEnd(source, i);
        if (end !== -1) {
          erase(i, end);
          i = end;
          prev = "value";
          prevWord = "";
          continue;
        }
      }
      i += source[i + 1] === "=" ? 2 : 1;
      prev = "operator";
      prevWord = "";
      continue;
    }
    if (c === '"' || c === "'") {
      i = stringLiteralEnd(source, i);
      prev = "value";
      prevWord = "";
      continue;
    }
    if (c === "`") {
      i += 1;
      inTemplate = true;
      continue;
    }
    if (IDENT_START.test(c)) {
      let j = i + 1;
      while (j < n && IDENT_PART.test(source[j])) j += 1;
      prevWord = source.slice(i, j);
      prev = REGEX_AFTER_KEYWORD.has(prevWord) ? "operator" : "value";
      i = j;
      continue;
    }
    if (c >= "0" && c <= "9") {
      let j = i + 1;
      while (j < n && /[0-9a-zA-Z_.]/.test(source[j])) j += 1;
      i = j;
      prev = "value";
      prevWord = "";
      continue;
    }
    if (c === "(") {
      parenIsControl.push(CONTROL_KEYWORD.has(prevWord));
      i += 1;
      prev = "operator";
      prevWord = "";
      continue;
    }
    if (c === ")") {
      // `if (ready) /re/.test(x)` is a regex; `(a + b) / c` is division.
      const control = parenIsControl.pop();
      i += 1;
      prev = control ? "operator" : "value";
      prevWord = "";
      continue;
    }
    if (c === "[") {
      i += 1;
      prev = "operator";
      prevWord = "";
      continue;
    }
    if (c === "]") {
      i += 1;
      prev = "value";
      prevWord = "";
      continue;
    }
    if (c === "{") {
      braceDepth += 1;
      i += 1;
      prev = "operator";
      prevWord = "";
      continue;
    }
    if (c === "}") {
      if (templateBraceDepth.length && braceDepth === templateBraceDepth[templateBraceDepth.length - 1]) {
        templateBraceDepth.pop();
        i += 1;
        inTemplate = true;
        continue;
      }
      braceDepth = Math.max(0, braceDepth - 1);
      i += 1;
      prev = "operator";
      prevWord = "";
      continue;
    }
    if ((c === "+" || c === "-") && source[i + 1] === c) {
      // `x++ / 2` divides; `++x` does not. Leaving `prev` alone gets both.
      i += 2;
      continue;
    }
    if (c === " " || c === "\t" || c === "\r" || c === "\n") {
      // Whitespace and comments must NOT clear prevWord, or `if /*c*/ (x)`
      // stops looking like a control head.
      i += 1;
      continue;
    }
    i += 1;
    prev = "operator";
    prevWord = "";
  }
  return out.join("");
}

export function lineIndex(source) {
  const starts = [0];
  for (let i = 0; i < source.length; i += 1) if (source[i] === "\n") starts.push(i + 1);
  return starts;
}

export function lineOf(starts, index) {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (starts[mid] <= index) lo = mid + 1;
    else hi = mid - 1;
  }
  return hi + 1;
}

/**
 * Resolve `const NAME = "value"` (TypeScript) and `const NAME: &str = "value"`
 * (Rust) across a set of files.
 *
 * This exists because both halves of the event surface name their events with
 * constants, and the two halves use DIFFERENT constant names for the SAME
 * string (`OVERLAY_CLOSED_EVENT` in apps/osl-hub/src/native_discord_overlay.rs:41
 * vs `NATIVE_DISCORD_OVERLAY_CLOSED_EVENT` in apps/osl-hub-ui/src/main.ts:243).
 * A ledger that only matched string literals at the call site would report a
 * clean set difference over an empty set -- the exact "search pattern rather
 * than the code" failure this task was told to guard against.
 */
export function stringConstants(files, readFile) {
  const map = new Map();
  for (const rel of files) {
    const src = blankComments(readFile(rel));
    const patterns = [
      /\bconst\s+([A-Za-z_$][\w$]*)\s*(?::\s*&'?[\w\s]*str\s*)?=\s*"((?:[^"\\]|\\.)*)"/g,
      /\bconst\s+([A-Za-z_$][\w$]*)\s*=\s*'((?:[^'\\]|\\.)*)'/g,
    ];
    for (const re of patterns) {
      for (const m of src.matchAll(re)) {
        const [, name, value] = m;
        if (map.has(name) && map.get(name).value !== value) {
          map.get(name).ambiguous = true;
          continue;
        }
        map.set(name, { value, file: rel, ambiguous: false });
      }
    }
    for (const obj of src.matchAll(/\bconst\s+([A-Za-z_$][\w$]*)\s*=\s*\{([\s\S]*?)\}\s*(?:as\s+const)?\s*;/g)) {
      const [, objectName, body] = obj;
      for (const prop of body.matchAll(/\b([A-Za-z_$][\w$]*)\s*:\s*"((?:[^"\\]|\\.)*)"|\b([A-Za-z_$][\w$]*)\s*:\s*'((?:[^'\\]|\\.)*)'/g)) {
        const name = `${objectName}.${prop[1] ?? prop[3]}`;
        const value = prop[2] ?? prop[4];
        if (map.has(name) && map.get(name).value !== value) {
          map.get(name).ambiguous = true;
          continue;
        }
        map.set(name, { value, file: rel, ambiguous: false });
      }
    }
  }
  return map;
}

/** Resolve a call argument that is either a string literal or a known constant. */
export function resolveArg(raw, constants) {
  const text = raw.trim();
  const lit = /^"((?:[^"\\]|\\.)*)"$|^'((?:[^'\\]|\\.)*)'$|^`([^`$\\]*)`$/.exec(text);
  if (lit) return { value: lit[1] ?? lit[2] ?? lit[3], kind: "literal" };
  if (/^[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)?$/.test(text)) {
    const c = constants.get(text);
    if (c && !c.ambiguous) return { value: c.value, kind: "constant", from: `${c.file}` };
    return { value: null, kind: "unresolved-identifier", text };
  }
  return { value: null, kind: "unresolved-expression", text };
}
