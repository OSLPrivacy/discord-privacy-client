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

import { createHash } from "node:crypto";
import { writeFileSync, mkdirSync, existsSync, readFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { repoRoot, walk, isTest, isDecl, LEDGER_DIR, inputProblems } from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";

export const CACHE = join(LEDGER_DIR, ".cache", "bundle-modules.json");
export const CACHE_SCHEMA = 2;

export function bundleInputFiles(root) {
  const files = [
    "apps/osl-hub-ui/package.json",
    "apps/osl-hub-ui/tsconfig.json",
    "apps/osl-hub-ui/vite.config.ts",
    "apps/osl-hub-ui/index.html",
    "apps/osl-hub-ui/whatsapp-qa.html",
    ...walk(
      root,
      "apps/osl-hub-ui/src",
      (r) => (r.endsWith(".ts") || r.endsWith(".css")) && !isTest(r) && !isDecl(r),
    ),
  ];
  return [...new Set(files)].filter((rel) => existsSync(join(root, rel))).sort();
}

export function bundleInputFingerprint(root) {
  const files = bundleInputFiles(root);
  const hash = createHash("sha256");
  for (const rel of files) {
    hash.update(rel);
    hash.update("\0");
    hash.update(readFileSync(join(root, rel)));
    hash.update("\0");
  }
  return { algorithm: "sha256", digest: hash.digest("hex"), files: files.length };
}

function cacheViolation(id, detail) {
  return {
    id,
    kind: "bundle-cache-not-trusted",
    detail,
    sites: ["scripts/ledger/.cache/bundle-modules.json:1"],
  };
}

export function readFreshBundleCache(root, fingerprint = bundleInputFingerprint(root)) {
  if (!existsSync(CACHE)) {
    return {
      doc: null,
      violation: cacheViolation(
        "bundle-cache-missing",
        "bundle cache is absent; default mode refuses to guess. Run with --refresh-cache or --no-cache so Rollup is asked for real.",
      ),
    };
  }
  let doc;
  try {
    doc = JSON.parse(readFileSync(CACHE, "utf8"));
  } catch (error) {
    return {
      doc: null,
      violation: cacheViolation(
        "bundle-cache-unreadable",
        `bundle cache is not valid JSON (${error.message}); run with --refresh-cache or --no-cache so Rollup is asked for real.`,
      ),
    };
  }
  if (doc.cacheSchema !== CACHE_SCHEMA) {
    return {
      doc,
      violation: cacheViolation(
        "bundle-cache-schema-stale",
        `bundle cache schema is ${JSON.stringify(doc.cacheSchema ?? null)}, expected ${CACHE_SCHEMA}; run with --refresh-cache or --no-cache so Rollup is asked for real.`,
      ),
    };
  }
  if (doc.root !== root) {
    return {
      doc,
      violation: cacheViolation(
        "bundle-cache-root-mismatch",
        `bundle cache was generated for ${JSON.stringify(doc.root)}, not ${JSON.stringify(root)}; run with --refresh-cache or --no-cache so Rollup is asked for real.`,
      ),
    };
  }
  if (doc.source?.algorithm !== fingerprint.algorithm || doc.source?.digest !== fingerprint.digest) {
    return {
      doc,
      violation: cacheViolation(
        "bundle-cache-source-stale",
        "bundle cache source fingerprint does not match the current UI source; refusing to report from stale Rollup module ids.",
      ),
    };
  }
  if (!Array.isArray(doc.modules) || !doc.edges || typeof doc.edges !== "object" || !Array.isArray(doc.entries)) {
    return {
      doc,
      violation: cacheViolation(
        "bundle-cache-incomplete",
        "bundle cache is missing modules, edges, or entries; run with --refresh-cache or --no-cache so Rollup is asked for real.",
      ),
    };
  }
  return { doc, violation: null };
}

export function throwBundleCacheViolation(violation) {
  const error = new Error(violation.detail);
  error.ledgerViolation = violation;
  throw error;
}

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
    configLoader: "runner",
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

export async function bundleSnapshot(root, { cache = true, refresh = false, writeCache = true } = {}) {
  const source = bundleInputFingerprint(root);
  if (cache && !refresh) {
    const { doc, violation } = readFreshBundleCache(root, source);
    if (!violation) return doc;
    throwBundleCacheViolation(violation);
  }
  const snapshot = await collectBundleModules(root);
  const doc = { cacheSchema: CACHE_SCHEMA, root, generatedAt: new Date().toISOString(), source, ...snapshot };
  if (writeCache) {
    mkdirSync(join(LEDGER_DIR, ".cache"), { recursive: true });
    writeFileSync(CACHE, JSON.stringify(doc, null, 2));
  }
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
  const input = inputProblems(root, ["apps/osl-hub-ui/src", "apps/osl-hub-ui/package.json", "apps/osl-hub-ui/vite.config.ts"]).map((p) => ({ ...p, kind: "ledger-input-missing" }));
  if (input.length) {
    return finish(report({ id: "bundle", title: "real bundle membership (rollup getModuleIds), ledger 7 of 7", violations: input }));
  }
  const noCache = argv.includes("--no-cache");
  const refresh = argv.includes("--refresh-cache");
  const cache = !noCache;
  const candidates = walk(
    root,
    "apps/osl-hub-ui/src",
    (r) => (r.endsWith(".ts") || r.endsWith(".css")) && !isTest(r) && !isDecl(r),
  );
  let snapshot;
  try {
    snapshot = await bundleSnapshot(root, { cache, refresh, writeCache: !noCache });
  } catch (error) {
    if (error.ledgerViolation) {
      return finish(
        report({
          id: "bundle",
          title: "real bundle membership (rollup getModuleIds), ledger 7 of 7",
          violations: [{ ...error.ledgerViolation, sites: ["scripts/ledger/.cache/bundle-modules.json:1"] }],
          stats: {
            "production sources": candidates.length,
            "bundle cache mode": refresh ? "refresh" : "read",
          },
        }),
      );
    }
    return finish(
      report({
        id: "bundle",
        title: "real bundle membership (rollup getModuleIds), ledger 7 of 7",
        violations: [
          {
            id: "rollup-build-failed",
            kind: "ledger-input-missing",
            detail: `Rollup/Vite module collection failed, so this ledger refuses to infer bundle membership: ${error.message}`,
            sites: ["apps/osl-hub-ui/vite.config.ts:1"],
          },
        ],
        stats: {
          "production sources": candidates.length,
          "bundle cache mode": noCache ? "bypass" : refresh ? "refresh" : "read",
        },
      }),
    );
  }
  const bundled = snapshot.modules;
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
        "bundle cache mode": noCache ? "bypass" : refresh ? "refresh" : "read",
        "bundle source files fingerprinted": snapshot.source?.files ?? "unknown",
      },
    }),
  );
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) {
  await main();
}
