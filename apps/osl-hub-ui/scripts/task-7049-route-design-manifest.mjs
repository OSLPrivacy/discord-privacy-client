/**
 * TASK 7049 — pair every route the shipping renderer declares with one D27
 * design page.  The page side is derived exclusively by TASK 6890; this file
 * never carries a second page catalogue.
 */
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { D27_FINAL_PAGE_LEAVES, D27_PACKAGE_SIZE_QUOTE, deriveShippingManifest } from "./task-6890-shipping-manifest.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const shippingRenderer = path.resolve(here, "../src/main.ts");

function fail(message) {
  throw new Error(`TASK 7049 D26(d): ${message}`);
}

function stringUnion(source, typeName) {
  const match = new RegExp(`\\b(?:export\\s+)?type\\s+${typeName}\\s*=\\s*([^;]+);`, "u").exec(source);
  if (!match) fail(`running shipping renderer does not declare type ${typeName}`);
  const values = [...match[1].matchAll(/"([^"]+)"/gu)].map((item) => item[1]);
  if (!values.length) fail(`running shipping renderer type ${typeName} has no literal routes`);
  return values;
}

/**
 * Reads the shipping renderer's own route domains.  This deliberately is not
 * a copied route list: a route added to the renderer type is discovered on the
 * next check and is UNREFERENCED until the D27 declaration names its page.
 */
export function enumerateShippingRoutes(rendererSource) {
  const topLevel = stringUnion(rendererSource, "Route");
  const onboarding = stringUnion(rendererSource, "OnboardingRoute");
  const settings = stringUnion(rendererSource, "SettingsSection");
  const routes = [
    ...topLevel.filter((route) => route !== "onboarding" && route !== "settings"),
    ...onboarding.map((state) => `onboarding/${state}`),
    ...settings.map((section) => `settings/${section}`),
  ];
  const duplicate = routes.find((route, index) => routes.indexOf(route) !== index);
  if (duplicate) fail(`running shipping renderer declares duplicate route ${JSON.stringify(duplicate)}`);
  return routes.sort((left, right) => left.localeCompare(right));
}

export async function readShippingRoutes(sourcePath = shippingRenderer) {
  return enumerateShippingRoutes(await readFile(sourcePath, "utf8"));
}

function pageAccounting(row) {
  if (row.kind === "routed") return "routed";
  if (row.kind === "state") return "state";
  if (row.kind === "not-built-yet") return "not-built-yet";
  if (row.kind !== "absent") fail(`${row.source}: unknown manifest kind ${JSON.stringify(row.kind)}`);
  if (/D10\(a\)/u.test(row.ruling ?? "")) return "deleted-D10(a)";
  if (/D10\(c\)/u.test(row.ruling ?? "")) return "excluded-D10(c)";
  fail(`${row.source}: absent page must be deleted by D10(a) or excluded by D10(c), with that ruling`);
}

/** Returns the five exhaustive D26 accounting buckets, and refuses overlap. */
export function accountD27Pages(manifest) {
  if (manifest.length !== D27_FINAL_PAGE_LEAVES) fail(`${D27_PACKAGE_SIZE_QUOTE}; manifest has ${manifest.length} pages`);
  const buckets = Object.fromEntries([
    "routed", "state", "deleted-D10(a)", "excluded-D10(c)", "not-built-yet",
  ].map((name) => [name, []]));
  const seen = new Set();
  for (const row of manifest) {
    if (!row.page || seen.has(row.page)) fail(`${row.source}: design page ${JSON.stringify(row.page)} is duplicated in D26 accounting`);
    seen.add(row.page);
    const bucket = pageAccounting(row);
    if (bucket === "state" && (!row.parent || !row.state || row.route !== null)) {
      fail(`${row.source}: state ${JSON.stringify(row.page)} must resolve to parent ${JSON.stringify(row.parent)} and state ${JSON.stringify(row.state)}, never a route of its own`);
    }
    if (bucket === "deleted-D10(a)" && !row.ruling) fail(`${row.source}: deleted page needs its D10(a) ruling`);
    if (bucket === "excluded-D10(c)" && !row.reason) fail(`${row.source}: D10(c) exclusion needs its reason`);
    buckets[bucket].push(row.page);
  }
  for (const pages of Object.values(buckets)) pages.sort((left, right) => left.localeCompare(right));
  return buckets;
}

/**
 * Pair runtime routes to the TASK 6890 page rows.  A single page must be the
 * target of precisely one route: mapping create and restore to one leaf is the
 * historical defect this makes impossible to hide.
 */
export function validateRouteDesignPairs(manifest, reachableRoutes) {
  const accounting = accountD27Pages(manifest);
  for (const row of manifest) {
    if (row.skippedRoutes != null) {
      const routes = Array.isArray(row.skippedRoutes) ? row.skippedRoutes : [row.skippedRoutes];
      fail(`${row.source}: routes may not be marked skipped; unreferenced route${routes.length === 1 ? "" : "s"}: ${routes.map((route) => JSON.stringify(route)).join(", ")}`);
    }
  }
  const byRoute = new Map();
  const byPage = new Map();
  for (const row of manifest.filter((candidate) => candidate.kind === "routed")) {
    if (typeof row.route !== "string" || !row.route.trim()) fail(`${row.source}: routed page ${JSON.stringify(row.page)} has no shipping route`);
    const routes = row.shippingRoutes ?? [row.route];
    if (!Array.isArray(routes) || !routes.length || routes.some((route) => typeof route !== "string" || !route.trim())) {
      fail(`${row.source}: routed page ${JSON.stringify(row.page)} has invalid shipping route pairing`);
    }
    const routesForPage = byPage.get(row.page) ?? [];
    for (const rawRoute of routes) {
      const route = rawRoute.trim();
      const previous = byRoute.get(route);
      if (previous) fail(`routes ${JSON.stringify(route)} name both design pages ${JSON.stringify(previous.page)} and ${JSON.stringify(row.page)}`);
      byRoute.set(route, row);
      routesForPage.push(route);
    }
    byPage.set(row.page, routesForPage);
  }
  const expected = [...new Set(reachableRoutes)].sort((left, right) => left.localeCompare(right));
  if (expected.length !== reachableRoutes.length) fail("running shipping route enumeration contains a duplicate");
  if (expected.length < accounting.routed.length) {
    const omitted = [...byRoute.keys()].filter((route) => !expected.includes(route));
    fail(`route count fell: running enumeration=${expected.length}, D26 routed pages=${accounting.routed.length}${omitted.length ? `; omitted route${omitted.length === 1 ? "" : "s"}: ${omitted.map((route) => JSON.stringify(route)).join(", ")}` : ""}`);
  }
  const missing = expected.filter((route) => !byRoute.has(route));
  if (missing.length) fail(`UNREFERENCED route${missing.length === 1 ? "" : "s"}: ${missing.map((route) => JSON.stringify(route)).join(", ")}`);
  for (const [page, routes] of byPage) if (routes.length !== 1) fail(`routes ${routes.map((route) => JSON.stringify(route)).join(", ")} point at design page ${JSON.stringify(page)}`);
  const extra = [...byRoute.keys()].filter((route) => !expected.includes(route));
  if (extra.length) fail(`manifest names unreachable route${extra.length === 1 ? "" : "s"}: ${extra.map((route) => JSON.stringify(route)).join(", ")}`);
  return {
    routePairs: expected.map((route) => ({ route, page: byRoute.get(route).page })),
    accounting,
  };
}

async function main() {
  // The explicit paths are only for disposable-package verification.  Release
  // invocation continues to read the checked-in D27 package and renderer.
  const designRoot = process.env.OSL_7049_DESIGN_ROOT;
  const rendererPath = process.env.OSL_7049_RENDERER;
  const [manifest, routes] = await Promise.all([
    deriveShippingManifest(designRoot || undefined),
    readShippingRoutes(rendererPath || undefined),
  ]);
  const result = validateRouteDesignPairs(manifest, routes);
  const counts = Object.fromEntries(Object.entries(result.accounting).map(([name, pages]) => [name, pages.length]));
  console.log(`TASK7049 ${D27_PACKAGE_SIZE_QUOTE} routes=${result.routePairs.length} pairs=${result.routePairs.length} unreferenced=0 accounting=${JSON.stringify(counts)}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
