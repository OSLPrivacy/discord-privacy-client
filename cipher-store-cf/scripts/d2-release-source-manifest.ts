/**
 * D2 release-source manifest DERIVATION.
 *
 * D-258. The pinned set used to be a hand-written 31-entry array. A list a
 * human maintains is omission-shaped: it is only ever wrong by being short, and
 * being short is silent. It cost this repo twice — ten source files and
 * migrations 0011-0017 were never inside it, so the `attachment-reserve.ts`
 * null-dereference fix changed the live fetch path without moving the digest by
 * a bit.
 *
 * The set is therefore no longer written down. It is derived from the file that
 * decides what actually deploys — `wrangler.toml`:
 *
 *   * `main` names the Worker entry point. Its top-level directory is a release
 *     root, walked whole.
 *   * every `migrations_dir` is a release root, walked whole. (Migrations are
 *     not bundled; they are applied to D1. They are release source anyway,
 *     because a Worker running against the wrong schema is the wrong release.)
 *   * `RELEASE_CONFIG_FILES` below adds the files that are release inputs
 *     without living under a root: the deploy config itself, the dependency
 *     manifest and its lockfile, and this deriver.
 *
 * A directory walk cannot omit a file that exists. What it CAN miss is a file
 * that ships from outside the roots, so `localModuleClosure` recomputes the
 * Worker's own relative-import graph from `main` and the contract test requires
 * that graph to be a subset of the derived set. Adding `lib/foo.ts` at the
 * project root and importing it from `src/index.ts` fails that check rather
 * than quietly shipping unpinned.
 *
 * This file is itself in `RELEASE_CONFIG_FILES`, so the derivation rule lives
 * inside the digest it derives: narrowing a root, dropping a config file, or
 * loosening the walk moves `D2_RELEASE_SOURCE_SHA256` and the contract test
 * goes red. The contract file that HOLDS the digest cannot be pinned by it —
 * a digest over its own declaration has no fixpoint — which is why the rule
 * lives here and only the value lives there.
 *
 * Residual, recorded rather than implied: this covers first-party source. It
 * does not pin `node_modules` bytes (the lockfile stands in for those), files
 * pulled in by a computed dynamic `import()`, or anything a deploy pipeline
 * injects outside `wrangler.toml`.
 */

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, posix } from "node:path";
import ts from "typescript";

/**
 * Release inputs that do not live under a walked root. `main` and
 * `migrations_dir` come from wrangler.toml and are never listed here.
 */
export const RELEASE_CONFIG_FILES = [
  "package-lock.json",
  "package.json",
  "scripts/d2-release-source-manifest.ts",
  "wrangler.toml",
] as const;

/**
 * A floor, not a count: it only has to be low enough that every legitimate
 * addition leaves it true, and high enough that a walk which silently returned
 * nothing (wrong root, unreadable directory, a "tidied" exclude) cannot pass.
 * A gate that cannot fail is decoration — this one fails when starved.
 */
export const RELEASE_SOURCE_FILE_FLOOR = 45;

export interface ReleaseRoots {
  /** Worker entry point, relative to the project root, POSIX separators. */
  main: string;
  /** Directories walked whole, relative to the project root. */
  roots: string[];
}

function fail(message: string): never {
  throw new Error(`D2 release source manifest: ${message}`);
}

function tomlStrings(toml: string, key: string): string[] {
  const values: string[] = [];
  for (const line of toml.split(/\r?\n/)) {
    const match = new RegExp(`^\\s*${key}\\s*=\\s*"([^"]+)"\\s*$`).exec(line);
    if (match) values.push(match[1]!);
  }
  return values;
}

/** Read the deploy inputs that decide which trees are release source. */
export function readReleaseRoots(projectRoot: string): ReleaseRoots {
  const toml = readFileSync(join(projectRoot, "wrangler.toml"), "utf8");
  const mains = tomlStrings(toml, "main");
  if (mains.length !== 1) {
    fail("wrangler.toml must declare exactly one `main` entry point");
  }
  const main = mains[0]!.replace(/\\/g, "/").replace(/^\.\//, "");
  const migrationDirs = tomlStrings(toml, "migrations_dir")
    .map((value) => value.replace(/\\/g, "/").replace(/^\.\//, ""));
  if (migrationDirs.length === 0) {
    fail("wrangler.toml declares no `migrations_dir`");
  }
  const mainRoot = main.split("/")[0]!;
  if (mainRoot === main) fail(`\`main\` (${main}) is not inside a directory`);
  const roots = [...new Set([mainRoot, ...migrationDirs])].sort();
  for (const root of roots) {
    if (!statSync(join(projectRoot, root)).isDirectory()) {
      fail(`release root ${root} is not a directory`);
    }
  }
  return { main, roots };
}

function walk(projectRoot: string, relative: string, out: string[]): string[] {
  const entries = readdirSync(join(projectRoot, relative), {
    withFileTypes: true,
  }).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  for (const entry of entries) {
    const child = posix.join(relative, entry.name);
    if (entry.isDirectory()) walk(projectRoot, child, out);
    else if (entry.isFile()) out.push(child);
  }
  return out;
}

/**
 * Every release-source path, sorted, with POSIX separators. No exclusions: a
 * derivation with an exclude list is a hand-written list again, one negation
 * further away from being read.
 */
export function deriveReleaseSourceFiles(projectRoot: string): string[] {
  const { roots } = readReleaseRoots(projectRoot);
  const walked: string[] = [];
  for (const root of roots) walk(projectRoot, root, walked);
  const files = [...new Set([...walked, ...RELEASE_CONFIG_FILES])].sort();
  if (files.length < RELEASE_SOURCE_FILE_FLOOR) {
    fail(
      `derived ${files.length} release files, below the floor of `
      + `${RELEASE_SOURCE_FILE_FLOOR}; the walk found less source than exists`,
    );
  }
  return files;
}

/**
 * Digest over (path, byte length, bytes) for each file in order. Unchanged from
 * the hand-listed era on purpose: only the membership rule moved, so a digest
 * recomputed over the old list still reproduces the old value, and the ledger
 * for the re-anchor can attribute the move to set expansion alone.
 */
export function releaseSourceManifestSha256(
  projectRoot: string,
  files: readonly string[],
): string {
  const manifest = createHash("sha256");
  for (const relative of files) {
    const bytes = readFileSync(join(projectRoot, relative));
    manifest.update(relative);
    manifest.update("\0");
    manifest.update(String(bytes.byteLength));
    manifest.update("\0");
    manifest.update(bytes);
  }
  return manifest.digest("hex");
}

function resolveLocal(projectRoot: string, relative: string): string | null {
  const candidates = [relative];
  if (relative.endsWith(".js")) candidates.push(`${relative.slice(0, -3)}.ts`);
  if (relative.endsWith(".mjs")) candidates.push(`${relative.slice(0, -4)}.mts`);
  if (!/\.[a-z]+$/.test(relative)) {
    candidates.push(`${relative}.ts`, `${relative}.js`, `${relative}/index.ts`);
  }
  for (const candidate of candidates) {
    try {
      if (statSync(join(projectRoot, candidate)).isFile()) return candidate;
    } catch {
      // not this candidate
    }
  }
  return null;
}

function relativeSpecifiers(projectRoot: string, relative: string): string[] {
  const source = ts.createSourceFile(
    relative,
    readFileSync(join(projectRoot, relative), "utf8"),
    ts.ScriptTarget.Latest,
    true,
    relative.endsWith(".ts") ? ts.ScriptKind.TS : ts.ScriptKind.JS,
  );
  const specifiers: string[] = [];
  const collect = (value: string): void => {
    if (value.startsWith(".")) specifiers.push(value);
  };
  const visit = (node: ts.Node): void => {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node))
      && node.moduleSpecifier
      && ts.isStringLiteral(node.moduleSpecifier)
    ) {
      collect(node.moduleSpecifier.text);
    }
    if (
      ts.isCallExpression(node)
      && node.expression.kind === ts.SyntaxKind.ImportKeyword
      && node.arguments[0]
      && ts.isStringLiteral(node.arguments[0])
    ) {
      collect((node.arguments[0] as ts.StringLiteral).text);
    }
    node.forEachChild(visit);
  };
  visit(source);
  return specifiers;
}

/**
 * Every first-party module reachable from `entry` by relative import, sorted.
 * Bare specifiers stop the walk: those are npm dependencies, and the lockfile
 * is what pins them.
 */
export function localModuleClosure(
  projectRoot: string,
  entry: string,
): string[] {
  const seen = new Set<string>();
  const queue = [entry];
  while (queue.length > 0) {
    const current = queue.pop()!;
    if (seen.has(current)) continue;
    seen.add(current);
    const directory = posix.dirname(current);
    for (const specifier of relativeSpecifiers(projectRoot, current)) {
      const target = resolveLocal(projectRoot, posix.join(directory, specifier));
      if (target === null) {
        fail(`${current} imports ${specifier}, which resolves to no file`);
      }
      if (!seen.has(target)) queue.push(target);
    }
  }
  return [...seen].sort();
}
