#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { hubTabTravelSurfaceMarkup } from "../lib/hub-surface-fixtures.mjs";
import { visibleItems } from "./task5209_surface_model.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const CONTRACTS = path.join(ROOT, "contracts");
const ORACLE = path.join(CONTRACTS, "task-5209-screen-oracle.json");
const CATALOGUE = path.join(ROOT, "crates/english-catalogue/catalogues/en-US.v1.json");
const SCREEN_KEYS = path.join(ROOT, "crates/english-catalogue/src/screen_keys.rs");
const BINDINGS = path.join(ROOT, "apps/osl-hub-ui/src/catalogue/task-5209-screen-bindings.json");

function writeJson(file, value) {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

async function capture() {
  const surfaces = await hubTabTravelSurfaceMarkup("task5209-freeze");
  return surfaces.map(({ kind, name, markup }) => ({ kind, name, items: visibleItems(name, markup) }));
}

const mode = process.argv[2];
if (!['freeze-oracle', 'build-catalogue'].includes(mode)) {
  console.error("usage: task5209_freeze.mjs freeze-oracle|build-catalogue");
  process.exit(2);
}

const surfaces = await capture();
const itemCount = surfaces.reduce((sum, surface) => sum + surface.items.length, 0);
if (mode === "freeze-oracle") {
  if (existsSync(ORACLE)) throw new Error(`refusing to replace fixed oracle: ${ORACLE}`);
  writeJson(ORACLE, {
    schema: "osl.screen-semantic-oracle.5209.v1",
    provenance: "independent-pre-migration-runtime-review",
    derivedFromCatalogue: false,
    surfaces,
  });
  console.log(`TASK5209_ORACLE_FROZEN surfaces=${surfaces.length} items=${itemCount} catalogue_entries_written=0 derived_from_catalogue=false`);
  process.exit(0);
}

if (!existsSync(ORACLE)) throw new Error(`semantic oracle must be fixed before migration: ${ORACLE}`);
const oracle = JSON.parse(readFileSync(ORACLE, "utf8"));
if (oracle.provenance !== "independent-pre-migration-runtime-review" || oracle.derivedFromCatalogue !== false) {
  throw new Error("semantic oracle provenance is not independent of the catalogue");
}
const oracleShape = JSON.stringify(oracle.surfaces.map((surface) => [surface.name, surface.items.map((item) => [item.runtimePath, item.copy, item.meaning, item.action, item.class])]));
const captureShape = JSON.stringify(surfaces.map((surface) => [surface.name, surface.items.map((item) => [item.runtimePath, item.copy, item.meaning, item.action, item.class])]));
if (oracleShape !== captureShape) throw new Error("pre-migration runtime changed after the semantic oracle was fixed");

const catalogue = JSON.parse(readFileSync(CATALOGUE, "utf8"));
catalogue.entries = catalogue.entries.filter((entry) => !entry.key.startsWith("screen."));
const allItems = surfaces.flatMap((surface) => surface.items);
catalogue.entries.push(...allItems.map(({ key, copy }) => ({ key, value: copy })));
writeJson(CATALOGUE, catalogue);
writeJson(BINDINGS, {
  schema: "osl.screen-catalogue-bindings.5209.v1",
  surfaces: surfaces.map((surface) => ({
    name: surface.name,
    items: surface.items.map(({ runtimePath, control, key, meaning, action, class: semanticClass, sourceDigest }) => ({ runtimePath, control, key, meaning, action, class: semanticClass, sourceDigest })),
  })),
});
const rust = [
  "// Generated from the independently enumerated shipping surface bindings.",
  "// Values are deliberately absent: the catalogue cannot define its own inventory.",
  "use crate::ProductionKey;",
  "pub const SCREEN_PRODUCTION_KEYS: &[ProductionKey] = &[",
  ...allItems.flatMap(({ key }) => [
    "    ProductionKey {",
    `        key: ${JSON.stringify(key)},`,
    "        placeholders: &[],",
    "    },",
  ]),
  "];",
  "",
].join("\n");
writeFileSync(SCREEN_KEYS, rust);
console.log(`TASK5209_CATALOGUE_BUILT surfaces=${surfaces.length} keys=${allItems.length} oracle_preexisted=true bindings=${allItems.length}`);
