#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  captureRetainedSurface,
  retainedCapabilityContracts,
  retainedRouteRegistry,
  retainedSurfaceFamilies,
  routeStateKey,
  type RetainedCapture,
  type RetainedPage,
  type RetainedTheme,
} from "../src/retained-redesign-6872";

interface RetainedManifest {
  schema_version: number;
  task: number;
  reviewed_package_commit: string;
  required_themes: RetainedTheme[];
  pages: RetainedPage[];
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, "..");
const workspaceRoot = resolve(appRoot, "../..");
const manifestPath = resolve(appRoot, "src/retained-redesign-manifest-6872.json");
const rosterPath = resolve(appRoot, "src/retained-redesign-package-roster-6873.json");
const capturePath = resolve(appRoot, "screenshots/task-6872-retained-captures.json");
const mutation = process.env.OSL6872_MUTATION ?? "";
const manifest = JSON.parse(readFileSync(manifestPath, "utf8")) as RetainedManifest;
const errors: string[] = [];

function check(condition: unknown, message: string): asserts condition {
  if (!condition) errors.push(message);
}

function applyMutation(): void {
  if (mutation === "starve-page") manifest.pages.shift();
  if (mutation === "duplicate-page") manifest.pages.push({ ...manifest.pages[0] });
  if (mutation === "starve-state") manifest.pages[0].state = "";
  if (mutation === "starve-width") manifest.pages.find((page) => page.disposition === "retained")!.required_windows_widths = [];
  if (mutation === "starve-theme") manifest.required_themes = ["dark"];
  if (mutation === "detach-capability") manifest.pages.find((page) => page.disposition === "retained")!.capability_feed = null;
}

applyMutation();

check(manifest.schema_version === 1, "schema_version must be 1");
check(manifest.task === 6872, "task must be 6872");
check(manifest.reviewed_package_commit === "08fbc20450b3f84bfc9a82b818e8ffc1c5f5f48c", "reviewed package commit changed");
check(manifest.pages.length === 194, `PAGE_COUNT expected=194 actual=${manifest.pages.length}`);
const ids = manifest.pages.map((page) => page.page_id);
check(new Set(ids).size === 194 && new Set(ids).size === ids.length, `UNIQUE_PAGE_IDS expected=194 actual=${new Set(ids).size} rows=${ids.length}`);
check(JSON.stringify(manifest.required_themes) === JSON.stringify(["dark", "high-contrast"]), `THEMES expected=dark,high-contrast actual=${manifest.required_themes.join(",")}`);

// TASK 6873: the manifest cannot be its own authority for "no page was omitted
// or resurrected".  The roster is pinned to the reviewed package tree, lives
// outside the manifest, and lets every inventory drift name the exact package
// page and route/state that moved.
interface RosterEntry {
  readonly page_id: string;
  readonly disposition: string;
  readonly route: string;
  readonly state: string;
  readonly capability_feed: string | null;
}
interface PackageRoster {
  readonly schema_version: number;
  readonly reviewed_package_commit: string;
  readonly page_count: number;
  readonly pages: readonly RosterEntry[];
}
const roster = JSON.parse(readFileSync(rosterPath, "utf8")) as PackageRoster;
check(roster.reviewed_package_commit === manifest.reviewed_package_commit, `ROSTER_COMMIT roster=${roster.reviewed_package_commit}`);
check(roster.page_count === 194 && roster.pages.length === 194, `ROSTER_STARVED expected=194 declared=${roster.page_count} actual=${roster.pages.length}`);
const packageTree = execFileSync("git", ["ls-tree", "-r", "--name-only", roster.reviewed_package_commit, "apps/osl-hub-ui/screenshots"], {
  cwd: workspaceRoot,
  encoding: "utf8",
  stdio: ["ignore", "pipe", "pipe"],
})
  .split("\n")
  .filter((path) => path.endsWith(".png"));
check(packageTree.length === 194, `PACKAGE_TREE expected=194 actual=${packageTree.length}`);
const rosterById = new Map(roster.pages.map((entry) => [entry.page_id, entry]));
for (const path of packageTree) check(rosterById.has(path), `ROSTER_PAGE_MISSING page=${path}`);

const manifestById = new Map<string, RetainedPage>();
const seenIds = new Set<string>();
for (const page of manifest.pages) {
  if (seenIds.has(page.page_id)) errors.push(`PACKAGE_PAGE_DUPLICATED page=${page.page_id} state=${routeStateKey(page)}`);
  seenIds.add(page.page_id);
  if (!manifestById.has(page.page_id)) manifestById.set(page.page_id, page);
}
for (const entry of roster.pages) {
  const page = manifestById.get(entry.page_id);
  const state = `${entry.route}#${entry.state}`;
  if (!page) {
    errors.push(`PACKAGE_PAGE_OMITTED page=${entry.page_id} state=${state}`);
    continue;
  }
  check(page.disposition === entry.disposition, `PACKAGE_PAGE_RECLASSIFIED page=${entry.page_id} state=${state} roster=${entry.disposition} manifest=${page.disposition}`);
  check(routeStateKey(page) === state, `PACKAGE_PAGE_STATE_CHANGED page=${entry.page_id} roster=${state} manifest=${routeStateKey(page)}`);
  check(page.capability_feed === entry.capability_feed, `PACKAGE_PAGE_CAPABILITY_CHANGED page=${entry.page_id} state=${state} roster=${entry.capability_feed} manifest=${page.capability_feed}`);
}
for (const id of seenIds) check(rosterById.has(id), `PACKAGE_PAGE_UNKNOWN page=${id}`);

const expectedDispositions = new Map([
  ["retained", 163],
  ["deleted-d1", 2],
  ["deleted-d4", 5],
  ["chats-strip-follow-up", 24],
]);
for (const [disposition, expected] of expectedDispositions) {
  const actual = manifest.pages.filter((page) => page.disposition === disposition).length;
  check(actual === expected, `DISPOSITION ${disposition} expected=${expected} actual=${actual}`);
}

for (const page of manifest.pages) {
  check(page.page_id === page.source_image.path, `SOURCE_PATH page=${page.page_id}`);
  check(page.source_image.commit === manifest.reviewed_package_commit, `SOURCE_COMMIT page=${page.page_id}`);
  check(/^[0-9a-f]{40}$/u.test(page.source_image.blob), `SOURCE_BLOB page=${page.page_id}`);
  check(Number.isSafeInteger(page.source_image.width) && page.source_image.width >= 480, `SOURCE_WIDTH page=${page.page_id}`);
  check(Number.isSafeInteger(page.source_image.height) && page.source_image.height >= 220, `SOURCE_HEIGHT page=${page.page_id}`);
  check(page.route.startsWith("/") && page.state.trim() !== "", `ROUTE_STATE page=${page.page_id}`);
  check(page.required_windows_widths.length > 0, `WIDTH_STARVED page=${page.page_id}`);
  check(page.required_windows_widths.includes(page.source_image.width), `SOURCE_WIDTH_MISSING page=${page.page_id} width=${page.source_image.width}`);
  try {
    const actualBlob = execFileSync("git", ["rev-parse", `${page.source_image.commit}:${page.source_image.path}`], {
      cwd: workspaceRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
    check(actualBlob === page.source_image.blob, `SOURCE_BLOB_MISMATCH page=${page.page_id}`);
  } catch {
    errors.push(`SOURCE_IMAGE_UNREADABLE page=${page.page_id}`);
  }
}

const retained = manifest.pages.filter((page) => page.disposition === "retained");
const deleted = manifest.pages.filter((page) => page.disposition === "deleted-d1" || page.disposition === "deleted-d4");
const routeRegistry = retainedRouteRegistry(manifest.pages);
if (mutation === "restore-deleted") {
  (routeRegistry as Set<string>).add(routeStateKey(deleted[0]));
}
for (const page of retained) check(routeRegistry.has(routeStateKey(page)), `RETAINED_ROUTE_MISSING page=${page.page_id}`);
for (const page of deleted) check(!routeRegistry.has(routeStateKey(page)), `DELETED_ROUTE_REACHABLE page=${page.page_id} route=${routeStateKey(page)}`);

const routes = new Set(retained.map((page) => page.route));
for (const route of ["/home", "/friends", "/calls", "/settings", "/privacy", "/onboarding", "/recovery", "/story", "/post", "/modal"]) {
  check(routes.has(route), `RETAINED_FAMILY_ROUTE_MISSING route=${route}`);
}

for (const [feed, contract] of Object.entries(retainedCapabilityContracts)) {
  if (contract.source === null || contract.symbol === null) continue;
  const source = readFileSync(resolve(appRoot, contract.source), "utf8");
  check(source.includes(contract.symbol), `CAPABILITY_SYMBOL_MISSING feed=${feed} source=${contract.source} symbol=${contract.symbol}`);
}

const captures: RetainedCapture[] = [];
for (const page of retained) {
  check(page.capability_feed !== null, `CAPABILITY_FEED_MISSING page=${page.page_id}`);
  check(page.capability_feed !== null && page.capability_feed in retainedCapabilityContracts, `CAPABILITY_FEED_UNKNOWN page=${page.page_id} feed=${page.capability_feed}`);
  if (page.capability_feed === null || !(page.capability_feed in retainedCapabilityContracts)) continue;
  for (const width of page.required_windows_widths) {
    for (const theme of manifest.required_themes) {
      captures.push(captureRetainedSurface(page, width, theme));
    }
  }
}

if (mutation === "starve-a11y" && captures[0]) captures[0].accessibility.nodes.pop();
if (mutation === "screenshot-only" && captures[0]) {
  captures[0].visual.html = `<img src="${captures[0].page_id}" alt="capture">`;
}

const expectedCaptureCount = retained.reduce((sum, page) => sum + page.required_windows_widths.length * manifest.required_themes.length, 0);
check(captures.length === expectedCaptureCount, `CAPTURE_COUNT expected=${expectedCaptureCount} actual=${captures.length}`);
check(new Set(captures.map((capture) => capture.capture_id)).size === captures.length, "CAPTURE_IDS_NOT_UNIQUE");

for (const capture of captures) {
  const html = capture.visual.html;
  const prefix = `CAPTURE page=${capture.page_id} width=${capture.width} theme=${capture.theme}`;
  check(capture.visual.renderer === "retained-redesign-6872", `${prefix} renderer`);
  check(capture.visual.paint_commands.length >= 4, `${prefix} visual-starved`);
  check(!/<img\b|background-image/iu.test(html), `${prefix} screenshot-only-markup`);
  check(/<nav\b[^>]*aria-label=/u.test(html), `${prefix} navigation-semantic`);
  check(/<main\b[^>]*role=/u.test(html), `${prefix} main-semantic`);
  check(/<h1>/u.test(html), `${prefix} heading-semantic`);
  check(/<button\b[^>]*type="button"/u.test(html), `${prefix} control-semantic`);
  check(html.includes(`data-retained-theme="${capture.theme}"`), `${prefix} theme-attribute`);
  check(html.includes(`data-route-state="${capture.route_state}"`), `${prefix} route-state-attribute`);
  check(/data-capability-feed=/u.test(html), `${prefix} capability-feed`);
  check(/data-capability-origin="subsystem"/u.test(html), `${prefix} capability-origin`);
  check(/data-capability-status="unavailable"/u.test(html), `${prefix} honest-unavailable`);
  check(/disabled aria-disabled="true"/u.test(html), `${prefix} unavailable-action-enabled`);
  const roles = new Set(capture.accessibility.nodes.map((node) => node.role));
  check(capture.accessibility.format === "deterministic-ax-v1", `${prefix} accessibility-format`);
  check(capture.accessibility.nodes.length >= 5, `${prefix} accessibility-starved`);
  check(roles.has("navigation") && roles.has("heading") && roles.has("button") && (roles.has("main") || roles.has("dialog")), `${prefix} accessibility-roles`);
  check(capture.accessibility.nodes.some((node) => node.live), `${prefix} accessibility-live-region`);
}

const coveredFamilies = new Set(captures.map((capture) => /class="retained-kicker">([^<]+)/u.exec(capture.visual.html)?.[1]));
// The shell is shared by every retained surface rather than owned by one
// package page, so one successful semantic capture proves it as well.
if (captures.length > 0 && captures.every((capture) => capture.visual.html.includes('class="retained-shell"'))) coveredFamilies.add("shell");
for (const family of retainedSurfaceFamilies) check(coveredFamilies.has(family), `SURFACE_FAMILY_MISSING family=${family}`);

const css = readFileSync(resolve(appRoot, "src/retained-redesign-6872.css"), "utf8");
for (const token of ["var(--bg)", "var(--panel)", "var(--text)", "var(--brand)"]) check(css.includes(token), `SHIPPED_TOKEN_MISSING token=${token}`);
check(css.includes('data-retained-theme="high-contrast"'), 'HIGH_CONTRAST_STYLE_MISSING theme=high-contrast selector=[data-retained-theme="high-contrast"]');
check(css.includes("forced-colors: active"), "WINDOWS_FORCED_COLORS_MISSING state=windows-forced-colors");
check(css.includes("prefers-reduced-motion: reduce"), "REDUCED_MOTION_MISSING state=prefers-reduced-motion");

if (errors.length > 0) {
  for (const error of errors.slice(0, 40)) console.error(`TASK6872_FAIL ${error}`);
  console.error(`TASK6872_RESULT status=red errors=${errors.length} mutation=${mutation || "none"}`);
  process.exitCode = 1;
} else {
  const capturePayload = {
    schema_version: 1,
    task: 6872,
    manifest_sha256: createHash("sha256").update(readFileSync(manifestPath)).digest("hex"),
    summary: {
      package_pages: manifest.pages.length,
      retained_pages: retained.length,
      deleted_pages: deleted.length,
      chats_strip_pages: manifest.pages.filter((page) => page.disposition === "chats-strip-follow-up").length,
      visual_captures: captures.length,
      accessibility_captures: captures.length,
      widths: [...new Set(captures.map((capture) => capture.width))].sort((left, right) => left - right),
      themes: manifest.required_themes,
      deleted_route_probes: deleted.length,
      capability_feeds: [...new Set(retained.map((page) => page.capability_feed))].sort(),
    },
    captures,
  };
  writeFileSync(capturePath, `${JSON.stringify(capturePayload, null, 2)}\n`, "utf8");
  console.log(`TASK6872_MANIFEST pages=${manifest.pages.length} unique=${new Set(ids).size} retained=${retained.length} deleted_d1=2 deleted_d4=5 chats_strip=24`);
  console.log(`TASK6872_CAPTURES visual=${captures.length} accessibility=${captures.length} widths=${capturePayload.summary.widths.join(",")} themes=${manifest.required_themes.join(",")}`);
  console.log(`TASK6872_ROUTES deleted_probes=${deleted.length} reachable_deleted=0 capability_feeds=${capturePayload.summary.capability_feeds.length}`);
  console.log(`TASK6872_RENDER semantic_controls=all screenshot_only=0 families=${retainedSurfaceFamilies.length}`);
  console.log("TASK6872_RESULT status=green errors=0");
}
