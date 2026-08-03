#!/usr/bin/env node
// LEDGER 7 -- real bundle membership, asked of the bundler.
//
// Ground truth is rollup's own `this.getModuleIds()` after `buildEnd`, not an
// import-graph walk. The earlier "~76 unreachable modules" figure was wrong in
// BOTH directions because esbuild erases type-only imports: `import type { X }`
// makes a module look reachable to a source-level walker while it is absent
// from the bundle, and `import { type X, y }` makes it look unreachable to a
// walker that filters on the `import type` prefix. Neither heuristic can be
// fixed; the question is only answerable by the tool that does the erasing.
//
// The set difference: every production source file under apps/osl-hub-ui/src
// vs every module id rollup actually pulled in. Violations run in BOTH
// directions -- a file outside the bundle that is not a recorded exception is a
// new orphan, and a recorded exception that IS in the bundle is a stale
// exception the ratchet forces you to delete.
//
//   node scripts/ledger/bundle.mjs [--root=<dir>] [--no-cache]

import { writeFileSync, mkdirSync, existsSync, readFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { repoRoot, walk, isTest, isDecl, LEDGER_DIR } from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";

export const CACHE = join(LEDGER_DIR, ".cache", "bundle-modules.json");

/** Run the real production build and ask rollup which modules it loaded. */
export async function collectBundleModules(root) {
  const uiRoot = join(root, "apps/osl-hub-ui");
  // vite is a devDependency of apps/osl-hub-ui, not of the repo root, so it
  // must be resolved from there rather than from this script's own directory.
  const { createRequire } = await import("node:module");
  const { pathToFileURL } = await import("node:url");
  const requireFromUi = createRequire(join(uiRoot, "package.json"));
  const vitePkg = requireFromUi.resolve("vite/package.json");
  const viteEntry = join(vitePkg, "..", "dist/node/index.js");
  const { build } = await import(pathToFileURL(viteEntry).href);
  const ids = new Set();
  const graph = [];
  await build({
    root: uiRoot,
    configFile: join(uiRoot, "vite.config.ts"),
    logLevel: "error",
    // write:false keeps this out of apps/osl-hub-ui/dist -- that directory is
    // embedded into the Rust binary at COMPILE time, and three other lanes are
    // building right now.
    build: { write: false, minify: false, reportCompressedSize: false },
    plugins: [
      {
        name: "osl-ledger-module-ids",
        buildEnd() {
          for (const id of this.getModuleIds()) {
            ids.add(id);
            const info = this.getModuleInfo(id);
            graph.push({
              id,
              imports: [...(info?.importedIds ?? []), ...(info?.dynamicallyImportedIds ?? [])],
              isEntry: Boolean(info?.isEntry),
            });
          }
        },
      },
    ],
  });
  const rel = (id) => {
    if (!id || id.startsWith("\0")) return null;
    const r = relative(root, id.split("?")[0]).split("\\").join("/");
    if (r.startsWith("..") || r.includes("node_modules/")) return null;
    return r;
  };
  const out = [];
  for (const id of ids) {
    const r = rel(id);
    if (r) out.push(r);
  }
  const edges = {};
  const entries = [];
  for (const node of graph) {
    const from = rel(node.id);
    if (!from) continue;
    if (node.isEntry) entries.push(from);
    edges[from] = [...new Set(node.imports.map(rel).filter(Boolean))];
  }
  return { modules: [...new Set(out)].sort(), edges, entries: [...new Set(entries)].sort() };
}

export async function bundleSnapshot(root, { cache = true } = {}) {
  if (cache && existsSync(CACHE)) {
    const doc = JSON.parse(readFileSync(CACHE, "utf8"));
    if (doc.root === root) return doc;
  }
  const snapshot = await collectBundleModules(root);
  mkdirSync(join(LEDGER_DIR, ".cache"), { recursive: true });
  const doc = { root, generatedAt: new Date().toISOString(), ...snapshot };
  writeFileSync(CACHE, JSON.stringify(doc, null, 2));
  return doc;
}

export async function bundleModules(root, opts) {
  return (await bundleSnapshot(root, opts)).modules;
}

/** Every module rollup can reach from `entry`, following real (post-erasure) edges. */
export function reachableFrom(snapshot, entry) {
  const seen = new Set();
  const stack = [entry];
  while (stack.length) {
    const cur = stack.pop();
    if (seen.has(cur)) continue;
    seen.add(cur);
    for (const next of snapshot.edges[cur] ?? []) stack.push(next);
  }
  return seen;
}

export function analyse(candidates, bundled) {
  const inBundle = new Set(bundled);
  const violations = [];
  for (const rel of candidates) {
    if (inBundle.has(rel)) continue;
    violations.push({
      id: rel,
      kind: "outside-the-bundle",
      detail: "production source file that rollup never loaded from any entry",
      sites: [`${rel}:1`],
    });
  }
  return violations;
}

export async function main(argv = process.argv) {
  const root = repoRoot(argv);
  const cache = !argv.includes("--no-cache");
  const candidates = walk(
    root,
    "apps/osl-hub-ui/src",
    (r) => (r.endsWith(".ts") || r.endsWith(".css")) && !isTest(r) && !isDecl(r),
  );
  const bundled = await bundleModules(root, { cache });
  const violations = analyse(candidates, bundled);
  return finish(
    report({
      id: "bundle",
      title: "real bundle membership (rollup getModuleIds), ledger 7 of 7",
      violations,
      stats: {
        "production sources": candidates.length,
        "modules rollup loaded (in-tree)": bundled.length,
        "outside the bundle": violations.length,
      },
    }),
  );
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) {
  await main();
}
