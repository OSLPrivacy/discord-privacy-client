#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(import.meta.dirname, "..");
const CHECKLIST_PATH = path.join(
  REPO_ROOT,
  "docs/design/osl-internal-build-checklist.md",
);

const CATEGORY_D_ANCHOR = "category_D_release_gate";
const CATEGORY_D_TITLE = "D \u00b7 Attachments and lifecycle";
const EXPECTED_TOTAL_WEIGHT = 30;
const EXPECTED_TOTAL_EARNED = 7;
const REQUIRED_ROWS = Object.freeze([
  "D1",
  "D2",
  "D3",
  "D4",
  "D5",
  "D6",
  "D7",
]);

function categorySection(markdown) {
  const headingPattern = /^## D \u00b7 Attachments and lifecycle \u2014 (\d+) points \((\d+) earned\)$/m;
  const match = headingPattern.exec(markdown);
  if (!match) {
    throw new Error("category D heading is absent or malformed");
  }
  const start = match.index;
  const nextHeading = markdown.slice(start + match[0].length).search(/^## [A-Z] /m);
  const end =
    nextHeading === -1
      ? markdown.length
      : start + match[0].length + nextHeading;
  return {
    declaredWeight: Number.parseInt(match[1], 10),
    declaredEarned: Number.parseInt(match[2], 10),
    text: markdown.slice(start, end),
  };
}

function parseRows(sectionText) {
  const rowStartPattern = /^- \S+ \*\*(D\d) \u00b7 ([^*]+)\*\* \u2014 /gm;
  const rows = new Map();
  const starts = [...sectionText.matchAll(rowStartPattern)];
  for (const [index, match] of starts.entries()) {
    const rowId = match[1];
    if (rows.has(rowId)) {
      throw new Error(`category D row ${rowId} is duplicated`);
    }
    const next = starts[index + 1]?.index ?? sectionText.length;
    const text = sectionText.slice(match.index, next).trimEnd();
    const weight = /`weight: (\d+)`/.exec(text);
    const earned = /`earned: (\d+)`/.exec(text);
    const needs = /`needs: ([^`]+)`/.exec(text);
    if (!weight || !earned || !needs) {
      throw new Error(`category D row ${rowId} lacks needs, weight, or earned metadata`);
    }
    rows.set(rowId, {
      id: rowId,
      title: match[2].trim(),
      text,
      needs: needs[1],
      weight: Number.parseInt(weight[1], 10),
      earned: Number.parseInt(earned[1], 10),
    });
  }
  return rows;
}

function requireText(row, pattern, message) {
  if (!pattern.test(row.text)) {
    throw new Error(`${row.id}: ${message}`);
  }
}

export function admitCategoryDLifecycleEvidence(markdown) {
  const section = categorySection(markdown);
  if (!section.text.includes(CATEGORY_D_ANCHOR)) {
    throw new Error("category D release-gate anchor is absent");
  }
  if (section.declaredWeight !== EXPECTED_TOTAL_WEIGHT) {
    throw new Error("category D declared weight changed without this gate");
  }
  if (section.declaredEarned !== EXPECTED_TOTAL_EARNED) {
    throw new Error("category D declared earned points changed without this gate");
  }

  const rows = parseRows(section.text);
  for (const rowId of REQUIRED_ROWS) {
    if (!rows.has(rowId)) {
      throw new Error(`category D row ${rowId} is absent`);
    }
  }
  if (rows.size !== REQUIRED_ROWS.length) {
    throw new Error("category D contains unexpected scored rows");
  }

  const totalWeight = [...rows.values()].reduce((sum, row) => sum + row.weight, 0);
  const totalEarned = [...rows.values()].reduce((sum, row) => sum + row.earned, 0);
  if (totalWeight !== section.declaredWeight) {
    throw new Error("category D row weights do not match the heading");
  }
  if (totalEarned !== section.declaredEarned) {
    throw new Error("category D earned points do not match the heading");
  }

  const d2 = rows.get("D2");
  requireText(d2, /not deployed|production cleanup\s+remains `unknown`/i, "must not imply live attachment recovery evidence");
  requireText(d2, /Migration `0010` is unapplied/i, "must name the unapplied migration blocker");
  requireText(d2, /Exact `b944e9a`.*lowercase 32-hex attachment object ID/is, "must record the corrected D2 N2 shipping-id evidence");
  requireText(d2, /wrong length, uppercase,\s+cross-origin, cross-object and symbolic-link substitutions/is, "must record D2 N2 mutation refusals");
  requireText(d2, /source\/test evidence only,\s+not deployment, migration, or two-identity runtime evidence/is, "must not promote the D2 N2 source candidate");
  requireText(d2, /None of\s+`e8fbd3f`,\s+`3938a73`,\s+or\s+`1e9e635`\s+is deployment proof/i, "must refuse source-only attachment deployment proof");

  const d6 = rows.get("D6");
  requireText(d6, /migration 0031 pending/i, "must keep sender-retention migration as a live blocker");
  requireText(d6, /not deployed or proved with two authenticated identities/i, "must require the two-identity runtime boundary");
  if (d6.earned !== 0) {
    throw new Error("D6 cannot earn points until bilateral lifecycle evidence exists");
  }

  const d7 = rows.get("D7");
  requireText(d7, /outbound `Opened` is suppressed/i, "must suppress opened emission without consent");
  requireText(d7, /inbound `Opened` is rejected/i, "must reject opened admission without consent");
  requireText(d7, /mutual-consent contract remains `implemented-unwired`/i, "must not treat unwired consent as permission");
  requireText(d7, /not `verified-live`/i, "must not claim live receipt evidence");

  return {
    title: CATEGORY_D_TITLE,
    rows: [...rows.keys()],
    weight: totalWeight,
    earned: totalEarned,
  };
}

async function runCli(argv, dependencies = {}) {
  const readUtf8 = dependencies.readUtf8 ?? ((filePath) => readFile(filePath, "utf8"));
  const writeOut = dependencies.writeOut ?? ((text) => process.stdout.write(text));
  const filePath = argv[0] ? path.resolve(argv[0]) : CHECKLIST_PATH;
  const result = admitCategoryDLifecycleEvidence(await readUtf8(filePath));
  writeOut(
    `admitted ${result.title}: ${result.earned}/${result.weight} points across ${result.rows.join(", ")}\n`,
  );
  return result;
}

function sectionFixture(overrides = {}) {
  const rows = {
    D1: "- X **D1 \u00b7 Structure-compatible text/image/video/file carriers** \u2014 text shaping works in QA; literal line/size/file-type parity and host transformations need proof. `needs: C contract` `weight: 5` `earned: 2`",
    D2: "- X **D2 \u00b7 Encrypted attachment transport** \u2014 Exact recovery source is local only. Migration `0010` is unapplied and the matching Worker is inactive. Production cleanup remains `unknown`. Exact `b944e9a` keeps the shipping lowercase 32-hex attachment object ID and rejects wrong length, uppercase, cross-origin, cross-object and symbolic-link substitutions. This is source/test evidence only, not deployment, migration, or two-identity runtime evidence. None of `e8fbd3f`, `3938a73`, or `1e9e635` is deployment proof. `needs: A6,D1` `weight: 5` `earned: 1`",
    D3: "- X **D3 \u00b7 View-once text** \u2014 mechanisms exist; two-identity second-open refusal unproved. `needs: B6` `weight: 4` `earned: 1`",
    D4: "- X **D4 \u00b7 View-once image/protected viewer** \u2014 viewer/link foundations exist; first-paint protection and end-to-end path unproved. `needs: D2,B6` `weight: 4` `earned: 1`",
    D5: "- X **D5 \u00b7 Timed deletion** \u2014 scheduler/ledger pieces exist; production wiring and all lifecycle outcomes unproved. `needs: B6,D2` `weight: 4` `earned: 1`",
    D6: "- X **D6 \u00b7 Bilateral Burn** \u2014 migration 0031 pending, so the combined product is not deployed or proved with two authenticated identities. `needs: A7,B6` `weight: 4` `earned: 0`",
    D7: "- X **D7 \u00b7 Receipts** \u2014 outbound `Opened` is suppressed and inbound `Opened` is rejected while `Received` is preserved. This is `test-proven-only`, not `verified-live`. The mutual-consent contract remains `implemented-unwired`. `needs: B6,D3-D6` `weight: 4` `earned: 1`",
    ...overrides.rows,
  };
  const body = REQUIRED_ROWS.map((rowId) => rows[rowId]).filter(Boolean).join("\n");
  return [
    "# Checklist",
    "",
    `## D \u00b7 Attachments and lifecycle \u2014 ${overrides.weight ?? 30} points (${overrides.earned ?? 7} earned)`,
    "",
    overrides.anchor === false ? "" : "<!-- category_D_release_gate -->",
    "",
    body,
    "",
    "## E \u00b7 Every offered app \u2014 40 points (4 earned)",
    "",
  ].join("\n");
}

function assertRejectsFixture(label, fixture, pattern) {
  assert.throws(
    () => admitCategoryDLifecycleEvidence(fixture),
    pattern,
    label,
  );
}

function runSelfTests() {
  assert.deepEqual(admitCategoryDLifecycleEvidence(sectionFixture()), {
    title: CATEGORY_D_TITLE,
    rows: REQUIRED_ROWS,
    weight: 30,
    earned: 7,
  });
  assertRejectsFixture(
    "missing anchor refuses admission",
    sectionFixture({ anchor: false }),
    /anchor is absent/,
  );
  assertRejectsFixture(
    "missing D6 refuses admission",
    sectionFixture({ rows: { D6: undefined } }),
    /row D6 is absent/,
  );
  assertRejectsFixture(
    "bad score arithmetic refuses admission",
    sectionFixture({ earned: 8 }),
    /declared earned points changed/,
  );
  assertRejectsFixture(
    "D2 cannot claim deployment without migration evidence",
    sectionFixture({
      rows: {
        D2: "- X **D2 \u00b7 Encrypted attachment transport** \u2014 all attachment recovery evidence is production-ready. `needs: A6,D1` `weight: 5` `earned: 1`",
      },
    }),
    /D2: must not imply live attachment recovery evidence/,
  );
  assertRejectsFixture(
    "D6 cannot earn while migration/live boundary is absent",
    sectionFixture({
      rows: {
        D1: "- X **D1 \u00b7 Structure-compatible text/image/video/file carriers** \u2014 text shaping works in QA; literal line/size/file-type parity and host transformations need proof. `needs: C contract` `weight: 5` `earned: 1`",
        D6: "- X **D6 \u00b7 Bilateral Burn** \u2014 migration 0031 pending, so the combined product is not deployed or proved with two authenticated identities. `needs: A7,B6` `weight: 4` `earned: 1`",
      },
    }),
    /D6 cannot earn/,
  );
  assertRejectsFixture(
    "D7 must refuse opened receipts without consent",
    sectionFixture({
      rows: {
        D7: "- X **D7 \u00b7 Receipts** \u2014 opened receipts are accepted by default. `needs: B6,D3-D6` `weight: 4` `earned: 1`",
      },
    }),
    /D7: must suppress opened emission without consent/,
  );
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));

if (isMain) {
  try {
    if (process.argv[2] === "--self-test") {
      runSelfTests();
      process.stdout.write("category D lifecycle admission self-tests passed\n");
    } else {
      await runCli(process.argv.slice(2));
    }
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  }
}
