#!/usr/bin/env node
// LEDGER 4 -- frontend invoke("x") calls vs the Rust Tauri command registry.
//
// This catches the 46-phantom-command class directly: a shipped frontend module
// asks Tauri for a command string that no Rust invoke_handler registered.
//
// The better long-term shape is generated source: emit the frontend HubCommand
// union from the same Rust manifest/macro this script reads, and make an
// unregistered command a TypeScript compile error. This lane owns only
// scripts/ledger/, so the executable ratchet lives here.
//
//   node scripts/ledger/commands.mjs [--root=<dir>] [--no-cache]

import { resolve } from "node:path";
import { repoRoot, read, uiSources, blankComments, lineIndex, lineOf, inputProblems, stringConstants, resolveArg } from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";
import { bundleSnapshot } from "./bundle.mjs";
import { entryReachability, importedValuesByEntry, functionSpans, enclosingFunction, localFunctionEntryResolver } from "./acl-diff.mjs";
// ONE parser for the Rust command registry, shared with ledger 3. It lives in
// lib/ rather than here because this ledger already imports from acl-diff.mjs,
// so acl-diff.mjs importing back would be an ESM cycle that deadlocks on a
// top-level await. D-137 was two ledgers disagreeing about the same commands.
import { rustRegistry } from "./lib/rust-registry.mjs";

export { rustRegistry };

const REQUIRED = [
  "apps/osl-hub-ui/src/main.ts",
  "apps/osl-hub/src/main.rs",
  "apps/osl-hub/src/hub_command_surface.rs",
];

function add(map, id, site) {
  if (!map.has(id)) map.set(id, []);
  map.get(id).push(site);
}

function relFromSite(site) {
  return site.replace(/:\d+$/, "");
}

function bundleEvidence(sites, bundled) {
  const modules = [...new Set(sites.map(relFromSite))].sort();
  const outside = modules.filter((rel) => !bundled.has(rel));
  const inside = modules.filter((rel) => bundled.has(rel));
  return { modules, outside, inside };
}

export function frontendInvokes(root, snapshot = null) {
  const invokes = new Map();
  const unresolved = [];
  const deadBundled = [];
  const files = uiSources(root);
  const constants = stringConstants(files, (rel) => read(root, rel));
  const bundled = new Set(snapshot?.modules ?? []);
  const reachability = snapshot ? entryReachability(snapshot) : null;
  const importsByEntry = reachability ? importedValuesByEntry(root, reachability.byEntry) : new Map();
  for (const rel of files) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    const spans = snapshot ? functionSpans(src) : [];
    const entriesForFunction = snapshot
      ? localFunctionEntryResolver(rel, spans, src, reachability?.byModule ?? new Map(), importsByEntry)
      : null;
    const liveEntriesAt = (index) => entriesForFunction?.(enclosingFunction(spans, index)) ?? new Set();
    const addReachable = (command, index) => {
      const site = `${rel}:${lineOf(starts, index)}`;
      if (snapshot && bundled.has(rel) && liveEntriesAt(index).size === 0) {
        deadBundled.push({ command, site });
        return;
      }
      add(invokes, command, site);
    };
    for (const m of src.matchAll(/\binvoke\s*(?:<[^>(]*>)?\s*\(\s*([^,)]+)/g)) {
      const arg = m[1].trim();
      const site = `${rel}:${lineOf(starts, m.index)}`;
      const command = resolveArg(arg, constants);
      if (command.value) addReachable(command.value, m.index);
      else unresolved.push({ site, expr: arg.slice(0, 80) });
    }
  }
  return { invokes, unresolved, deadBundled };
}

export async function main(argv = process.argv) {
  const root = repoRoot(argv);
  const input = inputProblems(root, REQUIRED).map((p) => ({ ...p, kind: "ledger-input-missing" }));
  if (input.length) {
    return finish(report({ id: "commands", title: "frontend invokes vs Rust command registry, ledger 4 of 7", violations: input }));
  }
  const noCache = argv.includes("--no-cache");
  const refresh = argv.includes("--refresh-cache");
  let snapshot;
  try {
    snapshot = await bundleSnapshot(root, { cache: !noCache, refresh, writeCache: !noCache });
  } catch (error) {
    const violation = error.ledgerViolation
      ? { ...error.ledgerViolation, kind: "ledger-input-missing", sites: ["scripts/ledger/.cache/bundle-modules.json:1"] }
      : {
          id: "rollup-build-failed",
          kind: "ledger-input-missing",
          detail: `Rollup/Vite module collection failed, so this ledger refuses to infer command reachability: ${error.message}`,
          sites: ["apps/osl-hub-ui/vite.config.ts:1"],
        };
    return finish(report({
      id: "commands",
      title: "frontend invokes vs Rust command registry, ledger 4 of 7",
      violations: [violation],
      stats: {
        "bundle cache mode": noCache ? "bypass" : refresh ? "refresh" : "read",
      },
    }));
  }
  const bundled = new Set(snapshot.modules);
  const { invokes, unresolved, deadBundled } = frontendInvokes(root, snapshot);
  const { registry, problems } = rustRegistry(root);
  const violations = [
    ...input,
    ...problems.map((p) => ({ ...p, kind: "ledger-input-missing" })),
  ];
  for (const [command, sites] of [...invokes.entries()].sort()) {
    if (registry.has(command)) continue;
    const evidence = bundleEvidence(sites, bundled);
    const reachability = evidence.inside.length
      ? `bundled issuer(s): ${evidence.inside.join(", ")}`
      : `outside every bundle: ${evidence.outside.join(", ")}`;
    violations.push({
      id: command,
      kind: "frontend-invoke-not-registered",
      detail: `frontend invokes this Tauri command but no Rust invoke_handler registry contains it; ${reachability}`,
      sites,
    });
  }
  for (const u of unresolved) {
    const rel = relFromSite(u.site);
    const reachability = bundled.has(rel) ? `bundled issuer: ${rel}` : `outside every bundle: ${rel}`;
    violations.push({
      id: `unresolved-invoke:${u.site}`,
      kind: "unresolved-command-name",
      detail: `invoke() command is not a literal or known string constant: ${u.expr}; ${reachability}`,
      sites: [u.site],
    });
  }
  return finish(report({
    id: "commands",
    title: "frontend invokes vs Rust command registry, ledger 4 of 7",
    violations,
    stats: {
      "frontend source files scanned": uiSources(root).length,
      "distinct frontend commands invoked": invokes.size,
      "registered Rust commands": registry.size,
      "invoke() calls with a non-literal command (not analysable)": unresolved.length,
      "bundled invoke() call sites reclassified dead by function attribution": deadBundled.length,
      "modules rollup loaded (in-tree)": bundled.size,
      "bundle cache mode": noCache ? "bypass" : refresh ? "refresh" : "read",
    },
  }));
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) await main();
