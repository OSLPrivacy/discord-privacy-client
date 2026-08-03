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
//   node scripts/ledger/commands.mjs [--root=<dir>]

import { resolve } from "node:path";
import { repoRoot, read, uiSources, blankComments, lineIndex, lineOf, inputProblems } from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";

const REQUIRED = [
  "apps/osl-hub-ui/src/main.ts",
  "apps/osl-hub/src/main.rs",
  "apps/osl-hub/src/hub_command_surface.rs",
];

function add(map, id, site) {
  if (!map.has(id)) map.set(id, []);
  map.get(id).push(site);
}

export function frontendInvokes(root) {
  const invokes = new Map();
  const unresolved = [];
  for (const rel of uiSources(root)) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    for (const m of src.matchAll(/\binvoke\s*(?:<[^>(]*>)?\s*\(\s*([^,)]+)/g)) {
      const arg = m[1].trim();
      const lit = /^"([^"]+)"$|^'([^']+)'$|^`([^`$]+)`$/.exec(arg);
      const site = `${rel}:${lineOf(starts, m.index)}`;
      if (lit) add(invokes, lit[1] ?? lit[2] ?? lit[3], site);
      else unresolved.push({ site, expr: arg.slice(0, 80) });
    }
  }
  return { invokes, unresolved };
}

function collectIdentifiers(body, rel, fullSource, offset = 0) {
  const starts = lineIndex(fullSource);
  const out = new Map();
  for (const m of body.matchAll(/\b([a-z][a-z0-9_]+)\b/g)) {
    const name = m[1];
    if (["macro_rules", "callback", "tauri", "generate_handler", "cfg", "feature"].includes(name)) continue;
    add(out, name, `${rel}:${lineOf(starts, offset + m.index)}`);
  }
  return out;
}

export function rustRegistry(root) {
  const registry = new Map();
  const problems = [];
  const surfaceRel = "apps/osl-hub/src/hub_command_surface.rs";
  const mainRel = "apps/osl-hub/src/main.rs";
  const surface = blankComments(read(root, surfaceRel));
  const main = blankComments(read(root, mainRel));

  const macroStart = surface.indexOf("macro_rules! hub_tauri_commands");
  const macroEnd = surface.indexOf("macro_rules! hub_tauri_command_names", macroStart);
  if (macroStart < 0 || macroEnd <= macroStart) {
    problems.push({ id: "missing-input:hub_tauri_commands", detail: "hub_tauri_commands registry macro anchor is missing or moved", sites: [`${surfaceRel}:1`] });
  } else {
    const body = surface.slice(macroStart, macroEnd);
    for (const [name, sites] of collectIdentifiers(body, surfaceRel, surface, macroStart)) {
      for (const site of sites) add(registry, name, site);
    }
  }

  const marker = "invoke_handler(tauri::generate_handler![";
  let literalLists = 0;
  for (let cursor = main.indexOf(marker); cursor >= 0; cursor = main.indexOf(marker, cursor + 1)) {
    const listStart = cursor + marker.length;
    const end = main.indexOf("]);", listStart);
    if (end < 0) continue;
    literalLists += 1;
    const body = main.slice(listStart, end);
    for (const [name, sites] of collectIdentifiers(body, mainRel, main, listStart)) {
      for (const site of sites) add(registry, name, site);
    }
  }
  if (literalLists === 0) {
    problems.push({ id: "missing-input:literal-generate-handler", detail: "no literal tauri::generate_handler list was found in main.rs; signal/alternate registries would be invisible", sites: [`${mainRel}:1`] });
  }
  if (registry.size === 0) {
    problems.push({ id: "empty-input:command-registry", detail: "Rust command registry extraction returned zero commands", sites: [`${surfaceRel}:1`, `${mainRel}:1`] });
  }
  return { registry, problems };
}

export function main(argv = process.argv) {
  const root = repoRoot(argv);
  const input = inputProblems(root, REQUIRED).map((p) => ({ ...p, kind: "ledger-input-missing" }));
  if (input.length) {
    return finish(report({ id: "commands", title: "frontend invokes vs Rust command registry, ledger 4 of 7", violations: input }));
  }
  const { invokes, unresolved } = frontendInvokes(root);
  const { registry, problems } = rustRegistry(root);
  const violations = [
    ...input,
    ...problems.map((p) => ({ ...p, kind: "ledger-input-missing" })),
  ];
  for (const [command, sites] of [...invokes.entries()].sort()) {
    if (registry.has(command)) continue;
    violations.push({
      id: command,
      kind: "frontend-invoke-not-registered",
      detail: "frontend invokes this Tauri command but no Rust invoke_handler registry contains it",
      sites,
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
    },
  }));
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) main();
