/**
 * TASK 7049b — every case runs the real TASK 7049 executable against a fresh
 * copy of a D27 package plus its shipping renderer.  These are deliberately
 * filesystem copies, rather than calls into the validation helpers, so a
 * route/page/accounting check accidentally removed from the CLI turns this
 * proof red.
 */
import assert from "node:assert/strict";
import { access, cp, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { D27_FINAL_PAGE_LEAVES, recordSourceIndex } from "./task-6890-shipping-manifest.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const gate = path.join(here, "task-7049-route-design-manifest.mjs");

const absent = new Set([
  "Onboarding Detected", "Onboarding Apps", "Onboarding Silent Visible", "Onboarding Recovery Empty", "Onboarding Mullvad",
  "Non-product sheet", ...Array.from({ length: 13 }, (_, index) => `OSL Mail ${index + 1}`),
]);
const states = new Map([
  ["Home Empty", ["Home", "empty"]],
  ["Home Notifications Empty", ["Home", "notifications empty"]],
  ["Home Notifications Off", ["Home", "notifications off"]],
  ["Settings Account", ["Settings Page", "account"]],
  ["Settings Account Password", ["Settings Page", "password"]],
  ["Settings Account Recovery", ["Settings Page", "recovery"]],
  ["Settings Account Delete", ["Settings Page", "delete"]],
  ["Onboarding Pro Active", ["Onboarding Pro Code", "active"]],
]);
const routed = ["Home", "Inbox", "Onboarding Pro Code", "Onboarding Install", "Settings Page", "Settings Apps Page"];
const requiredMutants = Object.freeze([
  "hidden-route", "shared-page", "d10a-route", "dropped-accounting", "skipped-unreferenced", "state-own-route",
]);
const executedMutants = new Set();

function declaration(page) {
  if (absent.has(page)) return {
    page, kind: "absent", route: null, drive: null,
    ruling: /^(Onboarding Detected|Onboarding Apps|Onboarding Silent Visible|Onboarding Recovery Empty)$/.test(page) ? "D10(a)" : "D10(c)",
    reason: page === "Onboarding Detected" ? "Superseded by Onboarding Install." : `Excluded by ruling for ${page}.`,
    ...(page === "Non-product sheet" ? { exclusion: "non-product-sheet" } : {}),
  };
  if (states.has(page)) {
    const [parent, state] = states.get(page);
    return { page, kind: "state", route: null, parent, state, drive: `drive ${parent} to ${state}` };
  }
  const route = page === "Home" ? "home"
    : page === "Inbox" ? "inbox"
      : page === "Onboarding Pro Code" ? "onboarding/welcome"
        : page === "Onboarding Install" ? "onboarding/install"
          : page === "Settings Page" ? "settings/account" : "settings/apps";
  return { page, kind: "routed", route, drive: `drive app to ${page}` };
}

async function writePage(root, page, value = declaration(page)) {
  await writeFile(path.join(root, `${page}.dc.html`), `<script type="application/json" data-osl-shipping-manifest>${JSON.stringify(value)}</script><main data-page="${page}"><h1>${page}</h1></main>`);
}

async function fixture(root) {
  const pages = [
    ...routed, ...states.keys(), "Onboarding Detected", "Onboarding Apps", "Onboarding Silent Visible", "Onboarding Recovery Empty", "Onboarding Mullvad", "Non-product sheet",
    ...Array.from({ length: 13 }, (_, index) => `OSL Mail ${index + 1}`),
    ...Array.from({ length: 37 }, (_, index) => `Unbuilt ${index + 1}`),
  ];
  assert.equal(pages.length, D27_FINAL_PAGE_LEAVES);
  for (const page of pages) {
    if (page.startsWith("Unbuilt ")) await writePage(root, page, { page, kind: "not-built-yet", route: null, drive: null, reason: "Not built in D27." });
    else await writePage(root, page);
  }
  await writeFile(path.join(root, "renderer-runtime.ts"), `export type Route = "home" | "inbox" | "onboarding" | "settings";\ntype OnboardingRoute = "welcome" | "install";\ntype SettingsSection = "account" | "apps";\n`);
  await recordSourceIndex(root);
}

async function readDeclaration(root, page) {
  const source = await readFile(path.join(root, `${page}.dc.html`), "utf8");
  return JSON.parse(/<script[^>]*data-osl-shipping-manifest[^>]*>([\s\S]*?)<\/script>/.exec(source)[1]);
}

async function mutateDeclaration(root, page, mutate) {
  const value = await readDeclaration(root, page);
  await writePage(root, page, mutate(value));
  await recordSourceIndex(root);
}

function run(root) {
  return spawnSync(process.execPath, [gate], {
    encoding: "utf8",
    env: { ...process.env, OSL_7049_DESIGN_ROOT: root, OSL_7049_RENDERER: path.join(root, "renderer-runtime.ts") },
  });
}

async function inCopy(base, name, mutate, expected) {
  const root = await mkdtemp(path.join(os.tmpdir(), `task-7049b-${name}-`));
  try {
    await cp(base, root, { recursive: true });
    await mutate(root);
    executedMutants.add(name);
    const result = run(root);
    assert.equal(result.status, 1, `${name} unexpectedly passed: ${result.stdout}${result.stderr}`);
    assert.match(`${result.stdout}${result.stderr}`, expected, `${name} did not name its missing route/page`);
    console.log(`TASK7049B_MUTANT_RED name=${name} exit=${result.status} output=${JSON.stringify(`${result.stdout}${result.stderr}`.trim())}`);
  } finally {
    await rm(root, { recursive: true, force: true });
    assert.equal(await access(root).then(() => true, () => false), false, `${name} throwaway remained`);
  }
}

test("TASK 7049b refuses every route/page mutation and restores reproducible accounting", async (t) => {
  const base = await mkdtemp(path.join(os.tmpdir(), "task-7049b-base-"));
  try {
    await fixture(base);
    const first = run(base);
    const second = run(base);
    assert.equal(first.status, 0, first.stderr);
    assert.equal(second.status, 0, second.stderr);
    assert.equal(first.stdout, second.stdout, "untouched accounting changed between consecutive runs");
    assert.match(first.stdout, /routes=6 pairs=6 unreferenced=0 accounting=\{"routed":6,"state":8,"deleted-D10\(a\)":4,"excluded-D10\(c\)":15,"not-built-yet":37\}/);
    console.log(`TASK7049B_UNTOUCHED_GREEN consecutive=2 accounting=${JSON.stringify(first.stdout.trim())}`);

    await inCopy(base, "hidden-route", async (root) => {
      const renderer = path.join(root, "renderer-runtime.ts");
      await writeFile(renderer, (await readFile(renderer, "utf8")).replace(' | "inbox"', ""));
    }, /route count fell[\s\S]*omitted route: "inbox"/);
    await inCopy(base, "shared-page", async (root) => {
      await mutateDeclaration(root, "Home", (row) => ({ ...row, shippingRoutes: ["home", "inbox"] }));
      await mutateDeclaration(root, "Inbox", (row) => ({ ...row, route: "aux" }));
    }, /routes "home", "inbox" point at design page "Home"/);
    await inCopy(base, "d10a-route", (root) => mutateDeclaration(root, "Onboarding Detected", (row) => ({ ...row, route: "onboarding/detected" })), /Onboarding Detected[\s\S]*reachable route/);
    await inCopy(base, "dropped-accounting", (root) => mutateDeclaration(root, "Unbuilt 37", (row) => ({ ...row, kind: "skipped" })), /Unbuilt 37\.dc\.html[\s\S]*kind must be routed, state, absent, or not-built-yet/);
    await inCopy(base, "skipped-unreferenced", (root) => mutateDeclaration(root, "Inbox", (row) => ({ ...row, skippedRoutes: ["unreferenced"] })), /Inbox\.dc\.html[\s\S]*routes may not be marked skipped[\s\S]*"unreferenced"/);
    await inCopy(base, "state-own-route", (root) => mutateDeclaration(root, "Home Empty", (row) => ({ ...row, route: "home-empty" })), /Home Empty\.dc\.html[\s\S]*state file may not own a route/);
    const starved = requiredMutants.filter((name) => !executedMutants.has(name));
    assert.equal(starved.length, 0, `TASK7049B proof starved; absent mutant=${starved.join(",")}`);
    assert.equal(executedMutants.size, requiredMutants.length, "TASK7049B proof ran an unexpected mutant set");
  } finally {
    await rm(base, { recursive: true, force: true });
    assert.equal(await access(base).then(() => true, () => false), false, "base throwaway remained");
    console.log("TASK7049B_THROWAWAYS_REMOVED=true");
  }
});
