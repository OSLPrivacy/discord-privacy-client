#!/usr/bin/env node
// LEDGER 5 -- Tauri events emitted vs listened.
//
// This is not a DOM-event ledger. It tracks the cross-webview/backend event
// bus: Rust `emit`/`emit_to`, frontend `emitTo`, and frontend `listen`.
//
//   node scripts/ledger/events.mjs [--root=<dir>]

import { resolve } from "node:path";
import {
  repoRoot,
  read,
  uiSources,
  rustSources,
  blankComments,
  lineIndex,
  lineOf,
  stringConstants,
  resolveArg,
  inputProblems,
} from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";

const REQUIRED = ["apps/osl-hub-ui/src/main.ts", "apps/osl-hub-ui/src/overlay.ts", "apps/osl-hub/src/main.rs"];

function add(map, id, site) {
  if (!map.has(id)) map.set(id, []);
  map.get(id).push(site);
}

function scanTs(root, files, constants) {
  const emitted = new Map();
  const listened = new Map();
  const unresolved = [];
  for (const rel of files) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    for (const m of src.matchAll(/\bemitTo\s*(?:<[^>(]*>)?\s*\(\s*[^,]+,\s*([^,)]+)/g)) {
      const site = `${rel}:${lineOf(starts, m.index)}`;
      const event = resolveArg(m[1], constants);
      if (event.value) add(emitted, event.value, site);
      else unresolved.push({ site, expr: m[1].trim() });
    }
    for (const m of src.matchAll(/\blisten\s*(?:<[^>(]*>)?\s*\(\s*([^,)]+)/g)) {
      const site = `${rel}:${lineOf(starts, m.index)}`;
      const event = resolveArg(m[1], constants);
      if (event.value) add(listened, event.value, site);
      else unresolved.push({ site, expr: m[1].trim() });
    }
  }
  return { emitted, listened, unresolved };
}

function scanRust(root, files, constants) {
  const emitted = new Map();
  const unresolved = [];
  for (const rel of files) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    const patterns = [
      /\.emit_to\s*\(\s*[^,]+,\s*([^,)]+)/g,
      /\b(?:window|app|app_handle)\.emit\s*\(\s*([^,)]+)/g,
    ];
    for (const re of patterns) {
      for (const m of src.matchAll(re)) {
        const site = `${rel}:${lineOf(starts, m.index)}`;
        const event = resolveArg(m[1], constants);
        if (event.value) add(emitted, event.value, site);
        else unresolved.push({ site, expr: m[1].trim() });
      }
    }
  }
  return { emitted, unresolved };
}

export function collect(root) {
  const ts = uiSources(root);
  const rs = rustSources(root);
  const constants = stringConstants([...ts, ...rs], (rel) => read(root, rel));
  const frontend = scanTs(root, ts, constants);
  const rust = scanRust(root, rs, constants);
  const emitted = new Map(rust.emitted);
  for (const [event, sites] of frontend.emitted) for (const site of sites) add(emitted, event, site);
  return {
    emitted,
    listened: frontend.listened,
    unresolved: [...frontend.unresolved, ...rust.unresolved],
    counts: { ts: ts.length, rs: rs.length },
  };
}

export function main(argv = process.argv) {
  const root = repoRoot(argv);
  const input = inputProblems(root, REQUIRED).map((p) => ({ ...p, kind: "ledger-input-missing" }));
  if (input.length) {
    return finish(report({ id: "events", title: "Tauri events emitted vs listened, ledger 5 of 7", violations: input }));
  }
  const collected = collect(root);
  const violations = [];

  for (const [event, sites] of [...collected.emitted.entries()].sort()) {
    if (collected.listened.has(event)) continue;
    violations.push({
      id: event,
      kind: "emitted-but-never-listened",
      detail: "event is emitted on the Tauri event bus but no shipped frontend listener subscribes to it",
      sites,
    });
  }
  for (const [event, sites] of [...collected.listened.entries()].sort()) {
    if (collected.emitted.has(event)) continue;
    violations.push({
      id: event,
      kind: "listened-but-never-emitted",
      detail: "frontend subscribes to this Tauri event but no shipped frontend/Rust emitter was found",
      sites,
    });
  }
  for (const u of collected.unresolved) {
    violations.push({
      id: `unresolved-event:${u.site}`,
      kind: "unresolved-event-name",
      detail: `event argument is not a literal or known string constant: ${u.expr}`,
      sites: [u.site],
    });
  }

  return finish(report({
    id: "events",
    title: "Tauri events emitted vs listened, ledger 5 of 7",
    violations,
    stats: {
      "frontend source files scanned": collected.counts.ts,
      "rust source files scanned": collected.counts.rs,
      "distinct events emitted": collected.emitted.size,
      "distinct events listened": collected.listened.size,
      "event calls with non-literal names": collected.unresolved.length,
    },
  }));
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) main();
