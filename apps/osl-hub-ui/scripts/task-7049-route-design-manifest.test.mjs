import assert from "node:assert/strict";
import test from "node:test";
import { D27_FINAL_PAGE_LEAVES } from "./task-6890-shipping-manifest.mjs";
import { accountD27Pages, enumerateShippingRoutes, validateRouteDesignPairs } from "./task-7049-route-design-manifest.mjs";

const source = `export type Route = "home" | "inbox" | "onboarding" | "settings";
type OnboardingRoute = "welcome" | "install";
type SettingsSection = "account" | "apps";`;

function manifest() {
  const routes = ["home", "inbox", "onboarding/welcome", "onboarding/install", "settings/account", "settings/apps"];
  const pages = routes.map((route, index) => ({ source: `page-${index}.dc.html`, page: `Page ${index}`, kind: "routed", route }));
  pages.push({ source: "state.dc.html", page: "Home Empty", kind: "state", route: null, parent: "Page 0", state: "empty" });
  pages.push({ source: "deleted.dc.html", page: "Onboarding Detected", kind: "absent", route: null, ruling: "D10(a)", reason: "Superseded by Onboarding Install" });
  pages.push({ source: "excluded.dc.html", page: "OSL Mail", kind: "absent", route: null, ruling: "D10(c)", reason: "Excluded mail surface" });
  while (pages.length < D27_FINAL_PAGE_LEAVES) pages.push({ source: `unbuilt-${pages.length}.dc.html`, page: `Unbuilt ${pages.length}`, kind: "not-built-yet", route: null });
  return pages;
}

test("TASK 7049 derives runtime routes and pairs all D27 pages exactly once", () => {
  const routes = enumerateShippingRoutes(source);
  assert.deepEqual(routes, ["home", "inbox", "onboarding/install", "onboarding/welcome", "settings/account", "settings/apps"]);
  const result = validateRouteDesignPairs(manifest(), routes);
  assert.equal(result.routePairs.length, routes.length);
  assert.deepEqual(Object.fromEntries(Object.entries(result.accounting).map(([kind, pages]) => [kind, pages.length])), {
    routed: 6, state: 1, "deleted-D10(a)": 1, "excluded-D10(c)": 1, "not-built-yet": 61,
  });
});

test("TASK 7049 names an unreferenced added shipping route", () => {
  const routes = enumerateShippingRoutes(source);
  const throwaway = [...routes, "throwaway"];
  assert.throws(() => validateRouteDesignPairs(manifest(), throwaway), /UNREFERENCED route: "throwaway"/);
  // Removing the throwaway route reuses the same page manifest and is green.
  assert.equal(validateRouteDesignPairs(manifest(), routes).routePairs.length, routes.length);
});

test("TASK 7049 names both routes and their shared page", () => {
  const rows = manifest();
  rows[0].shippingRoutes = ["home", "inbox"];
  rows[1].route = "aux";
  assert.throws(() => validateRouteDesignPairs(rows, enumerateShippingRoutes(source)), /routes "home", "inbox" point at design page "Page 0"/);
});

test("TASK 7049 rejects a state that claims a route and incomplete D26 accounting", () => {
  const stateRoute = manifest();
  stateRoute[6].route = "state-route";
  assert.throws(() => accountD27Pages(stateRoute), /never a route of its own/);
  assert.throws(() => accountD27Pages(manifest().slice(0, -1)), /70 design-page leaves; manifest has 69 pages/);
});
