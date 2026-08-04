#!/usr/bin/env node
// LEDGER 6 -- routes assigned vs dispatched.
//
// Route writes (`route = "x"`, `data-route="x"`) are only useful if the render
// dispatcher has a branch for the same value. This ledger checks the top-level
// Route surface and the nested OnboardingRoute surface separately.
//
//   node scripts/ledger/routes.mjs [--root=<dir>]

import { resolve } from "node:path";
import { repoRoot, read, uiSources, blankComments, lineIndex, lineOf, inputProblems } from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";

const REQUIRED = ["apps/osl-hub-ui/src/main.ts"];

function add(map, id, site) {
  if (!map.has(id)) map.set(id, []);
  map.get(id).push(site);
}

function matchingBrace(source, open) {
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    else if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return i;
    }
  }
  return -1;
}

function functionBody(source, name) {
  const match = new RegExp(`\\bfunction\\s+${name}\\s*\\([^)]*\\)\\s*(?::\\s*[^\\{]+)?\\{`).exec(source);
  if (!match) return null;
  const open = source.indexOf("{", match.index);
  const close = matchingBrace(source, open);
  return close < 0 ? null : { body: source.slice(open + 1, close), offset: open + 1 };
}

function collectUnion(source, starts, typeName) {
  const values = new Map();
  const re = new RegExp(`\\btype\\s+${typeName}\\s*=\\s*([^;]+);`);
  const m = re.exec(source);
  if (!m) return values;
  for (const lit of m[1].matchAll(/"([^"]+)"/g)) add(values, lit[1], `apps/osl-hub-ui/src/main.ts:${lineOf(starts, m.index + lit.index)}`);
  return values;
}

function collectAssigned(root, variable, attrNames) {
  const assigned = new Map();
  const unresolved = [];
  for (const rel of uiSources(root)) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    const assign = new RegExp(`(?<![\\w$.-])${variable}\\s*=\\s*(?![=>])([^;\\n]+)`, "g");
    for (const m of src.matchAll(assign)) {
      const lineStart = src.lastIndexOf("\n", m.index) + 1;
      const prefix = src.slice(lineStart, m.index);
      if (/\b(?:const|let|var)\s+$/.test(prefix)) continue;
      const lit = /^"([^"]+)"|'([^']+)'|`([^`$]+)`/.exec(m[1].trim());
      const site = `${rel}:${lineOf(starts, m.index)}`;
      if (lit) add(assigned, lit[1] ?? lit[2] ?? lit[3], site);
      else unresolved.push({ site, expr: m[1].trim().slice(0, 80) });
    }
    for (const attr of attrNames) {
      const data = new RegExp(`\\b${attr}=["']([^"'$<{]+)["']`, "g");
      for (const m of src.matchAll(data)) add(assigned, m[1], `${rel}:${lineOf(starts, m.index)}`);
    }
  }
  return { assigned, unresolved };
}

function collectDispatches(rel, src, variable) {
  const dispatched = new Map();
  const unresolved = [];
  const starts = lineIndex(src);
  const shadowed = [];
  for (const m of src.matchAll(/\bfunction\s+[A-Za-z_$][\w$]*\s*\(([^)]*)\)\s*(?::\s*[^{]+)?\{/g)) {
    if (!new RegExp(`\\b${variable}\\b`).test(m[1])) continue;
    const open = src.indexOf("{", m.index);
    const close = matchingBrace(src, open);
    if (close > open) shadowed.push([open, close]);
  }
  const isShadowed = (index) => shadowed.some(([start, end]) => index >= start && index <= end);
  const returnBranch = /(?:\bif\s*\(([\s\S]{0,700}?)\)\s*)?return\b/g;
  for (const m of src.matchAll(returnBranch)) {
    if (isShadowed(m.index)) continue;
    const condition = m[1] ?? "";
    const value = new RegExp(`\\b${variable}\\s*===\\s*"([^"]+)"`, "g");
    for (const c of condition.matchAll(value)) add(dispatched, c[1], `${rel}:${lineOf(starts, m.index + c.index)}`);
  }

  const switchBlock = new RegExp(`\\bswitch\\s*\\(\\s*${variable}\\s*\\)\\s*\\{`, "g");
  for (const m of src.matchAll(switchBlock)) {
    if (isShadowed(m.index)) continue;
    const open = src.indexOf("{", m.index);
    const close = matchingBrace(src, open);
    if (close < 0) {
      unresolved.push({ site: `${rel}:${lineOf(starts, m.index)}`, expr: `switch (${variable}) without a matching closing brace` });
      continue;
    }
    const body = src.slice(open + 1, close);
    for (const c of body.matchAll(/\bcase\s*"([^"]+)"\s*:/g)) add(dispatched, c[1], `${rel}:${lineOf(starts, open + 1 + c.index)}`);
  }
  return { dispatched, unresolved };
}

function analyse(prefix, declared, assigned, dispatched) {
  const violations = [];
  const live = new Set([...assigned.keys(), ...declared.keys()]);
  for (const route of [...live].sort()) {
    if (dispatched.has(route)) continue;
    violations.push({
      id: `${prefix}:${route}`,
      kind: "assigned-route-not-dispatched",
      detail: "route value can be assigned or declared but has no content dispatcher branch",
      sites: assigned.get(route) ?? declared.get(route) ?? [`apps/osl-hub-ui/src/main.ts:1`],
    });
  }
  for (const route of [...dispatched.keys()].sort()) {
    if (live.has(route)) continue;
    violations.push({
      id: `${prefix}:${route}`,
      kind: "dispatched-route-never-assigned",
      detail: "render dispatcher has a branch for a route value not declared or assigned by the scanned surface",
      sites: dispatched.get(route),
    });
  }
  return violations;
}

export function main(argv = process.argv) {
  const root = repoRoot(argv);
  const input = inputProblems(root, REQUIRED).map((p) => ({ ...p, kind: "ledger-input-missing" }));
  if (input.length) {
    return finish(report({ id: "routes", title: "routes assigned vs dispatched, ledger 6 of 7", violations: input }));
  }
  const mainRel = "apps/osl-hub-ui/src/main.ts";
  const mainSrc = blankComments(read(root, mainRel));
  const starts = lineIndex(mainSrc);
  const topAssigned = collectAssigned(root, "route", ["data-route"]);
  const onboardingAssigned = collectAssigned(root, "onboardingRoute", ["data-onboarding", "data-onboarding-action", "data-password-role-next"]);
  const routeDispatches = collectDispatches(mainRel, mainSrc, "route");
  const onboardingDispatches = collectDispatches(mainRel, mainSrc, "onboardingRoute");
  const violations = [
    ...analyse("route", collectUnion(mainSrc, starts, "Route"), topAssigned.assigned, routeDispatches.dispatched),
    ...analyse("onboarding", collectUnion(mainSrc, starts, "OnboardingRoute"), onboardingAssigned.assigned, onboardingDispatches.dispatched),
  ];
  for (const u of [...routeDispatches.unresolved, ...onboardingDispatches.unresolved]) {
    violations.push({
      id: `unresolved-route-dispatch:${u.site}`,
      kind: "unresolved-route-dispatch",
      detail: u.expr,
      sites: [u.site],
    });
  }
  return finish(report({
    id: "routes",
    title: "routes assigned vs dispatched, ledger 6 of 7",
    violations,
    stats: {
      "top-level routes assigned": topAssigned.assigned.size,
      "top-level routes dispatched": routeDispatches.dispatched.size,
      "onboarding routes assigned": onboardingAssigned.assigned.size,
      "onboarding routes dispatched": onboardingDispatches.dispatched.size,
      "non-literal route assignments": topAssigned.unresolved.length + onboardingAssigned.unresolved.length,
    },
  }));
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) main();
