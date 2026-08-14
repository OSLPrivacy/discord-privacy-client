#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const CUT_CHANGE = "278ba318787ddc0ac46f9d77cd73970f4f3724b3";
const RULING_PATH = "data/surface-ruling-2026-08-05.json";

const ACTIVE_SOURCES = [
  "apps/osl-hub-ui/src/services.ts",
  "apps/osl-hub-ui/src/browser-service-qa-shell.ts",
  "apps/osl-hub-ui/src/desktop-service-policy.ts",
  "apps/osl-hub/src/models.rs",
  "apps/osl-hub/src/native_apps.rs",
  "apps/osl-hub/src/service_host.rs",
  "apps/osl-hub/src/services.rs",
];

const REMOVED_PIECE_CLASSIFICATION = [
  {
    id: "instagram",
    kind: "app",
    classification: "deliberate",
    basis: "listed in data/surface-ruling-2026-08-05.json cut_surfaces",
  },
  {
    id: "snapchat",
    kind: "app",
    classification: "deliberate",
    basis: "listed in data/surface-ruling-2026-08-05.json cut_surfaces",
  },
  {
    id: "x",
    kind: "app",
    classification: "deliberate",
    basis: "listed in data/surface-ruling-2026-08-05.json cut_surfaces",
  },
  {
    id: "messenger",
    kind: "app",
    classification: "deliberate",
    basis: "listed in data/surface-ruling-2026-08-05.json cut_surfaces",
  },
  {
    id: "slack",
    kind: "app",
    classification: "deliberate",
    basis: "listed in data/surface-ruling-2026-08-05.json cut_surfaces",
  },
  {
    id: "linkedin",
    kind: "app",
    classification: "deliberate",
    basis: "listed in data/surface-ruling-2026-08-05.json cut_surfaces",
  },
  {
    id: "teams",
    kind: "app",
    classification: "deliberate",
    basis: "listed in data/surface-ruling-2026-08-05.json cut_surfaces",
  },
  {
    id: "fastmail",
    kind: "mail-service",
    classification: "accidental",
    repairTask: "TASK 4334 - write down the true state of the ten outside mail services",
    basis: "removed from active mail-provider inventories but absent from the ruling cut_surfaces",
  },
  {
    id: "zoho",
    kind: "mail-service",
    classification: "accidental",
    repairTask: "TASK 4334 - write down the true state of the ten outside mail services",
    basis: "removed from active mail-provider inventories but absent from the ruling cut_surfaces",
  },
];

function gitShow(ref, path) {
  return execFileSync("git", ["show", `${ref}:${path}`], { encoding: "utf8" });
}

function readTree(ref, path) {
  try {
    return gitShow(ref, path);
  } catch (error) {
    if (error.status === 128) return "";
    throw error;
  }
}

function quotedValues(text) {
  return [...text.matchAll(/"([a-z][a-z0-9-]*)"/g)].map((match) => match[1]);
}

function rustVariantId(variant) {
  return variant
    .replace("WhatsApp", "Whatsapp")
    .replace(/[A-Z]/g, (letter, offset) => `${offset === 0 ? "" : "-"}${letter.toLowerCase()}`)
    .replace("whats-app", "whatsapp");
}

function addTsUnion(set, text, name) {
  const match = new RegExp(`export type ${name} = ([^;]+);`, "u").exec(text);
  if (match) for (const value of quotedValues(match[1])) set.add(value);
}

function addTsArray(set, text, name) {
  const match = new RegExp(`(?:export )?const ${name}(?:: readonly [^=]+)? = \\[([\\s\\S]*?)\\](?: as const)?;`, "u").exec(text);
  if (match) for (const value of quotedValues(match[1])) set.add(value);
}

function addRustEnum(set, text, name) {
  const match = new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`, "u").exec(text);
  if (match) {
    for (const entry of match[1].matchAll(/^\s*([A-Z][A-Za-z0-9]*)\b/gm)) {
      set.add(rustVariantId(entry[1]));
    }
  }
}

function rustFunctionBody(text, name) {
  const start = text.indexOf(`fn ${name}`);
  if (start < 0) return "";
  const open = text.indexOf("{", start);
  if (open < 0) return "";
  let depth = 0;
  for (let index = open; index < text.length; index += 1) {
    if (text[index] === "{") depth += 1;
    if (text[index] === "}") depth -= 1;
    if (depth === 0) return text.slice(open + 1, index);
  }
  return "";
}

function activePiecesAt(ref) {
  const set = new Set();
  const source = Object.fromEntries(ACTIVE_SOURCES.map((path) => [path, readTree(ref, path)]));

  const servicesTs = source["apps/osl-hub-ui/src/services.ts"];
  addTsUnion(set, servicesTs, "ServiceId");
  addTsUnion(set, servicesTs, "EmailProvider");
  addTsUnion(set, servicesTs, "OfferedEmailProvider");
  addTsUnion(set, servicesTs, "NativeAppId");
  addTsArray(set, servicesTs, "serviceIds");
  addTsArray(set, servicesTs, "emailProviders");
  addTsArray(set, servicesTs, "firefoxServiceIds");
  for (const match of servicesTs.matchAll(/(?:homeApp|service)\("([a-z][a-z0-9]*)"/g)) {
    set.add(match[1]);
  }

  addTsArray(set, source["apps/osl-hub-ui/src/browser-service-qa-shell.ts"], "browserServiceQaIds");
  for (const match of source["apps/osl-hub-ui/src/desktop-service-policy.ts"].matchAll(/policy\("([a-z][a-z0-9]*)"/g)) {
    set.add(match[1]);
  }

  const modelsRs = source["apps/osl-hub/src/models.rs"];
  addRustEnum(set, modelsRs, "ServiceKind");
  addRustEnum(set, modelsRs, "EmailProvider");
  const nativeAppsRs = source["apps/osl-hub/src/native_apps.rs"];
  addRustEnum(set, nativeAppsRs, "NativeAppId");
  addRustEnum(set, nativeAppsRs, "FirefoxServiceId");
  for (const match of nativeAppsRs.matchAll(/FirefoxServiceId::([A-Z][A-Za-z0-9]*)/g)) {
    set.add(rustVariantId(match[1]));
  }

  const serviceHostRs = source["apps/osl-hub/src/service_host.rs"];
  const serviceManifestStart = serviceHostRs.indexOf("const SERVICES");
  const serviceManifestEnd = serviceHostRs.indexOf("const EMAIL_GMAIL");
  if (serviceManifestStart >= 0 && serviceManifestEnd > serviceManifestStart) {
    for (const match of serviceHostRs.slice(serviceManifestStart, serviceManifestEnd).matchAll(/id: "([a-z][a-z0-9-]*)"/g)) {
      set.add(match[1]);
    }
  }

  const servicesRs = source["apps/osl-hub/src/services.rs"];
  for (const value of quotedValues(rustFunctionBody(servicesRs, "service_kind_from_id"))) {
    set.add(value);
  }
  for (const match of rustFunctionBody(servicesRs, "service_descriptors").matchAll(/ServiceKind::([A-Z][A-Za-z0-9]*)/g)) {
    set.add(rustVariantId(match[1]));
  }

  return set;
}

function sorted(values) {
  return [...values].sort();
}

function fail(message) {
  console.error(`TASK4268_FAIL ${message}`);
  process.exitCode = 1;
}

const before = activePiecesAt(`${CUT_CHANGE}^`);
const after = activePiecesAt(CUT_CHANGE);
const derivedRemovedPieces = sorted([...before].filter((piece) => !after.has(piece)));
const ruling = JSON.parse(readFileSync(RULING_PATH, "utf8"));
const cutSurfaces = new Set(ruling.cut_surfaces);
const classificationById = new Map(REMOVED_PIECE_CLASSIFICATION.map((row) => [row.id, row]));
const classifiedIds = sorted(classificationById.keys());
const missingClassifications = derivedRemovedPieces.filter((piece) => !classificationById.has(piece));
const extraClassifications = classifiedIds.filter((piece) => !derivedRemovedPieces.includes(piece));
const stayedNeedsRemoved = derivedRemovedPieces.filter((piece) => after.has(piece));
const neitherWord = REMOVED_PIECE_CLASSIFICATION.filter(
  (row) => row.classification !== "deliberate" && row.classification !== "accidental",
);
const accidentalWithoutRepair = REMOVED_PIECE_CLASSIFICATION.filter(
  (row) => row.classification === "accidental" && !/^TASK \d+\b/.test(row.repairTask ?? ""),
);
const missingMailServices = REMOVED_PIECE_CLASSIFICATION.filter(
  (row) => row.kind === "mail-service" && !cutSurfaces.has(row.id),
).map((row) => row.id);

console.log("# Task 4268 same-day cut audit");
console.log(`CUT_CHANGE=${CUT_CHANGE}`);
console.log(`ACTIVE_SOURCE_COUNT=${ACTIVE_SOURCES.length}`);
console.log(`BEFORE_ACTIVE_PIECE_COUNT=${before.size}`);
console.log(`AFTER_ACTIVE_PIECE_COUNT=${after.size}`);
console.log(`REMOVED_PIECE_COUNT=${derivedRemovedPieces.length}`);
for (const row of REMOVED_PIECE_CLASSIFICATION) {
  const repair = row.classification === "accidental" ? ` repair=\"${row.repairTask}\"` : "";
  console.log(`PIECE ${row.id} ${row.classification} kind=${row.kind}${repair} basis="${row.basis}"`);
}
console.log(`MAIL_SERVICES_MISSING_FROM_RULING=${missingMailServices.join(",")}`);
console.log(`MAIL_SERVICES_MISSING_FROM_RULING_COUNT=${missingMailServices.length}`);
console.log(`PIECES_WITH_NEITHER_DELIBERATE_NOR_ACCIDENTAL=${neitherWord.length}`);
console.log(`ACCIDENTAL_WITHOUT_REPAIR_TASK_COUNT=${accidentalWithoutRepair.length}`);
console.log(`STAYED_NEEDS_REMOVED_PIECE_COUNT=${stayedNeedsRemoved.length}`);
console.log(`MISSING_CLASSIFICATION_COUNT=${missingClassifications.length}`);
console.log(`EXTRA_CLASSIFICATION_COUNT=${extraClassifications.length}`);

if (derivedRemovedPieces.length <= 0) fail("removed piece count is not above 0");
if (missingClassifications.length > 0) {
  fail(`classification list does not mention removed piece(s): ${missingClassifications.join(",")}`);
}
if (extraClassifications.length > 0) {
  fail(`classification list mentions piece(s) not removed by ${CUT_CHANGE}: ${extraClassifications.join(",")}`);
}
if (neitherWord.length > 0) fail(`piece(s) with neither deliberate nor accidental: ${neitherWord.map((row) => row.id).join(",")}`);
if (accidentalWithoutRepair.length > 0) fail(`accidental piece(s) without numbered repair task: ${accidentalWithoutRepair.map((row) => row.id).join(",")}`);
if (missingMailServices.join(",") !== "fastmail,zoho") {
  fail(`missing mail services are ${missingMailServices.join(",") || "(none)"}, expected fastmail,zoho`);
}
if (stayedNeedsRemoved.length > 0) fail(`removed piece(s) still needed by active inventories: ${stayedNeedsRemoved.join(",")}`);
if (process.exitCode) process.exit(process.exitCode);
