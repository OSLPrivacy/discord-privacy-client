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
 * So: JavaScript/TypeScript gets the real lexer, and EVERY other input keeps
 * the previous behaviour byte for byte. The language is taken from the path
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
  if (looksRust) return "other";
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
  if (known !== undefined && known !== AMBIGUOUS) return JS_EXTENSIONS.has(known) ? "js" : "other";
  return sniffSourceLanguage(source);
}

/**
 * Replace every comment and every regex-literal-looking span with spaces.
 *
 * Blanking rather than deleting keeps byte offsets stable, so line numbers
 * computed from the blanked text are the line numbers of the real file. Every
 * `file:line` this ledger prints comes from `lineOf` over blanked text.
 *
 * Pass `{ language: "js" | "other" }` to override the language; otherwise it is
 * resolved by `sourceLanguage` above.
 */
export function blankComments(source, options = {}) {
  const language = options.language ?? sourceLanguage(source);
  return language === "js" ? blankJsComments(source) : blankTextComments(source);
}

/**
 * The behaviour every ledger had before regex literals were understood, kept
 * verbatim for Rust/CSS/HTML/JSON/binary. It reads `//` and `/*` inside string
 * literals as comment starts, which is wrong -- but it is wrong in exactly the
 * way ledgers 3, 5 and 8 are currently calibrated for on the Rust corpus, and
 * changing THAT is a Rust-lexer lane (pins.mjs already has `blankRustComments`;
 * the other ledgers do not use it yet). Recorded, not silently altered.
 */
export function blankTextComments(source) {
  const blank = (s) => s.replace(/[^\n]/g, " ");
  return source
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/(^|[^:\\])\/\/[^\n]*/g, (m, p1) => p1 + blank(m.slice(p1.length)));
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
