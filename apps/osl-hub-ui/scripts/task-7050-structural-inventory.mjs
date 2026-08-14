/**
 * TASK 7050 — compare the controls people can reach, before any geometry work.
 *
 * This deliberately accepts inventories rather than markup diffs.  An
 * inventory has only a stable control name, its spoken label, its destination,
 * and its position.  That keeps this gate concerned with structure alone.
 */
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { deriveShippingManifest, finalDesignDirectory } from "./task-6890-shipping-manifest.mjs";
import { readShippingRoutes } from "./task-7049-route-design-manifest.mjs";
import { comparePageText } from "./task-7051-demo-content.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));

function quoted(value) {
  return JSON.stringify(value);
}

function routeName(route) {
  return `build route ${quoted(route)}`;
}

function pageName(page) {
  return `design page ${quoted(page)}`;
}

function controlName(control) {
  return quoted(control.label || control.name);
}

function invalidInventoryFinding(page, route, side, detail) {
  return `${pageName(page)}: ${routeName(route)} cannot compare because the ${side} inventory is unavailable${detail ? ` (${detail})` : ""}.`;
}

/** Normalise an externally collected structural inventory and reject ambiguity. */
export function normaliseInventory(inventory, side, page, route) {
  if (!Array.isArray(inventory)) {
    return { controls: [], findings: [invalidInventoryFinding(page, route, side)] };
  }
  const controls = [];
  const names = new Set();
  const findings = [];
  for (const [index, raw] of inventory.entries()) {
    if (!raw || typeof raw !== "object" || typeof raw.name !== "string" || !raw.name.trim() || typeof raw.label !== "string" || !raw.label.trim()) {
      findings.push(invalidInventoryFinding(page, route, side, `invalid control at position ${index + 1}`));
      continue;
    }
    const name = raw.name.trim();
    if (names.has(name)) {
      findings.push(invalidInventoryFinding(page, route, side, `duplicate control ${quoted(name)}`));
      continue;
    }
    names.add(name);
    controls.push({ name, label: raw.label.trim(), destination: typeof raw.destination === "string" ? raw.destination.trim() : "" });
  }
  return { controls, findings };
}

/**
 * Compare one route to the design page that the 7049 pairing names.  Every
 * finding is a person-actionable sentence, never source markup or a picture.
 */
export function compareRouteInventory({ page, route, designInventory, buildInventory }) {
  const design = normaliseInventory(designInventory, "design", page, route);
  const build = normaliseInventory(buildInventory, "build", page, route);
  const findings = [...design.findings, ...build.findings];
  if (findings.length) return { ok: false, findings };

  const designByName = new Map(design.controls.map((control) => [control.name, control]));
  const buildByName = new Map(build.controls.map((control) => [control.name, control]));

  for (const control of design.controls) {
    if (!buildByName.has(control.name)) {
      findings.push(`${pageName(page)}: design has control ${controlName(control)} and ${routeName(route)} lacks it.`);
    }
  }
  for (const control of build.controls) {
    if (!designByName.has(control.name)) {
      findings.push(`${pageName(page)}: ${routeName(route)} has control ${controlName(control)} and design lacks it.`);
    }
  }

  for (const control of design.controls) {
    const built = buildByName.get(control.name);
    if (!built) continue;
    if (control.label !== built.label) {
      findings.push(`${pageName(page)}: control ${quoted(control.name)} has a different label — design says ${quoted(control.label)} and ${routeName(route)} says ${quoted(built.label)}.`);
    }
    if (control.destination !== built.destination) {
      findings.push(`${pageName(page)}: control ${quoted(control.name)} reaches a different destination — design reaches ${quoted(control.destination)} and ${routeName(route)} reaches ${quoted(built.destination)}.`);
    }
  }

  const commonDesignOrder = design.controls.filter((control) => buildByName.has(control.name)).map((control) => control.name);
  const commonBuildOrder = build.controls.filter((control) => designByName.has(control.name)).map((control) => control.name);
  if (commonDesignOrder.length > 1 && commonDesignOrder.join("\u0000") !== commonBuildOrder.join("\u0000")) {
    const first = commonDesignOrder.find((name, index) => name !== commonBuildOrder[index]) ?? commonDesignOrder[0];
    const second = commonDesignOrder.find((name) => name !== first && commonBuildOrder.indexOf(name) < commonBuildOrder.indexOf(first)) ?? commonBuildOrder[0];
    findings.push(`${pageName(page)}: controls ${quoted(designByName.get(first).label)} and ${quoted(designByName.get(second).label)} are in a different order on ${routeName(route)} than design.`);
  }
  return { ok: findings.length === 0, findings };
}

/**
 * Build route groups from the same manifest fields 7049 uses.  More than one
 * route for a page stays visible here so this checker can describe the
 * one-page-versus-two defect rather than hiding it behind a pairing exception.
 */
export function pairManifestRoutes(manifest, reachableRoutes) {
  const findings = [];
  if (!Array.isArray(manifest)) return { groups: [], findings: reachableRoutes.map((route) => invalidInventoryFinding("<manifest unavailable>", route, "manifest pairing")) };
  const seen = new Map();
  const groups = [];
  for (const row of manifest) {
    if (row?.kind !== "routed") continue;
    const routes = row.shippingRoutes ?? (typeof row.route === "string" ? [row.route] : []);
    if (!row.page || !routes.length) continue;
    const group = { page: row.page, routes: routes.map((route) => String(route).trim()).filter(Boolean) };
    for (const route of group.routes) {
      const previous = seen.get(route);
      if (previous) findings.push(`${pageName(row.page)}: ${routeName(route)} has ambiguous manifest pairing with ${pageName(previous)}.`);
      seen.set(route, row.page);
    }
    groups.push(group);
  }
  for (const route of reachableRoutes) {
    if (!seen.has(route)) findings.push(`${pageName("<manifest unavailable>")}: ${routeName(route)} has no design-page pairing.`);
  }
  for (const route of seen.keys()) {
    if (!reachableRoutes.includes(route)) findings.push(`${pageName(seen.get(route))}: ${routeName(route)} is named by the manifest but is not reachable in the build.`);
  }
  return { groups, findings };
}

/** Compare all paired inventories, including the explicitly forbidden split-page case. */
export function auditStructuralInventories({ manifest, routes, designInventories, buildInventories, designPages, buildPages }) {
  const pairing = pairManifestRoutes(manifest, routes);
  const findings = [...pairing.findings];
  for (const group of pairing.groups) {
    const designInventory = designInventories?.[group.page];
    if (group.routes.length !== 1) {
      const labels = normaliseInventory(designInventory, "design", group.page, group.routes[0]).controls.map((control) => quoted(control.label));
      const offered = labels.length ? labels.join(" and ") : "no readable controls";
      findings.push(`${pageName(group.page)} has one page offering ${offered}; build routes ${group.routes.map(quoted).join(" and ")} reach them as ${group.routes.length} routes.`);
      continue;
    }
    const route = group.routes[0];
    findings.push(...compareRouteInventory({
      page: group.page,
      route,
      designInventory,
      buildInventory: buildInventories?.[route],
    }).findings);
    // These are optional until the runtime page-text adapter is supplied, but
    // when either side carries captured markup we refuse a partial comparison.
    const designMarkup = designPages?.[group.page];
    const buildMarkup = buildPages?.[route];
    if (designMarkup !== undefined || buildMarkup !== undefined) {
      if (typeof designMarkup !== "string" || typeof buildMarkup !== "string") {
        findings.push(invalidInventoryFinding(group.page, route, typeof designMarkup !== "string" ? "design text" : "build text"));
      } else {
        findings.push(...comparePageText({ page: group.page, route, designMarkup, buildMarkup }).findings);
      }
    }
  }
  return { ok: findings.length === 0, findings };
}

function report(result) {
  if (result.ok) {
    console.log("TASK7050 STRUCTURAL PASS controls agree control-for-control");
    return;
  }
  for (const finding of result.findings) console.error(`TASK7050 STRUCTURAL DIFFERENCE: ${finding}`);
}

async function readFixture(file) {
  try {
    return JSON.parse(await readFile(file, "utf8"));
  } catch (error) {
    throw new Error(`fixture inventory cannot be read: ${error.message}`);
  }
}

async function main() {
  const index = process.argv.indexOf("--fixture");
  if (index !== -1) {
    const file = process.argv[index + 1];
    if (!file) throw new Error("--fixture needs an inventory JSON file");
    const fixture = await readFixture(path.resolve(process.cwd(), file));
    const result = auditStructuralInventories(fixture);
    report(result);
    if (!result.ok) process.exitCode = 1;
    return;
  }

  // The ordinary command never invents a design or build inventory.  This
  // lane intentionally has no D27 directory, so it reports that starvation
  // against an actual route rather than returning a false green result.
  const routes = await readShippingRoutes();
  try {
    await deriveShippingManifest(finalDesignDirectory);
  } catch (error) {
    const result = { ok: false, findings: [invalidInventoryFinding("<manifest unavailable>", routes[0] ?? "<no route>", "manifest pairing", error.message)] };
    report(result);
    process.exitCode = 1;
    return;
  }
  const result = { ok: false, findings: routes.map((route) => invalidInventoryFinding("<manifest unavailable>", route, "build", "no shipped runtime inventory adapter was supplied")) };
  report(result);
  process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`TASK7050 STRUCTURAL DIFFERENCE: ${error.message}`);
    process.exitCode = 1;
  });
}
