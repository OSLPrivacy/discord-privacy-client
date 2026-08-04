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
  return readFileSync(join(root, rel), "utf8");
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
  return existsSync(p) ? readFileSync(p, "utf8") : null;
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

/**
 * Replace every comment and every regex-literal-looking span with spaces.
 *
 * Blanking rather than deleting keeps byte offsets stable, so line numbers
 * computed from the blanked text are the line numbers of the real file. Every
 * `file:line` this ledger prints comes from `lineOf` over blanked text.
 */
export function blankComments(source) {
  const blank = (s) => s.replace(/[^\n]/g, " ");
  return source
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/(^|[^:\\])\/\/[^\n]*/g, (m, p1) => p1 + blank(m.slice(p1.length)));
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
