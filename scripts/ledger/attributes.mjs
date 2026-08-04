#!/usr/bin/env node
// LEDGER 1 -- interactive data-* attributes emitted vs event-bound selectors.
//
// This ledger is intentionally about controls, not CSS state. An emitted
// `data-*` counts when it is written on an interactive element or written
// programmatically via dataset/setAttribute. A bound attribute counts when an
// event path selects it through querySelector/closest/matches.
//
//   node scripts/ledger/attributes.mjs [--root=<dir>]

import { resolve } from "node:path";
import { repoRoot, read, uiSources, uiHtmlPages, blankComments, lineIndex, lineOf, inputProblems } from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";
import { bundleSnapshot, reachableFrom } from "./bundle.mjs";

const REQUIRED = ["apps/osl-hub-ui/src/main.ts", "apps/osl-hub-ui/index.html"];

function add(map, id, site) {
  if (!map.has(id)) map.set(id, []);
  map.get(id).push(site);
}

const kebab = (name) => name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

function attrsIn(text) {
  return [...text.matchAll(/\b(data-[a-zA-Z0-9_-]+)\b/g)].map((m) => m[1]);
}

function scanEmitted(root, files) {
  const interactive = new Map();
  const all = new Map();
  const programmatic = new Map();
  for (const rel of files) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    for (const tag of src.matchAll(/<[^>]*\bdata-[^>]*>/g)) {
      for (const attr of attrsIn(tag[0])) add(all, attr, `${rel}:${lineOf(starts, tag.index)}`);
    }
    for (const tag of src.matchAll(/<(button|input|form|select|textarea|dialog|a)\b[^>]*>/g)) {
      for (const attr of attrsIn(tag[0])) add(interactive, attr, `${rel}:${lineOf(starts, tag.index)}`);
    }
    for (const m of src.matchAll(/setAttribute\s*\(\s*["'`](data-[a-zA-Z0-9_-]+)["'`]/g)) {
      add(programmatic, m[1], `${rel}:${lineOf(starts, m.index)}`);
    }
    for (const m of src.matchAll(/\.dataset\.([A-Za-z][A-Za-z0-9_]*)\s*=/g)) {
      add(programmatic, `data-${kebab(m[1])}`, `${rel}:${lineOf(starts, m.index)}`);
    }
  }
  return { interactive, all, programmatic };
}

function scanBound(root, files) {
  const bound = new Map();
  for (const rel of files) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    for (const m of src.matchAll(/\b(?:querySelector(?:All)?|closest|matches)\s*(?:<[^>(]*>)?\s*\(\s*(["'`])([^"'`$]*data-[^"'`]*)\1/g)) {
      const site = `${rel}:${lineOf(starts, m.index)}`;
      const nearby = src.slice(Math.max(0, m.index - 600), Math.min(src.length, m.index + 900));
      if (!/\baddEventListener\s*\(/.test(nearby)) continue;
      for (const attr of attrsIn(m[2])) add(bound, attr, site);
    }
  }
  return bound;
}

const MAIN_ENTRIES = ["apps/osl-hub-ui/index.html", "apps/osl-hub-ui/whatsapp-qa.html"];
const EXTRA_MAIN_MODULES = ["apps/osl-hub-ui/src/signal-qa-main.ts"];

export async function collect(root, argv = process.argv) {
  const noCache = argv.includes("--no-cache");
  const snapshot = await bundleSnapshot(root, { cache: !noCache, writeCache: !noCache });
  const reachable = new Set([
    ...MAIN_ENTRIES.flatMap((entry) => [...reachableFrom(snapshot, entry)]),
    ...EXTRA_MAIN_MODULES,
    ...MAIN_ENTRIES,
  ]);
  const files = [...new Set([...uiSources(root), ...uiHtmlPages(root)].filter((rel) => reachable.has(rel)))].sort();
  return { files, emitted: scanEmitted(root, files), bound: scanBound(root, files) };
}

export async function main(argv = process.argv) {
  const root = repoRoot(argv);
  const input = inputProblems(root, REQUIRED).map((p) => ({ ...p, kind: "ledger-input-missing" }));
  if (input.length) {
    return finish(report({ id: "attributes", title: "interactive data-* emitted vs event-bound selectors, ledger 1 of 7", violations: input }));
  }
  let collected;
  try {
    collected = await collect(root, argv);
  } catch (error) {
    if (error.ledgerViolation) {
      return finish(report({ id: "attributes", title: "interactive data-* emitted vs event-bound selectors, ledger 1 of 7", violations: [error.ledgerViolation] }));
    }
    return finish(report({
      id: "attributes",
      title: "interactive data-* emitted vs event-bound selectors, ledger 1 of 7",
      violations: [{
        id: "bundle-reachability-unavailable",
        kind: "ledger-input-missing",
        detail: `bundle reachability could not be collected, so interactive attribute scope is not trustworthy: ${error.message}`,
        sites: ["apps/osl-hub-ui/vite.config.ts:1"],
      }],
    }));
  }
  const violations = [];
  for (const [attr, sites] of [...collected.emitted.interactive.entries()].sort()) {
    if (collected.bound.has(attr)) continue;
    violations.push({
      id: attr,
      kind: "emitted-interactive-data-attr-never-bound",
      detail: "interactive markup writes this data-* attribute, but no event-bound selector selects it",
      sites,
    });
  }
  for (const [attr, sites] of [...collected.bound.entries()].sort()) {
    if (collected.emitted.interactive.has(attr) || collected.emitted.all.has(attr) || collected.emitted.programmatic.has(attr)) continue;
    violations.push({
      id: attr,
      kind: "bound-data-attr-never-emitted",
      detail: "event code selects this data-* attribute, but the scanned interactive markup never writes it",
      sites,
    });
  }
  return finish(report({
    id: "attributes",
    title: "interactive data-* emitted vs event-bound selectors, ledger 1 of 7",
    violations,
    stats: {
      "frontend files scanned": collected.files.length,
      "data-* attributes emitted anywhere in markup": collected.emitted.all.size,
      "interactive data-* attributes emitted": collected.emitted.interactive.size,
      "programmatic data-* writes": collected.emitted.programmatic.size,
      "event-bound data-* attributes selected": collected.bound.size,
    },
  }));
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) await main();
