import assert from "node:assert/strict";
import { mkdtemp, mkdir, rename, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { D27_FINAL_PAGE_LEAVES, D27_PACKAGE_SIZE_QUOTE, deriveShippingManifest, recordSourceIndex } from "./task-6890-shipping-manifest.mjs";

const absent = new Set([
  "Onboarding Detected", "Onboarding Apps", "Onboarding Silent Visible", "Onboarding Recovery Empty", "Onboarding Mullvad",
  "Non-product sheet",
  ...Array.from({ length: 13 }, (_, index) => `OSL Mail ${index + 1}`),
]);
const states = new Map([
  ["Home Empty", ["Home", "empty"]],
  ["Home Notifications Empty", ["Home", "notifications empty"]],
  ["Home Notifications Off", ["Home", "notifications off"]],
  ["Settings Account", ["Settings", "account"]],
  ["Settings Account Password", ["Settings", "password"]],
  ["Settings Account Recovery", ["Settings", "recovery"]],
  ["Settings Account Delete", ["Settings", "delete"]],
  ["Onboarding Pro Active", ["Onboarding Pro Code", "active"]],
]);

function declaration(page) {
  if (absent.has(page)) return {
    page, kind: "absent", route: null, drive: null,
    ruling: page === "Onboarding Detected" || page === "Onboarding Apps" || page === "Onboarding Silent Visible" || page === "Onboarding Recovery Empty" ? "D10(a)" : "D10(c)",
    reason: page === "Onboarding Detected" ? "Superseded by Onboarding Install." : `Excluded by ruling for ${page}.`,
    ...(page === "Non-product sheet" ? { exclusion: "non-product-sheet" } : {}),
  };
  if (states.has(page)) {
    const [parent, state] = states.get(page);
    return { page, kind: "state", route: null, parent, state, drive: `drive ${parent} to ${state}` };
  }
  return { page, kind: "routed", route: page.toLowerCase().replaceAll(" ", "-"), drive: `drive app to ${page}` };
}

async function fixture() {
  const root = await mkdtemp(path.join(os.tmpdir(), "task-6890-"));
  const pages = [
    "Home", "Home Empty", "Home Notifications Empty", "Home Notifications Off",
    "Settings", "Settings Account", "Settings Account Password", "Settings Account Recovery", "Settings Account Delete",
    "Onboarding Pro Code", "Onboarding Pro Active", "Onboarding Install", "Onboarding Detected", "Onboarding Apps", "Onboarding Silent Visible", "Onboarding Recovery Empty", "Onboarding Mullvad", "Non-product sheet",
    ...Array.from({ length: 13 }, (_, index) => `OSL Mail ${index + 1}`),
    ...Array.from({ length: 39 }, (_, index) => `Product Screen ${index + 1}`),
  ];
  assert.equal(pages.length, D27_FINAL_PAGE_LEAVES, D27_PACKAGE_SIZE_QUOTE);
  for (const page of pages) {
    const base = path.join(root, page);
    await writeFile(`${base}.dc.html`, `<script type="application/json" data-osl-shipping-manifest>${JSON.stringify(declaration(page))}</script><main data-page="${page}"><h1>${page}</h1></main>`);
  }
  await recordSourceIndex(root);
  return root;
}

test("TASK 6890 derives D27's 70 rows from source and rejects rename and shared capture", async (t) => {
  const root = await fixture();
  t.after(() => rm(root, { recursive: true, force: true }));
  const manifest = await deriveShippingManifest(root);
  assert.equal(manifest.length, D27_FINAL_PAGE_LEAVES);
  assert.equal(manifest.filter((row) => row.kind === "routed").length, 43);
  assert.equal(manifest.filter((row) => row.kind === "state").length, 8);
  assert.equal(manifest.filter((row) => row.kind === "absent").length, 19);

  await rename(path.join(root, "Home.dc.html"), path.join(root, "Home Renamed.dc.html"));
  await assert.rejects(() => deriveShippingManifest(root), /final source changed .*Home (?:Renamed\.dc\.html, Home\.dc\.html|\.dc\.html, Home Renamed\.dc\.html)/);
  await rename(path.join(root, "Home Renamed.dc.html"), path.join(root, "Home.dc.html"));
  const duplicate = `<script type="application/json" data-osl-shipping-manifest>${JSON.stringify(declaration("Product Screen 1"))}</script><main data-page="Home"><h1>Home</h1></main>`;
  await writeFile(path.join(root, "Product Screen 1.dc.html"), duplicate);
  await recordSourceIndex(root);
  await assert.rejects(() => deriveShippingManifest(root), /indistinguishable from/);
});
