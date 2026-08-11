#!/usr/bin/env node
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
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
const mutantIndex = process.argv.indexOf("--mutant");
const mutant = mutantIndex >= 0 ? process.argv[mutantIndex + 1] : null;
const MUTANTS = [
  "routed-hard-coded-sentence",
  "modal-hard-coded-button",
  "hard-coded-hover-title-reason",
  "unregistered-reachable-screen",
  "swap-confirm-cancel",
  "generic-destructive-harmless-label",
  "cross-wired-security-reason",
];
let disposableRoot = null;
let disposal = 0;
let workingRoot = ROOT;

function fail({ file = MAIN_FILE, surface, runtimePath = "<inventory>", control = "<surface>", expected = "valid", actual = "invalid", meaning = "5209.inventory", meaningClass = "inventory", action = "enumerate", message }) {
  const exactSurface = surface ?? (runtimePath.includes("/") ? runtimePath.slice(0, runtimePath.indexOf("/")) : runtimePath);
  console.error(`5209b file=${file} surface=${exactSurface} runtime_path=${runtimePath} control=${control} expected_copy=${JSON.stringify(expected)} actual_copy=${JSON.stringify(actual)} meaning_class=${meaningClass} invoked_action=${action} expected=${JSON.stringify(expected)} actual=${JSON.stringify(actual)} meaning=${meaning} action=${action}: ${message}`);
  discardMutantCopy();
  if (mutant) console.error(`TASK5209B disposal=${disposal} build_discarded=${disposal} temp_remaining=0 poisoned_oracle=regenerated`);
  process.exit(1);
}

function replaceOnce(file, before, after) {
  const source = readFileSync(file, "utf8");
  if (!source.includes(before)) throw new Error(`5209b fixture mutation anchor missing in ${file}`);
  writeFileSync(file, source.replace(before, after));
}

function makeMutantCopy(name) {
  if (!MUTANTS.includes(name)) throw new Error(`unknown 5209b mutant ${name}`);
  const tempParent = process.env.OSL_5209B_TMP_ROOT ?? os.tmpdir();
  const root = mkdtempSync(path.join(tempParent, `osl-5209b-${name}-`));
  mkdirSync(path.join(root, "apps/osl-hub-ui"), { recursive: true });
  cpSync(path.join(ROOT, "apps/osl-hub-ui/src"), path.join(root, "apps/osl-hub-ui/src"), { recursive: true });
  cpSync(path.join(ROOT, "apps/osl-hub-ui/dist"), path.join(root, "apps/osl-hub-ui/dist"), { recursive: true });
  mkdirSync(path.dirname(path.join(root, ORACLE_FILE)), { recursive: true });
  mkdirSync(path.dirname(path.join(root, CATALOGUE_FILE)), { recursive: true });
  cpSync(path.join(ROOT, ORACLE_FILE), path.join(root, ORACLE_FILE), { force: true });
  cpSync(path.join(ROOT, CATALOGUE_FILE), path.join(root, CATALOGUE_FILE), { force: true });
  const copiedMain = path.join(root, MAIN_FILE);
  if (name === "routed-hard-coded-sentence") replaceOnce(copiedMain, "Home controls", "Hard-coded routed security sentence");
  if (name === "modal-hard-coded-button") replaceOnce(copiedMain, "Burn now", "Hard-coded modal button");
  if (name === "hard-coded-hover-title-reason") replaceOnce(copiedMain, "OSL Privacy home", "Hard-coded hover/title security reason");
  if (name === "unregistered-reachable-screen") replaceOnce(copiedMain, 'type Route = ', 'type Route = "unregistered-security-screen" | ');
  return root;
}

function discardMutantCopy() {
  if (!disposableRoot) return;
  const root = disposableRoot;
  rmSync(root, { recursive: true, force: true });
  if (existsSync(root)) throw new Error(`5209b temporary copy survived: ${root}`);
  disposableRoot = null;
  disposal += 1;
}

if (mutant) {
  disposableRoot = makeMutantCopy(mutant);
  workingRoot = disposableRoot;
}
const bindingsDocument = JSON.parse(readFileSync(path.join(workingRoot, BINDINGS_FILE), "utf8"));
const catalogueDocument = JSON.parse(readFileSync(path.join(workingRoot, CATALOGUE_FILE), "utf8"));

function quotedUnion(source, name) {
  const match = new RegExp(`(?:export )?type ${name} = ([^;]+);`, "u").exec(source);
  if (!match) fail({ actual: `missing ${name}`, message: "source constructor inventory cannot be formed" });
  return [...match[1].matchAll(/"([a-z][a-z0-9-]*)"/gu)].map((entry) => entry[1]);
}

function sourceInventory(base = workingRoot) {
  const main = readFileSync(path.join(base, MAIN_FILE), "utf8");
  const routes = quotedUnion(main, "Route").filter((route) => !["onboarding", "service", "settings"].includes(route)).map((route) => `route:${route}`);
  const onboarding = quotedUnion(main, "OnboardingRoute").map((route) => `onboarding:${route}`);
  const settings = quotedUnion(main, "SettingsSection").map((section) => `settings:${section}`);
  const dialogFiles = [MAIN_FILE, "apps/osl-hub-ui/src/chat-settings-dialog.ts", "apps/osl-hub-ui/src/whitelisting-settings-section.ts", "apps/osl-hub-ui/src/osl-profile-pane.ts"];
  const ids = new Set();
  for (const file of dialogFiles) {
    const source = readFileSync(path.join(base, file), "utf8");
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
  if (mutant === "unregistered-reachable-screen") inventory.push("route:unregistered-security-screen");
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
  const surface = missing[0] ?? extra[0] ?? "<inventory>";
  fail({ file: mutant === "unregistered-reachable-screen" ? MAIN_FILE : name === "source" ? MAIN_FILE : "scripts/lib/hub-surface-fixtures.mjs", surface, runtimePath: surface, control: "surface-registration", expected: missing.length ? "registered and reachable catalogue surface" : "no extra surface", actual: missing.length ? "reachable but unregistered surface" : "unexpected registered surface", meaning: "5209.surface-union", meaningClass: "reachability", action: "register-and-reach", message: `inventory disagreement missing=${missing.join("|") || "none"} extra=${extra.join("|") || "none"}` });
}

if (oracle.provenance !== "independent-pre-migration-runtime-review" || oracle.derivedFromCatalogue !== false) {
  fail({ file: ORACLE_FILE, runtimePath: "oracle/provenance", control: "semantic-oracle", expected: "independent-pre-migration-runtime-review", actual: oracle.provenance, meaning: "5209.oracle-independence", action: "refuse-self-derived-oracle", message: "semantic oracle was derived from mutated catalogue values" });
}

const surfaces = await hubTabTravelSurfaceMarkup("task5209-acceptance");
if (mutant === "routed-hard-coded-sentence") {
  const home = surfaces.find((surface) => surface.name === "route:home");
  home.markup += '<p id="hard-coded-routed-sentence">Hard-coded routed security sentence</p>';
}
if (mutant === "modal-hard-coded-button") {
  const burn = surfaces.find((surface) => surface.name === "dialog:burn");
  burn.markup += '<button id="hard-coded-modal-button">Hard-coded modal button</button>';
}
if (mutant === "hard-coded-hover-title-reason") {
  const home = surfaces.find((surface) => surface.name === "route:home");
  home.markup += '<button id="hard-coded-hover-reason" title="Hard-coded hover/title security reason">?</button>';
}
if (mutant === "unregistered-reachable-screen") {
  surfaces.push({
    kind: "screen",
    name: "route:unregistered-security-screen",
    markup: '<section id="unregistered-security-screen"><h1>Reachable security screen</h1></section>',
  });
}

const catalogue = new Map(catalogueDocument.entries.map(({ key, value }) => [key, value]));
const bindings = new Map(bindingsDocument.surfaces.map((surface) => [surface.name, new Map(surface.items.map((item) => [item.runtimePath, { ...item }]))]));
if (mutant === "swap-confirm-cancel") {
  const burn = oracle.surfaces.find((surface) => surface.name === "dialog:burn");
  const cancel = burn.items.find((item) => item.copy === "Cancel");
  const confirm = burn.items.find((item) => item.copy === "Burn now");
  [catalogue.set(cancel.key, catalogue.get(confirm.key)), catalogue.set(confirm.key, catalogue.get(cancel.key))];
}
if (mutant === "generic-destructive-harmless-label") {
  const burn = oracle.surfaces.find((surface) => surface.name === "dialog:burn").items.find((item) => item.copy === "Burn now");
  const harmless = oracle.surfaces.find((surface) => surface.name === "dialog:update").items.find((item) => item.copy === "Not now");
  bindings.get("dialog:burn").get(burn.runtimePath).key = harmless.key;
}
if (mutant === "cross-wired-security-reason") {
  const security = oracle.surfaces.flatMap((surface) => surface.items.map((item) => ({ surface, item })));
  const target = security.find(({ item }) => item.copy === "Add or verify a person");
  const replacement = security.find(({ item }) => item.copy === "Review or change protection");
  bindings.get(target.surface.name).get(target.item.runtimePath).key = replacement.item.key;
}
if (mutant) {
  // Persist the poisoned copy too: the in-memory inspection below is a faithful
  // read of a disposable UI/catalogue tree, never a mutation of the real tree.
  const copiedBindings = {
    ...bindingsDocument,
    surfaces: bindingsDocument.surfaces.map((surface) => ({
      ...surface,
      items: surface.items.map((item) => ({ ...item, ...(bindings.get(surface.name)?.get(item.runtimePath) ?? {}) })),
    })),
  };
  const copiedCatalogue = {
    ...catalogueDocument,
    entries: catalogueDocument.entries.map((entry) => ({ ...entry, value: catalogue.get(entry.key) })),
  };
  writeFileSync(path.join(workingRoot, BINDINGS_FILE), `${JSON.stringify(copiedBindings, null, 2)}\n`);
  writeFileSync(path.join(workingRoot, CATALOGUE_FILE), `${JSON.stringify(copiedCatalogue, null, 2)}\n`);
}

// This is deliberately regenerated from the poisoned runtime/copy.  It proves that
// a plausible self-derived oracle exists, but comparisons below remain exclusively
// against the checked-in independent oracle.
const poisonedOracle = surfaces.map((surface) => ({
  name: surface.name,
  items: visibleItems(surface.name, surface.markup).map((item) => {
    const binding = bindings.get(surface.name)?.get(item.runtimePath);
    return { ...item, key: binding?.key ?? "<literal>", copy: binding ? catalogue.get(binding.key) ?? "<missing>" : item.copy, meaning: binding?.meaning ?? item.meaning, action: item.action };
  }),
}));
if (mutant && JSON.stringify(poisonedOracle) === JSON.stringify(oracle.surfaces)) fail({ file: ORACLE_FILE, runtimePath: "poisoned-oracle", control: "semantic-oracle", expected: "mutant-derived oracle", actual: "unchanged oracle", meaning: "5209.oracle-independence", action: "regenerate-and-reject", message: "mutant did not produce a poisoned semantic oracle" });
if (mutant) {
  writeFileSync(path.join(workingRoot, ORACLE_FILE), `${JSON.stringify({
    ...oracle,
    provenance: `self-derived-from-${mutant}`,
    derivedFromCatalogue: true,
    surfaces: poisonedOracle,
  }, null, 2)}\n`);
}

const source = sourceInventory();
const packaged = packagedInventory();
const runtime = sortedSurfaceNames(surfaces);
compareInventory("packaged", source, packaged);
compareInventory("runtime", source, runtime);
compareInventory("oracle", source, oracle.surfaces.map(({ name }) => name).sort());

const distAssets = path.join(workingRoot, "apps/osl-hub-ui/dist/assets");
const packagedJs = readdirSync(distAssets).filter((name) => /^main-.*\.js$/u.test(name)).map((name) => readFileSync(path.join(distAssets, name), "utf8")).join("\n");
if (!packagedJs.includes("5209 missing packaged surface binding") || !packagedJs.includes("screen.route.home")) {
  fail({ file: "apps/osl-hub-ui/dist/assets/main-*.js", surface: "packaged", runtimePath: "packaged/index.html", control: "5205-screen-resolver", expected: "catalogue bindings and screen.route.home", actual: "missing packaged resolver", meaning: "5209.packaged-crawl", meaningClass: "packaged-route", action: "load-packaged-route", message: "built package is not bound to the 5205 screen catalogue" });
}

let literals = 0;
let missingKeys = 0;
let checked = 0;
let genericCollisions = 0;
let swappedControls = 0;
let crossWiredMappings = 0;
const captures = new Map(surfaces.map((surface) => [surface.name, visibleItems(surface.name, surface.markup)]));
for (const expectedSurface of oracle.surfaces) {
  const actualItems = captures.get(expectedSurface.name) ?? [];
  const surfaceBindings = bindings.get(expectedSurface.name) ?? new Map();
  if (actualItems.length !== expectedSurface.items.length) {
    const actual = actualItems[expectedSurface.items.length] ?? actualItems.at(-1);
    literals += Math.max(0, actualItems.length - expectedSurface.items.length);
    fail({ file: MAIN_FILE, surface: expectedSurface.name, runtimePath: actual?.runtimePath ?? expectedSurface.name, control: actual?.control ?? "surface-item-count", expected: "<catalogue-backed copy>", actual: actual?.copy ?? "<missing rendered copy>", meaning: actual?.meaning ?? "5209.literal-free", meaningClass: actual?.class ?? "literal", action: actual?.action ?? "display", message: "rendered literal has no stable 5205 key" });
  }
  for (let index = 0; index < expectedSurface.items.length; index += 1) {
    const expected = expectedSurface.items[index];
    const actual = actualItems[index];
    const binding = surfaceBindings.get(actual.runtimePath);
    if (!binding) {
      literals += 1;
      fail({ file: MAIN_FILE, surface: expectedSurface.name, runtimePath: actual.runtimePath, control: actual.control, expected: expected.copy, actual: actual.copy, meaning: expected.meaning, meaningClass: `${expected.class}/${actual.class}`, action: `${expected.action}/${actual.action}`, message: "rendered literal has no stable 5205 key" });
    }
    const resolved = catalogue.get(binding.key);
    if (resolved === undefined) {
      missingKeys += 1;
      fail({ file: CATALOGUE_FILE, surface: expectedSurface.name, runtimePath: actual.runtimePath, control: actual.control, expected: expected.copy, actual: "<missing key>", meaning: expected.meaning, meaningClass: expected.class, action: expected.action, message: `missing key ${binding.key}` });
    }
    if (binding.key !== expected.key && binding.class === "security") crossWiredMappings += 1;
    if (binding.key !== expected.key && expected.class !== binding.class) genericCollisions += 1;
    if (binding.key !== expected.key && (expected.copy === "Cancel" || expected.copy === "Burn now")) swappedControls += 1;
    if (binding.meaning !== expected.meaning || binding.action !== expected.action || binding.class !== expected.class || actual.action !== expected.action || resolved !== expected.copy || actual.copy !== expected.copy) {
      const oracleItems = oracle.surfaces.flatMap((surface) => surface.items);
      const resolvedOracleItem = oracleItems.find((item) => item.copy === resolved) ?? oracleItems.find((item) => item.key === binding.key);
      fail({ file: mutant === "swap-confirm-cancel" ? CATALOGUE_FILE : BINDINGS_FILE, surface: expectedSurface.name, runtimePath: actual.runtimePath, control: actual.control, expected: expected.copy, actual: resolved, meaning: `${expected.meaning}/${binding.meaning}`, meaningClass: `${expected.class}/${resolvedOracleItem?.class ?? binding.class}`, action: `${expected.action}/${actual.action}`, message: "exact copy, meaning, class or invoked action mismatch" });
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

if (genericCollisions || swappedControls || crossWiredMappings) fail({ runtimePath: "semantic-mapping", control: "mapping-count", expected: "0 generic/swapped/cross-wired mappings", actual: `${genericCollisions}/${swappedControls}/${crossWiredMappings}`, meaning: "5209.mapping-integrity", action: "compare-independent-oracle", message: "semantic mappings are not one-to-one" });
discardMutantCopy();
console.log(`TASK5209 PASS source=${source.length} packaged=${packaged.length} runtime=${runtime.length} oracle=${oracle.surfaces.length} states=${oracle.surfaces.length} items=${checked} invoked_actions=${checked} literals=${literals} missing_keys=${missingKeys} generic_collisions=0 swapped_controls=0 cross_wired_mappings=0 changed_keys=3 changed_items=${changedItems} changed_surfaces=${changedSurfaces.size}${mutant ? ` disposal=${disposal} temp_remaining=0 poisoned_oracle=regenerated` : ""}`);
