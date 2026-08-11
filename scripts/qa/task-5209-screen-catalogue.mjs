#!/usr/bin/env node
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  HUB_DIALOG_SURFACES,
  HUB_ONBOARDING_STEPS,
  HUB_SCREEN_ROUTES,
  HUB_SETTINGS_SECTIONS,
  hubTabTravelSurfaceMarkup,
} from "../lib/hub-surface-fixtures.mjs";
import { sortedSurfaceNames, visibleItems } from "./task5209_surface_model.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const ORACLE_FILE = "contracts/task-5209-screen-oracle.json";
const BINDINGS_FILE = "apps/osl-hub-ui/src/catalogue/task-5209-screen-bindings.json";
const CATALOGUE_FILE = "crates/english-catalogue/catalogues/en-US.v1.json";
const MAIN_FILE = "apps/osl-hub-ui/src/main.ts";
const oracle = JSON.parse(readFileSync(path.join(ROOT, ORACLE_FILE), "utf8"));
const bindingsDocument = JSON.parse(readFileSync(path.join(ROOT, BINDINGS_FILE), "utf8"));
const catalogueDocument = JSON.parse(readFileSync(path.join(ROOT, CATALOGUE_FILE), "utf8"));
const mutantIndex = process.argv.indexOf("--mutant");
const mutant = mutantIndex >= 0 ? process.argv[mutantIndex + 1] : null;

function fail({ file = MAIN_FILE, runtimePath = "<inventory>", control = "<surface>", expected = "valid", actual = "invalid", meaning = "5209.inventory", action = "enumerate", message }) {
  console.error(`5209b file=${file} runtime_path=${runtimePath} control=${control} expected=${JSON.stringify(expected)} actual=${JSON.stringify(actual)} meaning=${meaning} action=${action}: ${message}`);
  process.exit(1);
}

function quotedUnion(source, name) {
  const match = new RegExp(`(?:export )?type ${name} = ([^;]+);`, "u").exec(source);
  if (!match) fail({ actual: `missing ${name}`, message: "source constructor inventory cannot be formed" });
  return [...match[1].matchAll(/"([a-z][a-z0-9-]*)"/gu)].map((entry) => entry[1]);
}

function sourceInventory() {
  const main = readFileSync(path.join(ROOT, MAIN_FILE), "utf8");
  const routes = quotedUnion(main, "Route").filter((route) => !["onboarding", "service", "settings"].includes(route)).map((route) => `route:${route}`);
  const onboarding = quotedUnion(main, "OnboardingRoute").map((route) => `onboarding:${route}`);
  const settings = quotedUnion(main, "SettingsSection").map((section) => `settings:${section}`);
  const dialogFiles = [MAIN_FILE, "apps/osl-hub-ui/src/chat-settings-dialog.ts", "apps/osl-hub-ui/src/whitelisting-settings-section.ts", "apps/osl-hub-ui/src/osl-profile-pane.ts"];
  const ids = new Set();
  for (const file of dialogFiles) {
    const source = readFileSync(path.join(ROOT, file), "utf8");
    for (const match of source.matchAll(/<dialog\b[^>]*\bid="([a-z0-9-]+)"/gu)) ids.add(match[1]);
  }
  const dialogNames = [...ids].map((id) => ({
    "people-dialog": "people-in-chat",
    "friends-dialog": "friends",
    "whitelist-roster-dialog": "whitelist-roster",
    "native-protect-friend-dialog": "native-protect-friend",
    "scrub-review-dialog": "scrub-review",
    "burn-dialog": "burn",
    "owned-confirmation-dialog": "owned-confirmation",
    "update-dialog": "update",
    "osl-chat-settings-dialog": "osl-chat-settings",
    "osl-profile-pane-dialog": "osl-profile-pane",
  })[id] ?? id).map((name) => `dialog:${name}`);
  const inventory = [...routes, ...onboarding, ...settings, "service:discord", "protected-sheets", ...dialogNames];
  if (mutant === "unregistered-surface") inventory.push("dialog:unregistered-security-surface");
  return [...new Set(inventory)].sort();
}

function packagedInventory() {
  return [
    ...HUB_SCREEN_ROUTES.map((name) => `route:${name}`),
    ...HUB_ONBOARDING_STEPS.map((name) => `onboarding:${name}`),
    ...HUB_SETTINGS_SECTIONS.map((name) => `settings:${name}`),
    "service:discord",
    "protected-sheets",
    ...HUB_DIALOG_SURFACES.map((name) => `dialog:${name}`),
  ].sort();
}

function compareInventory(name, expected, actual) {
  if (JSON.stringify(expected) === JSON.stringify(actual)) return;
  const missing = expected.filter((item) => !actual.includes(item));
  const extra = actual.filter((item) => !expected.includes(item));
  fail({ file: name === "source" ? MAIN_FILE : "scripts/lib/hub-surface-fixtures.mjs", runtimePath: missing[0] ?? extra[0] ?? "<inventory>", control: "surface-registration", expected: expected.join(","), actual: actual.join(","), meaning: "5209.surface-union", action: "register-and-reach", message: `inventory disagreement missing=${missing.join("|") || "none"} extra=${extra.join("|") || "none"}` });
}

if (oracle.provenance !== "independent-pre-migration-runtime-review" || oracle.derivedFromCatalogue !== false || mutant === "self-derived-oracle") {
  fail({ file: ORACLE_FILE, runtimePath: "oracle/provenance", control: "semantic-oracle", expected: "independent-pre-migration-runtime-review", actual: mutant === "self-derived-oracle" ? "derived-from-mutated-catalogue" : oracle.provenance, meaning: "5209.oracle-independence", action: "refuse-self-derived-oracle", message: "semantic oracle was derived from mutated catalogue values" });
}

const surfaces = await hubTabTravelSurfaceMarkup("task5209-acceptance");
if (mutant === "hard-coded-item") {
  const home = surfaces.find((surface) => surface.name === "route:home");
  home.markup += '<button id="hard-coded-security-control">Hard-coded security bypass</button>';
}
const source = sourceInventory();
const packaged = packagedInventory();
const runtime = sortedSurfaceNames(surfaces);
compareInventory("packaged", source, packaged);
compareInventory("runtime", source, runtime);
compareInventory("oracle", source, oracle.surfaces.map(({ name }) => name).sort());

const distAssets = path.join(ROOT, "apps/osl-hub-ui/dist/assets");
const packagedJs = readdirSync(distAssets).filter((name) => /^main-.*\.js$/u.test(name)).map((name) => readFileSync(path.join(distAssets, name), "utf8")).join("\n");
if (!packagedJs.includes("5209 missing packaged surface binding") || !packagedJs.includes("screen.route.home")) {
  fail({ file: "apps/osl-hub-ui/dist/assets/main-*.js", runtimePath: "packaged/index.html", control: "5205-screen-resolver", expected: "catalogue bindings and screen.route.home", actual: "missing packaged resolver", meaning: "5209.packaged-crawl", action: "load-packaged-route", message: "built package is not bound to the 5205 screen catalogue" });
}

const catalogue = new Map(catalogueDocument.entries.map(({ key, value }) => [key, value]));
const bindings = new Map(bindingsDocument.surfaces.map((surface) => [surface.name, new Map(surface.items.map((item) => [item.runtimePath, { ...item }]))]));
if (mutant === "swap-confirm-cancel") {
  const burn = oracle.surfaces.find((surface) => surface.name === "dialog:burn");
  const cancel = burn.items.find((item) => item.copy === "Cancel");
  const confirm = burn.items.find((item) => item.copy === "Burn now");
  [catalogue.set(cancel.key, catalogue.get(confirm.key)), catalogue.set(confirm.key, catalogue.get(cancel.key))];
}
if (mutant === "generic-collision") {
  const burn = oracle.surfaces.find((surface) => surface.name === "dialog:burn").items.find((item) => item.copy === "Burn now");
  const harmless = oracle.surfaces.find((surface) => surface.name === "dialog:update").items.find((item) => item.copy === "Not now");
  bindings.get("dialog:burn").get(burn.runtimePath).key = harmless.key;
}

let literals = 0;
let missingKeys = 0;
let checked = 0;
const captures = new Map(surfaces.map((surface) => [surface.name, visibleItems(surface.name, surface.markup)]));
for (const expectedSurface of oracle.surfaces) {
  const actualItems = captures.get(expectedSurface.name) ?? [];
  const surfaceBindings = bindings.get(expectedSurface.name) ?? new Map();
  if (actualItems.length !== expectedSurface.items.length) {
    const actual = actualItems[expectedSurface.items.length] ?? actualItems.at(-1);
    literals += Math.max(0, actualItems.length - expectedSurface.items.length);
    fail({ file: MAIN_FILE, runtimePath: actual?.runtimePath ?? expectedSurface.name, control: actual?.control ?? "surface-item-count", expected: `${expectedSurface.items.length} catalogued items`, actual: `${actualItems.length} items`, meaning: actual?.meaning ?? "5209.literal-free", action: actual?.action ?? "display", message: "rendered literal has no stable 5205 key" });
  }
  for (let index = 0; index < expectedSurface.items.length; index += 1) {
    const expected = expectedSurface.items[index];
    const actual = actualItems[index];
    const binding = surfaceBindings.get(actual.runtimePath);
    if (!binding) {
      literals += 1;
      fail({ file: MAIN_FILE, runtimePath: actual.runtimePath, control: actual.control, expected: expected.copy, actual: actual.copy, meaning: expected.meaning, action: expected.action, message: "rendered literal has no stable 5205 key" });
    }
    const resolved = catalogue.get(binding.key);
    if (resolved === undefined) {
      missingKeys += 1;
      fail({ file: CATALOGUE_FILE, runtimePath: actual.runtimePath, control: actual.control, expected: expected.copy, actual: "<missing key>", meaning: expected.meaning, action: expected.action, message: `missing key ${binding.key}` });
    }
    if (binding.meaning !== expected.meaning || binding.action !== expected.action || binding.class !== expected.class || actual.action !== expected.action || resolved !== expected.copy || actual.copy !== expected.copy) {
      fail({ file: CATALOGUE_FILE, runtimePath: actual.runtimePath, control: actual.control, expected: `${expected.copy} | ${expected.class} | ${expected.action}`, actual: `${resolved} | ${binding.class} | ${actual.action}`, meaning: `${expected.meaning}/${binding.meaning}`, action: `${expected.action}/${actual.action}`, message: "exact copy, meaning, class or invoked action mismatch" });
    }
    checked += 1;
  }
}

const selected = ["route:home", "onboarding:welcome", "dialog:burn"].map((surfaceName) => oracle.surfaces.find((surface) => surface.name === surfaceName).items[0]);
const changedCatalogue = new Map(catalogue);
for (const item of selected) changedCatalogue.set(item.key, `${catalogue.get(item.key)} [5209 change]`);
const changedSurfaces = new Set();
let changedItems = 0;
for (const surface of oracle.surfaces) {
  const surfaceBindings = bindings.get(surface.name);
  for (const item of surface.items) {
    const key = surfaceBindings.get(item.runtimePath).key;
    if (changedCatalogue.get(key) !== catalogue.get(key)) {
      changedItems += 1;
      changedSurfaces.add(surface.name);
    }
  }
}
if (changedItems !== 3 || changedSurfaces.size !== 3) fail({ expected: "3 items on 3 surfaces", actual: `${changedItems} items on ${changedSurfaces.size} surfaces`, meaning: "5209.three-key-propagation", action: "resolve", message: "three approved key changes did not affect exactly three real surfaces" });

console.log(`TASK5209 PASS source=${source.length} packaged=${packaged.length} runtime=${runtime.length} oracle=${oracle.surfaces.length} states=${oracle.surfaces.length} items=${checked} literals=${literals} missing_keys=${missingKeys} generic_collisions=0 swapped_controls=0 changed_keys=3 changed_items=${changedItems} changed_surfaces=${changedSurfaces.size}`);
