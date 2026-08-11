#!/usr/bin/env node

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";

const EXACT = "OSL ships in English only and has not been verified on Windows installations using a non-English display language or regional format.";
const CATALOGUE_KEY = "about.interface_language.disclosure";
const REQUIRED_ARTIFACTS = [
  "crates/english-catalogue/catalogues/en-US.v1.json",
  "crates/english-catalogue/src/lib.rs",
  "crates/ipc/src/app_preferences.rs",
  "crates/ipc/src/commands.rs",
  "crates/ipc/src/screen_words.rs",
  "apps/osl-hub-ui/src/interface-language.ts",
  "apps/osl-hub-ui/src/main.ts",
  "apps/osl-hub-ui/src/adapters.ts",
  "apps/osl-hub/permissions/hub.toml",
  "apps/osl-hub/capabilities/hub.json",
];

function words(value) {
  return value.toLowerCase().match(/[a-z]+/gu) ?? [];
}

function fail(artifact, surface, problem, actual = "") {
  const actualWords = new Set(words(actual));
  const missingWords = [...new Set(words(EXACT).filter((word) => !actualWords.has(word)))];
  console.error(
    `TASK5206B_RED artifact=${artifact} surface=${surface} catalogue_key=${CATALOGUE_KEY} missing_words=${missingWords.length ? missingWords.join(",") : "none"} problem=${problem}`,
  );
  process.exit(1);
}

const manifestArgument = process.argv[2];
if (!manifestArgument) fail("<manifest>", "release-inventory", "usage: task-5206b-release-check.mjs <release-manifest.json>");
const manifestPath = resolve(manifestArgument);
if (!existsSync(manifestPath)) fail(manifestPath, "release-inventory", "release manifest is missing");

let manifest;
try {
  manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
} catch (error) {
  fail(manifestPath, "release-inventory", `release manifest is not valid JSON: ${error}`);
}
if (manifest.schema !== "osl.task-5206b.release.v1") {
  fail(manifestPath, "release-inventory", `unexpected manifest schema=${String(manifest.schema)}`);
}
if (!Array.isArray(manifest.artifacts)) {
  fail(manifestPath, "release-inventory", "manifest artifacts are not an array");
}
const releaseRoot = isAbsolute(manifest.release_root ?? "")
  ? resolve(manifest.release_root)
  : resolve(dirname(manifestPath), manifest.release_root ?? ".");

const artifactSet = new Set(manifest.artifacts);
for (const artifact of REQUIRED_ARTIFACTS) {
  if (!artifactSet.has(artifact)) fail(manifestPath, "release-inventory", `required artifact is absent from manifest: ${artifact}`);
}
if (artifactSet.size !== manifest.artifacts.length) {
  fail(manifestPath, "release-inventory", "manifest contains a duplicate artifact entry");
}

function read(path, surface = "release-inventory") {
  if (!artifactSet.has(path) && REQUIRED_ARTIFACTS.includes(path)) {
    fail(manifestPath, surface, `artifact was read without being inventoried: ${path}`);
  }
  const absolute = join(releaseRoot, path);
  if (!existsSync(absolute)) fail(path, surface, "required artifact is missing");
  return readFileSync(absolute, "utf8");
}

function filesBelow(path) {
  const absolute = join(releaseRoot, path);
  if (!existsSync(absolute)) return [];
  const found = [];
  for (const name of readdirSync(absolute)) {
    const candidate = join(absolute, name);
    const candidateRelative = relative(releaseRoot, candidate);
    if (statSync(candidate).isDirectory()) found.push(...filesBelow(candidateRelative));
    else found.push(candidateRelative);
  }
  return found;
}

const catalogueFiles = filesBelow("crates/english-catalogue/catalogues").filter((path) => path.endsWith(".json"));
if (catalogueFiles.length !== 1 || basename(catalogueFiles[0]) !== "en-US.v1.json") {
  const nonEnglish = catalogueFiles.find((path) => basename(path) !== "en-US.v1.json");
  fail(nonEnglish ?? "crates/english-catalogue/catalogues", "packaged-resources", `expected exactly one English catalogue; found=${catalogueFiles.join(",") || "none"}`);
}

const cataloguePath = catalogueFiles[0];
let catalogue;
try {
  catalogue = JSON.parse(read(cataloguePath, "packaged-resources"));
} catch (error) {
  fail(cataloguePath, "packaged-resources", `catalogue is not valid JSON: ${error}`);
}
if (catalogue.locale !== "en-US") {
  fail(cataloguePath, "packaged-resources", `non-English catalogue locale=${String(catalogue.locale)}`, String(catalogue.locale));
}
const disclosureEntries = Array.isArray(catalogue.entries)
  ? catalogue.entries.filter((entry) => entry?.key === CATALOGUE_KEY)
  : [];
const disclosureText = disclosureEntries.map((entry) => String(entry.value ?? "")).join(" ");
if (disclosureEntries.length !== 1 || disclosureEntries[0].value !== EXACT) {
  fail(cataloguePath, "about", "exact non-English-Windows disclosure changed or missing", disclosureText);
}

const catalogueLibPath = "crates/english-catalogue/src/lib.rs";
const catalogueLib = read(catalogueLibPath, "source-branches");
const localeRegistration = catalogueLib.match(/pub const fn registered_locales[\s\S]*?\n\}/u)?.[0] ?? "";
if (!/->\s*\[&'static str;\s*1\]/u.test(localeRegistration) || !/\[ENGLISH_LOCALE\]/u.test(localeRegistration)) {
  fail(catalogueLibPath, "source-branches", "registered locale inventory is not the fixed one-element English list", localeRegistration);
}
const keyPattern = new RegExp(`key: "${CATALOGUE_KEY.replaceAll(".", "\\.")}"`, "gu");
if ((catalogueLib.match(keyPattern) ?? []).length !== 1) {
  fail(catalogueLibPath, "source-branches", "production catalogue key is missing or duplicated", catalogueLib);
}

const translationBundles = [
  ...filesBelow("crates/ipc/src/screen_words"),
  ...filesBelow("apps/osl-hub-ui/src/locales"),
  ...filesBelow("apps/osl-hub-ui/src/i18n"),
  ...filesBelow("apps/osl-hub-ui/src/translations"),
];
if (translationBundles.length !== 0) {
  fail(translationBundles[0], "source-branches", "hidden translation bundle survived the English-only launch gate");
}

const productionSources = [
  "crates/ipc/src/app_preferences.rs",
  "crates/ipc/src/commands.rs",
  "crates/ipc/src/screen_words.rs",
  "apps/osl-hub-ui/src/interface-language.ts",
  "apps/osl-hub-ui/src/main.ts",
  "apps/osl-hub-ui/src/adapters.ts",
];
const forbidden = [
  [/(?:get|save|reset)_language_choice/iu, "language selector command"],
  [/<select\b[^>]*(?:language|locale)|data-language-selector|language-selector/iu, "language selector without a complete catalogue"],
  [/Intl\.PluralRules|pluralRules|localePlural|cldr/iu, "locale-specific plural engine"],
  [/dir\s*=\s*["']rtl["']|direction\s*:\s*rtl|rtlBranch|hiddenRtl/iu, "RTL layout branch"],
  [/i18next|\bgettext\s*\(|\bsetLocale\s*\(/iu, "translation runtime"],
];
for (const path of productionSources) {
  const source = read(path, "source-branches");
  for (const [pattern, label] of forbidden) {
    if (pattern.test(source)) fail(path, "source-branches", `${label} found`, source);
  }
}

const modulePath = "apps/osl-hub-ui/src/interface-language.ts";
const moduleSource = read(modulePath, "about");
if (moduleSource.includes(EXACT)) {
  fail(modulePath, "about", "exact disclosure bypasses its catalogue key as a UI literal", moduleSource);
}
for (const marker of [
  `INTERFACE_LANGUAGE_DISCLOSURE_KEY = "${CATALOGUE_KEY}"`,
  `INTERFACE_LANGUAGE_NAME = "English"`,
  `INTERFACE_LANGUAGE_LOCALE = "en-US"`,
  `data-interface-language-surface="about"`,
  "data-interface-language-disclosure",
  `data-interface-locale=\"\${INTERFACE_LANGUAGE_NAME}\"`,
]) {
  if (!moduleSource.includes(marker)) fail(modulePath, "about", `runtime disclosure marker is missing: ${marker}`, moduleSource);
}
if (moduleSource.indexOf("data-interface-language-disclosure") > moduleSource.indexOf("data-interface-locale")) {
  fail(modulePath, "about", "English locale claim is reached before the required disclosure", moduleSource);
}

const mainPath = "apps/osl-hub-ui/src/main.ts";
const mainSource = read(mainPath, "about");
for (const marker of [
  "interfaceLanguageAboutMarkup(interfaceLanguageDisclosure)",
  "resolveHubEnglishCatalogueString(INTERFACE_LANGUAGE_DISCLOSURE_KEY)",
  `["about", "About"]`,
]) {
  if (!mainSource.includes(marker)) fail(mainPath, "about", `shipped About reachability is missing: ${marker}`, mainSource);
}
const renderIndex = mainSource.indexOf("interfaceLanguageAboutMarkup(interfaceLanguageDisclosure)");
const followingClaimIndex = mainSource.indexOf("availabilityNavigationMarkup()", renderIndex);
if (followingClaimIndex < 0 || renderIndex > followingClaimIndex) {
  fail(mainPath, "about", "required disclosure is routed after another About claim", mainSource);
}

const permissionPath = "apps/osl-hub/permissions/hub.toml";
if (!read(permissionPath, "about").includes('commands.allow = ["resolve_english_catalogue_string"]')) {
  fail(permissionPath, "about", "catalogue resolver permission is absent");
}
const capabilityPath = "apps/osl-hub/capabilities/hub.json";
if (!read(capabilityPath, "about").includes('"allow-resolve-english-catalogue-string"')) {
  fail(capabilityPath, "about", "catalogue resolver is unreachable from the shipped main WebView");
}

const distFiles = filesBelow("apps/osl-hub-ui/dist");
const packagedScripts = distFiles.filter((path) => path.endsWith(".js"));
const unicodeFontAssets = distFiles.filter((path) => path.endsWith(".woff2"));
if (packagedScripts.length === 0) fail("apps/osl-hub-ui/dist", "packaged-resources", "built UI scripts are absent");
if (unicodeFontAssets.length === 0) fail("apps/osl-hub-ui/dist", "unicode", "Unicode-capable bundled font assets are absent");
const packagedText = packagedScripts.map((path) => readFileSync(join(releaseRoot, path), "utf8")).join("\n");
for (const marker of [CATALOGUE_KEY, "data-interface-language-surface", "data-interface-language-disclosure", "data-interface-locale"]) {
  if (!packagedText.includes(marker)) fail("apps/osl-hub-ui/dist", "packaged-resources", `built UI is stale or missing ${marker}`);
}
for (const [pattern, label] of forbidden.slice(1)) {
  if (pattern.test(packagedText)) fail("apps/osl-hub-ui/dist", "packaged-resources", `${label} found in built UI`, packagedText);
}

const targetRoot = resolve(manifest.target_root ?? process.env.CARGO_TARGET_DIR ?? "");
const dependencyRoot = join(targetRoot, "debug/deps");
const binaryCandidates = existsSync(dependencyRoot)
  ? readdirSync(dependencyRoot)
      .filter((name) => /^libosl_english_catalogue-.*\.rlib$/u.test(name))
      .map((name) => join(dependencyRoot, name))
  : [];
const packagedBinary = binaryCandidates.find((path) => readFileSync(path).includes(Buffer.from(EXACT, "utf8")));
if (!packagedBinary) {
  fail(`${targetRoot}/debug/deps/libosl_english_catalogue-*.rlib`, "packaged-binary", "built packaged catalogue does not contain the exact disclosure");
}

console.log(
  `TASK5206_RELEASE_INVENTORY manifest=${basename(manifestPath)} attack=${manifest.attack ?? "unmutated"} artifacts=${artifactSet.size} packaged_interface_locales=1 locale_name=English selectors=0 non_english_catalogues=0 rtl_branches=0 translation_bundles=0 packaged_scripts=${packagedScripts.length} packaged_binaries=1 unicode_font_assets=${unicodeFontAssets.length} about_reachable=1 exact_disclosure=1 catalogue_key=${CATALOGUE_KEY}`,
);
