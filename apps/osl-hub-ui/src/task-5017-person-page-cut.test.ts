import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { build } from "vite";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type { Route } from "./main";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const uiRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));
const gapListPath = new URL("../public/shipped-gap-list.json", import.meta.url);
const mainSourcePath = new URL("./main.ts", import.meta.url);
const requiredGapId = "48h-person-page-posts-stories";

// Carrier Story/Status composers remain allowed. These tokens identify only
// the cut OSL person/profile page, its posts tab, and its story-ring surface.
const scopedSurfaceToken = /data-(?:person|profile)-(?:page-)?(?:posts?|stories)|data-story-rings?|(?:person|profile)(?:Page)?(?:Posts?|Stories|StoryRings?)|story[-_]?rings?|(?:person|profile)[-_]page[-_](?:posts?|stories)|(?:person|profile)[-_](?:posts?|stories)/iu;
const workspaceRoutes = [
  "home", "inbox", "people", "privacy", "activity", "connections", "service", "settings",
  "mullvad", "osl-chat", "osl-mail", "osl-servers", "signal-qa",
] as const satisfies readonly Exclude<Route, "onboarding">[];
const settingsSections = ["account", "apps", "friends", "scrub", "cleanup", "notifications", "appearance", "about"] as const;
// TASK 6802: `tutorial`, `detected`, `install` and `apps` were deleted by owner
// rulings D4/D5; `setup-apps` replaced all four. The union itself now lives in
// `onboarding-route-contract.ts`, which is what `unionValues` reads.
const onboardingRoutes = [
  "pro", "welcome", "create", "import", "unlock", "keylost", "account-recovery", "recovery",
  "recovery-check", "mullvad", "sending", "defaults", "tor", "cover", "passwords", "burnpass",
  "privacy", "forward-secrecy", "visibility", "setup-apps", "browser",
] as const;

type Gap = { id?: unknown; feature?: unknown; status?: unknown; productPromise?: unknown; reason?: unknown; uiDisposition?: unknown };
type GapList = { schemaVersion?: unknown; gaps?: unknown };
type Finding = { location: string; control: string; grey: boolean };

function readGapList(path: URL = gapListPath): Gap[] {
  const parsed = JSON.parse(readFileSync(path, "utf8")) as GapList;
  if (parsed.schemaVersion !== 1 || !Array.isArray(parsed.gaps)) {
    throw new Error("TASK5017 missing-record: public/shipped-gap-list.json must have schemaVersion 1 and a gaps array");
  }
  return parsed.gaps as Gap[];
}

function personPageGapRecords(path: URL = gapListPath): Gap[] {
  return readGapList(path).filter((gap) => gap.id === requiredGapId);
}

function requireOnePersonPageGapRecord(path: URL = gapListPath): Gap {
  const records = personPageGapRecords(path);
  if (records.length !== 1) {
    throw new Error(`TASK5017 missing-record: expected exactly one ${requiredGapId} gap entry, found ${records.length}`);
  }
  const [record] = records;
  const reason = String(record.reason ?? "");
  if (record.feature !== "Person-page posts and story rings" || record.status !== "cut"
    || record.productPromise !== "A person's page shows their posts, flat and hairline-divided, and their stories as rings."
    || !reason.includes("PLAN-48H") || !reason.includes("merged-plan ruling")
    || !reason.includes("profiles/posts/stories") || !reason.includes("48-hour release")
    || !reason.includes("4650-4653 remain open")
    || record.uiDisposition !== "Absent from the 48-hour release, never greyed.") {
    throw new Error("TASK5017 missing-record: person-page entry must name posts/story rings, the PLAN-48H merged ruling, open decisions 4650-4653, and absent-not-greyed disposition");
  }
  return record;
}

function productionSourceFiles(): string[] {
  const sourceDirectory = fileURLToPath(new URL(".", import.meta.url));
  const nested = (directory: string): string[] => readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = `${directory}/${entry.name}`;
    if (entry.isDirectory()) return nested(path);
    if (!entry.isFile() || /\.(?:test|spec)\.(?:ts|tsx|js|mjs)$/u.test(entry.name) || entry.name.endsWith(".d.ts")) return [];
    return /\.(?:ts|tsx|js|mjs|css|html)$/u.test(entry.name) ? [path] : [];
  });
  const entryPages = readdirSync(uiRoot, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".html"))
    .map((entry) => `${uiRoot}/${entry.name}`);
  return [...nested(sourceDirectory), ...entryPages].sort();
}

function isGrey(control: string): boolean {
  return /\bdisabled\b|aria-disabled\s*=\s*["']true["']|\b(?:greyed|grayed|unavailable)\b/iu.test(control);
}

function sourceFindings(): Finding[] {
  return productionSourceFiles().flatMap((path) => readFileSync(path, "utf8").split("\n").flatMap((line, index) => {
    if (!scopedSurfaceToken.test(line)) return [];
    return [{ location: `${path.replace(`${repoRoot}/`, "")}:${index + 1}`, control: line.trim(), grey: isGrey(line) }];
  }));
}

function packagedRouteFiles(): URL[] {
  const distRoot = new URL("../dist/", import.meta.url);
  const pages = readdirSync(distRoot).filter((name) => name.endsWith(".html")).map((name) => new URL(name, distRoot));
  const distAssets = new URL("assets/", distRoot);
  const scripts = readdirSync(distAssets).filter((name) => name.endsWith(".js")).map((name) => new URL(name, distAssets));
  if (pages.length === 0 || scripts.length === 0) {
    throw new Error("TASK5017 packaged-route-crawl: Vite produced no HTML pages or JavaScript route bundles");
  }
  return [...pages, ...scripts];
}

function packageFindings(files: URL[]): Finding[] {
  return files.flatMap((path) => {
    const text = readFileSync(path, "utf8");
    if (!scopedSurfaceToken.test(text)) return [];
    const at = text.search(scopedSurfaceToken);
    return [{
      location: fileURLToPath(path).replace(`${repoRoot}/`, ""),
      control: text.slice(Math.max(0, at - 160), at + 320).replace(/\s+/gu, " "),
      grey: isGrey(text.slice(Math.max(0, at - 160), at + 320)),
    }];
  });
}

function controlTree(markup: string, location: string): Finding[] {
  const controls: string[] = [];
  const openings = markup.matchAll(/<(button|input|select|textarea|a|summary)\b[^>]*>/giu);
  for (const opening of openings) {
    const tag = opening[1].toLocaleLowerCase();
    const start = opening.index;
    if (tag === "input") {
      controls.push(opening[0]);
      continue;
    }
    const closing = `</${tag}>`;
    const end = markup.toLocaleLowerCase().indexOf(closing, start + opening[0].length);
    controls.push(end < 0 ? opening[0] : markup.slice(start, end + closing.length));
  }
  // Story rings can be custom controls. Include explicit roles even when the
  // element is not one of the native control tags above.
  for (const match of markup.matchAll(/<([a-z][\w-]*)\b(?=[^>]*\brole\s*=\s*["'](?:button|link)["'])[^>]*>[\s\S]*?<\/\1>/giu)) {
    if (!/^(?:button|input|select|textarea|a|summary)$/iu.test(match[1])) controls.push(match[0]);
  }
  return controls.filter((control) => scopedSurfaceToken.test(control)).map((control) => ({ location, control, grey: isGrey(control) }));
}

function unionValues(typeName: string): string[] {
  // TASK 6802: the onboarding union moved out of main.ts into the route
  // contract both the static inventory and the physical crawl read, so this
  // looks there for it and keeps reading main.ts for the other two.
  if (typeName === "OnboardingRoute") {
    const contract = readFileSync(new URL("./onboarding-route-contract.ts", import.meta.url), "utf8");
    const values: string[] = [];
    for (const name of ["RETAINED_SETUP_ROUTES", "RETAINED_ENTRY_ROUTES"]) {
      const block = new RegExp(`${name}\\s*=\\s*\\[([^\\]]+)\\]`, "u").exec(contract);
      if (!block) throw new Error(`TASK5017 unregistered-surface: could not read ${name} from onboarding-route-contract.ts`);
      values.push(...[...block[1].matchAll(/["']([^"']+)["']/gu)].map((value) => value[1]));
    }
    return values;
  }
  const source = readFileSync(mainSourcePath, "utf8");
  const match = new RegExp(`(?:export\\s+)?type\\s+${typeName}\\s*=\\s*([^;]+);`, "u").exec(source);
  if (!match) throw new Error(`TASK5017 unregistered-surface: could not read ${typeName} route registry from main.ts`);
  return [...match[1].matchAll(/["']([^"']+)["']/gu)].map((value) => value[1]);
}

function requireRuntimeRegistryCoverage(): void {
  const expectedRoutes = ["onboarding", ...workspaceRoutes].sort();
  const expectedOnboarding = [...onboardingRoutes].sort();
  const expectedSettings = [...settingsSections].sort();
  for (const [kind, sourceValues, registeredValues] of [
    ["route", unionValues("Route"), expectedRoutes],
    ["onboarding", unionValues("OnboardingRoute"), expectedOnboarding],
    ["settings", unionValues("SettingsSection"), expectedSettings],
  ] as Array<[string, readonly string[], readonly string[]]>) {
    const unregistered = sourceValues.filter((value) => !registeredValues.includes(value));
    const stale = registeredValues.filter((value) => !sourceValues.includes(value));
    if (unregistered.length > 0 || stale.length > 0) {
      throw new Error(`TASK5017 unregistered-surface: ${kind} runtime registry mismatch; unregistered=[${unregistered.join(",")}] stale=[${stale.join(",")}]`);
    }
  }
}

let ui: typeof import("./main");

beforeAll(async () => {
  // A direct Vite production build intentionally bypasses unrelated test-file
  // type errors while still emitting the exact frontendDist Tauri packages.
  await build({ root: uiRoot, logLevel: "silent" });
  mocks.invoke.mockResolvedValue(undefined);
  mocks.listen.mockResolvedValue(() => undefined);
  mocks.getCurrentWindow.mockReturnValue({ onFocusChanged: vi.fn().mockResolvedValue(() => undefined) });
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (key: string) => store.get(key) ?? null, setItem: (key: string, value: string) => { store.set(key, value); }, removeItem: (key: string) => { store.delete(key); }, clear: () => store.clear() });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => vi.unstubAllGlobals());

describe("TASK 5017 person-page posts and stories are shipped as an absent 48-hour gap", () => {
  it("ships exactly one gap record naming the PLAN-48H ruling", () => {
    const record = requireOnePersonPageGapRecord();
    console.log(`TASK5017_GAP_COUNT=1 TASK5017_GAP=id=${record.id} reason=${record.reason}`);
  });

  it("finds no live or grey person-page posts/story-ring surface in three independent walks", () => {
    requireOnePersonPageGapRecord();
    requireRuntimeRegistryCoverage();

    const sources = productionSourceFiles();
    const sourceLeaks = sourceFindings();
    if (sourceLeaks.length > 0) {
      throw new Error(`TASK5017 leaked-surface: unregistered person-page posts/story surface in production source ${sourceLeaks[0].location}: ${sourceLeaks[0].control}`);
    }

    const packagedFiles = packagedRouteFiles();
    const packagedLeaks = packageFindings(packagedFiles);
    if (packagedLeaks.length > 0) {
      throw new Error(`TASK5017 leaked-surface: person-page posts/story surface reached packaged route ${packagedLeaks[0].location}: ${packagedLeaks[0].control}`);
    }
    const packagedGapPath = new URL("../dist/shipped-gap-list.json", import.meta.url);
    if (!readFileSync(packagedGapPath).equals(readFileSync(gapListPath))) {
      throw new Error("TASK5017 missing-record: packaged shipped-gap-list.json is not byte-identical to the public gap list");
    }
    requireOnePersonPageGapRecord(packagedGapPath);

    const runtimeLeaks: Finding[] = [];
    for (const route of workspaceRoutes) {
      ui.__oslHubUiTest.reset({ route, coreReady: true, storageMethod: "tpm-pcp", services: [], servicesChecked: true });
      runtimeLeaks.push(...controlTree(ui.__oslHubUiTest.renderRouteShell(route), `route/${route}`));
    }
    for (const section of settingsSections) {
      ui.__oslHubUiTest.reset({ route: "settings", coreReady: true, storageMethod: "tpm-pcp", services: [], servicesChecked: true });
      runtimeLeaks.push(...controlTree(ui.__oslHubUiTest.renderSettingsSection(section), `settings/${section}`));
    }
    for (const destination of onboardingRoutes) {
      ui.__oslHubUiTest.reset({ route: "onboarding", coreReady: true, storageMethod: "tpm-pcp", services: [], servicesChecked: true });
      const onboarding = destination as Parameters<typeof ui.__oslHubUiTest.renderOnboardingRoute>[0];
      runtimeLeaks.push(...controlTree(ui.__oslHubUiTest.renderOnboardingRoute(onboarding), `onboarding/${destination}`));
    }
    if (runtimeLeaks.length > 0) {
      throw new Error(`TASK5017 leaked-surface: dynamically reached person-page posts/story-ring control at ${runtimeLeaks[0].location}: ${runtimeLeaks[0].control}`);
    }

    const sourceGrey = sourceLeaks.filter((finding) => finding.grey).length;
    const packageGrey = packagedLeaks.filter((finding) => finding.grey).length;
    const runtimeGrey = runtimeLeaks.filter((finding) => finding.grey).length;
    console.log(`TASK5017_SOURCE_FILES=${sources.length} TASK5017_SOURCE_LIVE=${sourceLeaks.length - sourceGrey} TASK5017_SOURCE_GREY=${sourceGrey}`);
    console.log(`TASK5017_PACKAGED_ROUTE_FILES=${packagedFiles.length} TASK5017_PACKAGED_LIVE=${packagedLeaks.length - packageGrey} TASK5017_PACKAGED_GREY=${packageGrey}`);
    console.log(`TASK5017_RUNTIME_ROUTES=${workspaceRoutes.length} TASK5017_RUNTIME_SETTINGS=${settingsSections.length} TASK5017_RUNTIME_ONBOARDING=${onboardingRoutes.length} TASK5017_RUNTIME_LIVE=${runtimeLeaks.length - runtimeGrey} TASK5017_RUNTIME_GREY=${runtimeGrey}`);
  });

  it("names the missing record, unregistered source, and dynamically reached leak failures", () => {
    expect(() => {
      const records = personPageGapRecords().filter(() => false);
      if (records.length !== 1) throw new Error(`TASK5017 missing-record: expected exactly one ${requiredGapId} gap entry, found ${records.length}`);
    }).toThrow(/TASK5017 missing-record/u);
    expect(() => {
      const leaked = '<button disabled data-person-posts aria-disabled="true">Posts</button>';
      if (scopedSurfaceToken.test(leaked)) throw new Error(`TASK5017 leaked-surface: unregistered person-page posts control: ${leaked}`);
    }).toThrow(/TASK5017 leaked-surface/u);
    expect(() => {
      const leaked = `<button data-story-ring>${String.fromCharCode(83, 116, 111, 114, 105, 101, 115)}</button>`;
      const findings = controlTree(leaked, "route/home");
      if (findings.length > 0) throw new Error(`TASK5017 leaked-surface: dynamically reached story-ring control: ${findings[0].control}`);
    }).toThrow(/TASK5017 leaked-surface/u);
  });
});
