/**
 * TASK 6890 — the final design package is the manifest authority.
 *
 * This intentionally has no handwritten page catalogue.  The source files and
 * their in-file `data-osl-shipping-manifest` declarations are read each time
 * this runs.
 * `source-index.json` is a generated attestation (create it with --record),
 * not a second manifest: it makes a same-count rename fail closed.
 */
import { createHash } from "node:crypto";
import { readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
export const finalDesignDirectory = path.resolve(here, "../../../design/UI-FINAL-INSTRUCTIONS");
const indexName = ".shipping-source-index.json";
const pageExtension = ".dc.html";
// D27: the final delivery contains 70 design-page leaves.  The source index is
// an attestation beside those leaves, not a seventy-first page.
export const D27_FINAL_PAGE_LEAVES = 70;
export const D27_PACKAGE_SIZE_QUOTE = "D27: 70 design-page leaves";
const deletedByD10a = new Set([
  "Onboarding Detected",
  "Onboarding Apps",
  "Onboarding Silent Visible",
  "Onboarding Recovery Empty",
]);

function fail(message) {
  throw new Error(`TASK 6890: ${message}`);
}

function digest(text) {
  return createHash("sha256").update(text).digest("hex");
}

function pageNameFrom(relativePath) {
  return path.basename(relativePath, pageExtension);
}

async function listFiles(root) {
  let entries;
  try {
    entries = await readdir(root, { withFileTypes: true });
  } catch (error) {
    if (error?.code === "ENOENT") fail(`final source directory is missing: ${root}`);
    throw error;
  }
  const files = [];
  for (const entry of entries) {
    const full = path.join(root, entry.name);
    if (entry.isDirectory()) files.push(...await listFiles(full));
    else if (entry.isFile() && entry.name.endsWith(pageExtension)) files.push(full);
  }
  return files.sort((a, b) => a.localeCompare(b));
}

async function sourceInventory(root) {
  const files = await listFiles(root);
  const rows = await Promise.all(files.map(async (full) => {
    const relativePath = path.relative(root, full).split(path.sep).join("/");
    const markup = await readFile(full, "utf8");
    return { relativePath, sha256: digest(markup) };
  }));
  return rows;
}

function sameJson(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

export async function recordSourceIndex(root = finalDesignDirectory) {
  const inventory = await sourceInventory(root);
  if (inventory.length !== D27_FINAL_PAGE_LEAVES) fail(`${D27_PACKAGE_SIZE_QUOTE}; expected exactly ${D27_FINAL_PAGE_LEAVES} .dc.html files, found ${inventory.length}`);
  await writeFile(path.join(root, indexName), `${JSON.stringify({ version: 1, inventory }, null, 2)}\n`);
  return inventory;
}

async function assertSourceIndex(root, inventory) {
  let index;
  const indexPath = path.join(root, indexName);
  try {
    index = JSON.parse(await readFile(indexPath, "utf8"));
  } catch (error) {
    if (error?.code === "ENOENT") fail(`source index is missing: ${indexPath}; run with --record after reviewing the final package`);
    fail(`cannot read source index ${indexPath}: ${error.message}`);
  }
  if (index?.version !== 1 || !Array.isArray(index.inventory)) fail(`invalid source index: ${indexPath}`);
  if (!sameJson(index.inventory, inventory)) {
    const indexed = new Set(index.inventory.map((entry) => entry.relativePath));
    const current = new Set(inventory.map((entry) => entry.relativePath));
    const changed = [...new Set([
      ...[...indexed].filter((file) => !current.has(file)),
      ...[...current].filter((file) => !indexed.has(file)),
      ...inventory.filter((entry) => indexed.has(entry.relativePath) && index.inventory.find((old) => old.relativePath === entry.relativePath)?.sha256 !== entry.sha256).map((entry) => entry.relativePath),
    ])].sort();
    fail(`final source changed (${changed.join(", ") || "index differs"}); additions, removals, renames, and markup changes require an explicit reviewed --record`);
  }
}

function declarationFor(sourceRelativePath, markup) {
  const annotation = /<script\b(?=[^>]*\bdata-osl-shipping-manifest\b)[^>]*>([\s\S]*?)<\/script\s*>/i.exec(markup);
  if (!annotation) fail(`${sourceRelativePath}: missing <script data-osl-shipping-manifest> declaration in the .dc.html source`);
  try {
    return JSON.parse(annotation[1]);
  } catch (error) {
    fail(`${sourceRelativePath}: invalid in-file shipping declaration: ${error.message}`);
  }
}

function capturedMarkupFrom(markup) {
  return markup.replace(/<script\b(?=[^>]*\bdata-osl-shipping-manifest\b)[^>]*>[\s\S]*?<\/script\s*>/ig, "");
}

function assertText(value, label, source) {
  if (typeof value !== "string" || !value.trim()) fail(`${source}: ${label} must be a non-empty string`);
  return value.trim();
}

function assertNoReachableRoute(row) {
  if (row.route !== null || row.drive !== null) fail(`${row.source}: absent page ${row.page} has a reachable route or drive`);
}

function assertDeletedAndExcludedRows(rows) {
  for (const page of deletedByD10a) {
    const row = rows.find((candidate) => candidate.page === page);
    if (!row) fail(`${page}: deleted by D10(a) but not recorded as absent`);
    if (row.kind !== "absent" || !row.ruling?.includes("D10(a)")) fail(`${page}: must be absent under D10(a)`);
    assertNoReachableRoute(row);
  }
  const mail = rows.filter((row) => /^OSL Mail\b/.test(row.page));
  if (mail.length !== 13) fail(`expected exactly 13 OSL Mail pages recorded as absent, found ${mail.length}`);
  for (const row of mail) {
    if (row.kind !== "absent" || !row.ruling?.includes("D10(c)")) fail(`${row.source}: OSL Mail page must be excluded under D10(c)`);
    assertNoReachableRoute(row);
  }
  const mullvad = rows.filter((row) => /mullvad.*onboarding|onboarding.*mullvad/i.test(row.page));
  if (!mullvad.length) fail("Mullvad onboarding is not recorded as an absent page");
  for (const row of mullvad) {
    if (row.kind !== "absent" || !row.ruling?.includes("D10(c)")) fail(`${row.source}: Mullvad onboarding must be excluded under D10(c)`);
    assertNoReachableRoute(row);
  }
  const nonProductSheets = rows.filter((row) => row.exclusion === "non-product-sheet");
  if (!nonProductSheets.length) fail("excluded non-product sheets are not recorded as absent");
  for (const row of nonProductSheets) {
    if (row.kind !== "absent" || !row.ruling?.includes("D10(c)")) fail(`${row.source}: non-product sheet must be excluded under D10(c)`);
    assertNoReachableRoute(row);
  }
}

function assertStates(rows) {
  const required = new Map([
    ["Home Empty", "Home"],
    ["Home Notifications Empty", "Home"],
    ["Home Notifications Off", "Home"],
    ["Onboarding Pro Active", "Onboarding Pro Code"],
  ]);
  for (const [page, parent] of required) {
    const row = rows.find((candidate) => candidate.page === page);
    if (!row || row.kind !== "state" || row.parent !== parent) fail(`${page}: must resolve to ${parent} as a named state, not a route`);
  }
  const accountRows = rows.filter((row) => /^Settings Account(?:\b| )/.test(row.page));
  if (accountRows.length !== 4) fail(`expected four Settings Account state files, found ${accountRows.length}`);
  for (const row of [...rows.filter((row) => row.kind === "state"), ...accountRows]) {
    if (row.kind !== "state") fail(`${row.source}: Settings Account file must be a state`);
    if (row.route !== null || typeof row.drive !== "string" || !row.drive.trim()) fail(`${row.source}: state must have no route and a named app drive`);
    if (!row.parent || !row.state) fail(`${row.source}: state must name its parent and state`);
    const parent = rows.find((candidate) => candidate.page === row.parent);
    if (!parent || parent.kind !== "routed" || typeof parent.route !== "string" || !parent.route.trim()) {
      fail(`${row.source}: state parent ${JSON.stringify(row.parent)} must be a routed page`);
    }
  }
}

function assertDistinctRoutedMarkup(rows) {
  const routed = rows.filter((row) => row.kind === "routed");
  const captures = new Map();
  for (const row of routed) {
    if (typeof row.route !== "string" || !row.route.trim() || !row.drive) fail(`${row.source}: routed page needs route and app drive`);
    const prior = captures.get(row.captureDigest);
    if (prior) fail(`${row.source}: captured markup is indistinguishable from ${prior.source}`);
    if (!new RegExp(`<(?:[^>]+)>[^<]*${row.page.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}|data-(?:page|screen|route)=["'][^"']*${row.page.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`, "i").test(row.capturedMarkup)) {
      fail(`${row.source}: captured markup has no identifying element naming ${row.page}`);
    }
    captures.set(row.captureDigest, row);
  }
}

export async function deriveShippingManifest(root = finalDesignDirectory) {
  const inventory = await sourceInventory(root);
  if (inventory.length !== D27_FINAL_PAGE_LEAVES) fail(`${D27_PACKAGE_SIZE_QUOTE}; expected exactly ${D27_FINAL_PAGE_LEAVES} .dc.html files, found ${inventory.length}`);
  await assertSourceIndex(root, inventory);
  const rows = [];
  for (const item of inventory) {
    const source = item.relativePath;
    const page = pageNameFrom(source);
    const markup = await readFile(path.join(root, source), "utf8");
    const declaration = declarationFor(source, markup);
    if (declaration.page !== page) fail(`${source}: declaration page must equal the filename page (${page})`);
    const kind = declaration.kind;
    if (!["routed", "state", "absent", "not-built-yet"].includes(kind)) fail(`${source}: kind must be routed, state, absent, or not-built-yet`);
    const row = {
      source,
      page,
      kind,
      route: declaration.route ?? null,
      shippingRoutes: declaration.shippingRoutes ?? null,
      skippedRoutes: declaration.skippedRoutes ?? null,
      parent: declaration.parent ?? null,
      state: declaration.state ?? null,
      drive: declaration.drive ?? null,
      ruling: declaration.ruling ?? null,
      reason: declaration.reason ?? null,
      exclusion: declaration.exclusion ?? null,
      contested: declaration.contested ?? null,
      capturedMarkup: capturedMarkupFrom(markup),
      captureDigest: digest(capturedMarkupFrom(markup)),
    };
    if (kind === "absent") {
      assertNoReachableRoute(row);
      assertText(row.ruling, "ruling", source);
      assertText(row.reason, "reason", source);
    }
    if (kind === "state" && row.route !== null) fail(`${source}: a state file may not own a route`);
    if (kind === "not-built-yet") {
      if (row.route !== null || row.drive !== null) fail(`${source}: not-built-yet page may not claim a reachable route or drive`);
      assertText(row.reason, "reason", source);
    }
    if (row.contested !== null) {
      if (typeof row.contested !== "object") fail(`${source}: contested must name a ruling and disagreeing spec text`);
      assertText(row.contested.ruling, "contested.ruling", source);
      assertText(row.contested.specText, "contested.specText", source);
    }
    rows.push(row);
  }
  assertDeletedAndExcludedRows(rows);
  assertStates(rows);
  const detected = rows.find((row) => row.page === "Onboarding Detected");
  if (!detected || detected.kind !== "absent" || !detected.ruling?.includes("D10(a)") || !/Onboarding Install/.test(detected.reason ?? "")) fail("Onboarding Detected must be absent and superseded by Onboarding Install under D10(a)");
  assertDistinctRoutedMarkup(rows);
  return rows.map(({ capturedMarkup, ...row }) => row);
}

async function main() {
  const record = process.argv.includes("--record");
  if (record) await recordSourceIndex();
  const manifest = await deriveShippingManifest();
  const counts = Object.fromEntries(["routed", "state", "absent"].map((kind) => [kind, manifest.filter((row) => row.kind === kind).length]));
  console.log(`TASK6890 d27=${JSON.stringify(D27_PACKAGE_SIZE_QUOTE)} source_files=${manifest.length} routed=${counts.routed} states=${counts.state} absent=${counts.absent}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
