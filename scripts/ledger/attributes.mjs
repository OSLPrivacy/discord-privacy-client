#!/usr/bin/env node
// LEDGER 1 -- interactive data-* attributes emitted vs event-bound selectors.
//
// This ledger is intentionally about controls, not CSS state. An emitted
// `data-*` counts when it is written on an interactive element or written
// programmatically via dataset/setAttribute. A bound attribute counts when an
// event path selects it through querySelector/closest/matches.
//
// Three classifier rules keep "control" and "state" apart. Each exists because
// the naive form produced a false accusation against shipped, working code:
//
//   RULE C (consumption). An event path consumes a `data-*` in three ways, not
//   one. It can select on it (`querySelector("[data-x]")`), it can read the
//   value off an element it already holds (`button.dataset.x`), or it can probe
//   the attribute directly (`hasAttribute("data-x")`). Only the first was
//   counted, so every *parameter* attribute -- the value a handler reads after
//   selecting on a sibling attribute -- was reported as an unbound control.
//   `data-whitelist-scope-key` (main.ts:4762) is read at main.ts:7068 inside the
//   `[data-whitelist-scope-remove]` click handler; `data-remove-person`
//   (ui-behavior.ts:331) is read at ui-behavior.ts:339. Reading an attribute is
//   consuming it.
//
//   RULE S (state, not control). A `data-*` on an interactive element whose tag
//   also carries a literal `id="..."` that shipped JS selects (`#id` inside a
//   querySelector/closest/matches literal) is a state or parameter attribute,
//   not this control's binding hook: the element is already reachable, and a
//   missing handler on it would surface as an unreachable *id*, never as an
//   unreachable attribute. That is the whole difference between
//   `data-lock-state` (main.ts:3789, on `#discord-qa-toggle-composer`, bound at
//   main.ts:7073, value read by CSS and by the QA prober) and
//   `data-account-recovery-phrase` (account-recovery.ts:106, on a `<form>` with
//   no id and no other handle -- a control the user can submit with nothing
//   listening). The rule never excuses an element that has no handle at all, so
//   the B0-02 class of defect still fires.
//
//   RULE E (emission through fragments). Attribute markup is routinely built in
//   an interpolated fragment variable and spliced into the tag later, e.g.
//   main.ts:1850 `const action = available ? \`data-onboarding-app-choice=...\``.
//   A tag-shaped scan cannot see those, so live markup read as "never emitted".
//   The bound-but-never-emitted direction therefore also counts attributes in
//   attribute position anywhere in the shipped source. It deliberately does not
//   count selector literals (`"[data-service]"`), so a listener waiting on
//   markup that no longer exists is still caught.
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

/** RULE E: `data-x` sitting in attribute position, not inside a selector literal. */
function attrsInAttributePosition(text) {
  return [...text.matchAll(/\b(data-[a-zA-Z0-9_-]+)(?=\s*=\s*["'`]|[\s>])/g)].map((m) => m[1]);
}

const SELECTOR_CALL = String.raw`\b(?:querySelector(?:All)?|closest|matches)\s*(?:<[^>(]*>)?\s*\(\s*`;

function matchingBrace(source, open) {
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    else if (source[i] === "}" && (depth -= 1) === 0) return i;
  }
  return -1;
}

/**
 * RULE C, third form: a handler does not have to be an inline closure. A named
 * function taking a DOM `*Event` is an event path wherever it is registered --
 * `submitPasswordRole(event: SubmitEvent)` (main.ts:8184) reads
 * `form.dataset.passwordRemove` 1,200 lines away from its `addEventListener`
 * (main.ts:7009), which no proximity window can reach.
 */
function eventHandlerRanges(src) {
  const ranges = [];
  for (const m of src.matchAll(/\bfunction\s+[A-Za-z_$][\w$]*\s*\(([^)]*)\)\s*(?::\s*[^{]+)?\{/g)) {
    if (!/:\s*[A-Za-z]*Event\b/.test(m[1])) continue;
    const open = src.indexOf("{", m.index + m[0].length - 1);
    const close = matchingBrace(src, open);
    if (close > open) ranges.push([open, close]);
  }
  return ranges;
}

/** RULE S: ids that shipped JS actually selects, so the element is reachable. */
function scanSelectedIds(root, files) {
  const ids = new Set();
  for (const rel of files) {
    const src = blankComments(read(root, rel));
    for (const m of src.matchAll(new RegExp(`${SELECTOR_CALL}(["'\`])#([A-Za-z][\\w-]*)\\1`, "g"))) ids.add(m[2]);
  }
  return ids;
}

function scanEmitted(root, files, selectedIds) {
  const interactive = new Map();
  const stateOnBoundElement = new Map();
  const all = new Map();
  const declared = new Map();
  const programmatic = new Map();
  for (const rel of files) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    for (const tag of src.matchAll(/<[^>]*\bdata-[^>]*>/g)) {
      for (const attr of attrsIn(tag[0])) add(all, attr, `${rel}:${lineOf(starts, tag.index)}`);
    }
    for (const attr of attrsInAttributePosition(src)) add(declared, attr, rel);
    for (const tag of src.matchAll(/<(button|input|form|select|textarea|dialog|a)\b[^>]*>/g)) {
      // RULE S: a literal id this codebase selects means the element is already
      // reachable; its remaining data-* are state/parameters, not the hook.
      const id = /\bid="([A-Za-z][\w-]*)"/.exec(tag[0]);
      const target = id && selectedIds.has(id[1]) ? stateOnBoundElement : interactive;
      for (const attr of attrsIn(tag[0])) add(target, attr, `${rel}:${lineOf(starts, tag.index)}`);
    }
    for (const m of src.matchAll(/setAttribute\s*\(\s*["'`](data-[a-zA-Z0-9_-]+)["'`]/g)) {
      add(programmatic, m[1], `${rel}:${lineOf(starts, m.index)}`);
    }
    for (const m of src.matchAll(/\.dataset\.([A-Za-z][A-Za-z0-9_]*)\s*=(?!=)/g)) {
      add(programmatic, `data-${kebab(m[1])}`, `${rel}:${lineOf(starts, m.index)}`);
    }
  }
  return { interactive, stateOnBoundElement, all, declared, programmatic };
}

function scanBound(root, files) {
  const bound = new Map();
  for (const rel of files) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    // An event path is required for every form: a read outside one proves nothing.
    const handlers = eventHandlerRanges(src);
    const inEventPath = (index) => /\baddEventListener\s*\(/
      .test(src.slice(Math.max(0, index - 600), Math.min(src.length, index + 900)))
      || handlers.some(([open, close]) => index > open && index < close);
    for (const m of src.matchAll(new RegExp(`${SELECTOR_CALL}(["'\`])([^"'\`$]*data-[^"'\`]*)\\1`, "g"))) {
      if (!inEventPath(m.index)) continue;
      for (const attr of attrsIn(m[2])) add(bound, attr, `${rel}:${lineOf(starts, m.index)}`);
    }
    // RULE C: reading the value off an element the handler already holds.
    for (const m of src.matchAll(/\.dataset\.([A-Za-z][A-Za-z0-9_]*)\b(?!\s*=(?!=))/g)) {
      if (!inEventPath(m.index)) continue;
      add(bound, `data-${kebab(m[1])}`, `${rel}:${lineOf(starts, m.index)}`);
    }
    // RULE C: probing the attribute directly.
    for (const m of src.matchAll(/\b(?:has|get|remove)Attribute\s*\(\s*["'`](data-[a-zA-Z0-9_-]+)["'`]/g)) {
      if (!inEventPath(m.index)) continue;
      add(bound, m[1], `${rel}:${lineOf(starts, m.index)}`);
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
  const selectedIds = scanSelectedIds(root, files);
  return { files, selectedIds, emitted: scanEmitted(root, files, selectedIds), bound: scanBound(root, files) };
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
    if (collected.emitted.interactive.has(attr)
      || collected.emitted.stateOnBoundElement.has(attr)
      || collected.emitted.all.has(attr)
      || collected.emitted.declared.has(attr)
      || collected.emitted.programmatic.has(attr)) continue;
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
      "state data-* on already-selected elements (rule S)": collected.emitted.stateOnBoundElement.size,
      "element ids shipped JS selects": collected.selectedIds.size,
      "programmatic data-* writes": collected.emitted.programmatic.size,
      "event-bound data-* attributes selected": collected.bound.size,
    },
  }));
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) await main();
