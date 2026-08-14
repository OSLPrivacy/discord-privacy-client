#!/usr/bin/env node
// TASK 6841 — the exhaustive user-facing scan half of the TASK 6840 check.
//
// TASK 6840 renamed the misleading `PLAINTEXT ATTACHMENT` composer control to
// `TEXT ATTACHMENT` and put the sentence "Fully encrypted like every OSL
// attachment." beside it. Its evidence quoted a one-off shell scan that was
// never committed, so the rename had no re-runnable completeness check. This
// script is that check, and unlike a bare grep it names the surface it is
// unhappy about so a mutation can be attributed:
//
//     TASK6841_RED surface=<repo-relative path> class=<...> slot=<...> detail=<...>
//
// Classes:
//   stale-label   a resource surface still says PLAINTEXT, or lost the new name
//   disclosure    a resource surface lost the "fully encrypted" guidance
//   locale        a localisation bundle carries a stale label / lacks the rename
//   missing       a resource surface that must carry the rename is absent
//
// Usage: node scan-6840.mjs [treeRoot]   (default: cwd)

import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, relative, sep, posix } from "node:path";
import { resolve } from "node:path";

const treeRoot = resolve(process.argv[2] ?? process.cwd());

const LABEL = "TEXT ATTACHMENT";
const GUIDANCE_TEXT = "fully encrypted like every OSL attachment";
const A11Y_NAME = "TEXT ATTACHMENT — fully encrypted like every OSL attachment";
// The label this rename retired. Matched case-insensitively and across the
// separators a resource file might use (space, underscore, hyphen, newline).
const STALE_LABEL = /\bPLAINTEXT[\s_-]+ATTACHMENT\b/giu;

const failures = [];
const fail = (surface, cls, slot, detail) =>
  failures.push({ surface, class: cls, slot, detail });

const norm = (text) => text.replace(/\s+/gu, " ");
const countOf = (haystack, needle) => haystack.split(needle).length - 1;
const countRe = (haystack, re) => (haystack.match(re) ?? []).length;

// ---------------------------------------------------------------------------
// 1. The named resource surfaces that must carry the rename.
// ---------------------------------------------------------------------------

const composerExpectations = (role) => ({
  role,
  slots: [
    { slot: "visible-label", cls: "stale-label", needle: `>${LABEL}</button>`, count: 1 },
    { slot: "aria-label", cls: "stale-label", needle: `aria-label="${A11Y_NAME}"`, count: 1 },
    { slot: "tooltip", cls: "stale-label", needle: `title="${A11Y_NAME}"`, count: 1 },
    {
      slot: "first-use-guidance",
      cls: "disclosure",
      needle: "Fully encrypted like every OSL attachment.",
      count: 1,
    },
  ],
  // Totals over the whole file, so deleting one of several redundant copies of
  // the name or the guidance is still red.
  totals: { label: 3, guidance: 3 },
});

const SURFACES = [
  { path: "apps/osl-hub-ui/overlay.html", ...composerExpectations("overlay composer control") },
  { path: "apps/osl-hub-ui/src/main.ts", ...composerExpectations("OSL chat composer control") },
  {
    path: "apps/osl-hub-ui/README.md",
    role: "help text",
    normalize: true,
    slots: [
      { slot: "help-name", cls: "stale-label", needle: `**${LABEL}**`, count: 1 },
      {
        slot: "help-guidance",
        cls: "disclosure",
        needle: "It is fully encrypted like every OSL attachment",
        count: 1,
      },
      {
        slot: "help-wire-disclosure",
        cls: "disclosure",
        needle: "the file format, not its encryption or wire representation",
        count: 1,
      },
    ],
    totals: { label: 1, guidance: 1 },
  },
];

const surfaceText = new Map();
for (const surface of SURFACES) {
  const abs = join(treeRoot, surface.path);
  if (!existsSync(abs)) {
    fail(surface.path, "missing", "file", `resource surface absent under ${treeRoot}`);
    continue;
  }
  const raw = readFileSync(abs, "utf8");
  const text = surface.normalize ? norm(raw) : raw;
  surfaceText.set(surface.path, raw);

  for (const { slot, cls, needle, count } of surface.slots) {
    const seen = countOf(text, needle);
    if (seen !== count) {
      fail(surface.path, cls, slot, `expected ${count}x ${JSON.stringify(needle)}, found ${seen}`);
    }
  }

  const labels = countOf(text, LABEL);
  if (labels !== surface.totals.label) {
    fail(surface.path, "stale-label", "label-count",
      `expected ${surface.totals.label}x "${LABEL}", found ${labels}`);
  }
  const guidance = countRe(norm(raw), new RegExp(GUIDANCE_TEXT, "giu"));
  if (guidance !== surface.totals.guidance) {
    fail(surface.path, "disclosure", "guidance-count",
      `expected ${surface.totals.guidance}x "${GUIDANCE_TEXT}", found ${guidance}`);
  }

  const stale = raw.match(STALE_LABEL) ?? [];
  if (stale.length) {
    fail(surface.path, "stale-label", "legacy-label",
      `${stale.length} legacy label(s): ${[...new Set(stale)].join(", ")}`);
  }
}

// ---------------------------------------------------------------------------
// 2. Exhaustive sweep of every other user-facing resource, so a stale label
//    hiding outside the three named surfaces is still caught and named.
// ---------------------------------------------------------------------------

// Same scope TASK 6840 scanned: the UI app, the app's declared capability and
// permission resources, docs, root help, webview and src-tauri. Dependency
// trees, build output, tests and the internal design/report archives are out —
// those discuss technical plaintext as a security property rather than label a
// control.
const SCOPE = [
  "apps/osl-hub-ui",
  "apps/osl-hub/capabilities",
  "apps/osl-hub/permissions",
  "docs",
  "webview",
  "src-tauri",
  "README.md",
  "CHANGELOG.md",
  "SECURITY.md",
];
const SKIP_DIRS = new Set(["node_modules", "dist", "target", ".git", "screenshots", "build"]);
const SKIP_PATHS = ["docs/design", "docs/reports", "docs/archive"];
const KEEP_EXT = /\.(html|ts|tsx|js|mjs|cjs|md|json|toml|css|txt|ftl|po|properties|ya?ml)$/i;
const SKIP_FILE = /(\.test\.[cm]?[jt]sx?$|^package-lock\.json$|^pnpm-lock\.yaml$|^Cargo\.lock$)/i;

const rel = (abs) => relative(treeRoot, abs).split(sep).join(posix.sep);

const walk = (abs, out) => {
  let entries;
  try {
    entries = readdirSync(abs, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    const child = join(abs, entry.name);
    const r = rel(child);
    if (entry.isSymbolicLink()) continue;
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name) || entry.name.startsWith(".tmp")) continue;
      if (SKIP_PATHS.some((p) => r === p || r.startsWith(`${p}/`))) continue;
      walk(child, out);
    } else if (entry.isFile() && KEEP_EXT.test(entry.name) && !SKIP_FILE.test(entry.name)) {
      out.push(child);
    }
  }
};

const scanned = [];
const rootsPresent = [];
for (const entry of SCOPE) {
  const abs = join(treeRoot, entry);
  if (!existsSync(abs)) continue;
  rootsPresent.push(entry);
  if (statSync(abs).isDirectory()) walk(abs, scanned);
  else if (KEEP_EXT.test(entry) && !SKIP_FILE.test(entry)) scanned.push(abs);
}

if (rootsPresent.length === 0) {
  fail("<scope>", "missing", "scope", `no user-facing roots found under ${treeRoot}`);
}

let totalLabels = 0;
let totalGuidance = 0;
let totalA11y = 0;
let totalTooltips = 0;
let totalStale = 0;

// A localisation bundle is either a file under a locale-ish directory or a file
// whose name is a locale code. Every one of them is a resource surface for this
// rename: if a bundle carries an ATTACHMENT control label it must carry the new
// name, and none of them may carry the retired one.
const LOCALE_DIR = /(^|\/)(locales?|i18n|lang|langs|translations|messages|intl)(\/|$)/i;
const LOCALE_CODES = new Set([
  "en", "es", "fr", "de", "pt", "it", "ja", "ko", "zh", "ru", "ar", "hi", "nl",
  "pl", "tr", "sv", "no", "da", "fi", "cs", "el", "he", "th", "vi", "id", "uk",
  "ro", "hu", "bg", "ca", "fa", "ms", "sr", "hr", "sk", "sl", "lt", "lv", "et",
]);
const LOCALE_FILE = /^([a-z]{2})(?:[-_]([A-Za-z]{2,4}))?\.(json|ts|js|mjs|md|html|ftl|po|properties|ya?ml)$/;
const localeBundles = [];
const localeRenameSurfaces = [];

for (const abs of scanned) {
  const r = rel(abs);
  const text = readFileSync(abs, "utf8");
  const flat = norm(text);

  totalLabels += countOf(text, LABEL);
  totalGuidance += countRe(flat, new RegExp(GUIDANCE_TEXT, "giu"));
  totalA11y += countOf(text, `aria-label="${A11Y_NAME}"`);
  totalTooltips += countOf(text, `title="${A11Y_NAME}"`);

  const stale = text.match(STALE_LABEL) ?? [];
  const base = r.split(posix.sep).pop();
  const localeMatch = LOCALE_FILE.exec(base);
  const isLocale =
    LOCALE_DIR.test(r) || (localeMatch !== null && LOCALE_CODES.has(localeMatch[1]));
  if (isLocale) {
    localeBundles.push(r);
    if (stale.length || /\bATTACHMENT\b/u.test(text)) {
      localeRenameSurfaces.push(r);
      if (stale.length) {
        totalStale += stale.length;
        fail(r, "locale", "legacy-label",
          `localisation bundle still says ${[...new Set(stale)].join(", ")}`);
      }
      if (!text.includes(LABEL)) {
        fail(r, "locale", "missing-rename",
          `localisation bundle names an attachment control but never says "${LABEL}"`);
      }
      if (!new RegExp(GUIDANCE_TEXT, "iu").test(flat)) {
        fail(r, "locale", "missing-guidance",
          `localisation bundle names an attachment control but never says "${GUIDANCE_TEXT}"`);
      }
    }
    continue;
  }

  if (stale.length && !SURFACES.some((s) => s.path === r)) {
    totalStale += stale.length;
    fail(r, "stale-label", "legacy-label",
      `${stale.length} user-facing legacy label(s): ${[...new Set(stale)].join(", ")}`);
  } else if (stale.length) {
    totalStale += stale.length;
  }
}

// ---------------------------------------------------------------------------
// 3. Report.
// ---------------------------------------------------------------------------

if (failures.length) {
  for (const f of failures) {
    console.log(
      `TASK6841_RED surface=${f.surface} class=${f.class} slot=${f.slot} detail=${f.detail}`,
    );
  }
  console.log(`TASK6841_RED_COUNT ${failures.length}`);
  process.exit(1);
}

console.log(
  `TASK6840_SCAN user_facing_plaintext_attachment=${totalStale} ` +
    `text_attachment=${totalLabels} fully_encrypted_guidance=${totalGuidance} ` +
    `accessibility_names=${totalA11y} tooltips=${totalTooltips} ` +
    `surfaces_checked=${SURFACES.length} locale_bundles=${localeBundles.length} ` +
    `locale_rename_surfaces=${localeRenameSurfaces.length} ` +
    `files_scanned=${scanned.length} roots=${rootsPresent.length}`,
);
process.exit(0);
