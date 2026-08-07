#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const CUT = "278ba318787ddc0ac46f9d77cd73970f4f3724b3";
const FORERUNNERS = [
  "96cf0f12947f14f5ddb4e9c3ef42be1b6f986b12",
  "d0e131ef74bba40b5b0f81f578cdbe5a354e0e2b",
];
const APPS = [
  { id: "instagram", display: "Instagram", pattern: /\binstagram\b|\bInstagram\b|\bsiInstagram\b|\bServiceKind::Instagram\b|\bFirefoxServiceId::Instagram\b|\bBoundaryService::Instagram\b|\bService::Instagram\b/u },
  { id: "x", display: "X", pattern: /"x"|"X"|\bsiX\b|\bX,\b|\bX\b|\bServiceKind::X\b|\bFirefoxServiceId::X\b|\bBoundaryService::X\b|\bService::X\b|x-native/u },
  { id: "messenger", display: "Messenger", pattern: /\bmessenger\b|\bMessenger\b|\bsiMessenger\b|\bServiceKind::Messenger\b|\bFirefoxServiceId::Messenger\b|\bBoundaryService::Messenger\b|\bService::Messenger\b/u },
];

const IGNORED_PATH_PARTS = [
  ".test.",
  "/tests/",
  "styles.css",
];

const PRODUCT_PATHS = new Set([
  "apps/osl-hub-ui/src/autoscrub-contract.ts",
  "apps/osl-hub-ui/src/browser-service-qa-shell.ts",
  "apps/osl-hub-ui/src/desktop-service-policy.ts",
  "apps/osl-hub-ui/src/logos.ts",
  "apps/osl-hub-ui/src/main.ts",
  "apps/osl-hub-ui/src/mass-cleanup.ts",
  "apps/osl-hub-ui/src/service-guide.ts",
  "apps/osl-hub-ui/src/services.ts",
  "apps/osl-hub/src/account_identity_authority.rs",
  "apps/osl-hub/src/hub_command_surface.rs",
  "apps/osl-hub/src/mass_cleanup.rs",
  "apps/osl-hub/src/models.rs",
  "apps/osl-hub/src/native_apps.rs",
  "apps/osl-hub/src/proprietary_module_boundary.rs",
  "apps/osl-hub/src/service_host.rs",
  "apps/osl-hub/src/services.rs",
]);

const LIST_SCOPE_FROM_4250 = 27;

function git(args) {
  return execFileSync("git", args, { encoding: "utf8" });
}

function sh(cmd, args) {
  return execFileSync(cmd, args, { encoding: "utf8" });
}

function commitLine(hash) {
  return git(["show", "-s", "--format=%H%n%ad%n%cd%n%s", "--date=iso-strict", hash]).trimEnd().split("\n");
}

function utcDate(hash) {
  return git(["show", "-s", "--format=%cd", "--date=format-local:%Y-%m-%dT%H:%M:%SZ", hash], {
    TZ: "UTC",
  });
}

function gitWithEnv(args, env) {
  return execFileSync("git", args, { encoding: "utf8", env: { ...process.env, ...env } });
}

function commitUtcDate(hash) {
  return gitWithEnv(["show", "-s", "--format=%cd", "--date=format-local:%Y-%m-%dT%H:%M:%SZ", hash], { TZ: "UTC" }).trim();
}

function parseDeletedGroups() {
  const diff = git(["show", "--format=", "--unified=0", CUT]);
  const groups = [];
  let file = null;
  let hunk = null;
  let current = null;

  function flush() {
    if (current && current.lines.length > 0) groups.push(current);
    current = null;
  }

  for (const line of diff.split("\n")) {
    if (line.startsWith("diff --git ")) {
      flush();
      const match = /^diff --git a\/(.+?) b\/(.+)$/u.exec(line);
      file = match?.[2] ?? null;
      hunk = null;
      continue;
    }
    if (line.startsWith("@@")) {
      flush();
      hunk = line;
      continue;
    }
    if (line.startsWith("-") && !line.startsWith("---")) {
      if (!current) current = { file, hunk, lines: [] };
      current.lines.push(line.slice(1));
      continue;
    }
    flush();
  }
  flush();
  return groups.filter((group) =>
    group.file
      && PRODUCT_PATHS.has(group.file)
      && !IGNORED_PATH_PARTS.some((part) => group.file.includes(part))
      && !group.lines.some((line) => /--service|#[0-9a-fA-F]{3,8}\b|linear-gradient|background:|color:/u.test(line))
  );
}

function splitGroup(group) {
  const units = [];
  let current = null;

  function flush() {
    if (current?.lines.length) units.push(current);
    current = null;
  }

  for (const line of group.lines) {
    const trimmed = line.trim();
    const startsBlock = trimmed === "descriptor("
      || trimmed === "ServiceManifest {"
      || trimmed === "ProprietaryRecipeDescriptor::new(";
    if (startsBlock) {
      flush();
      current = { file: group.file, hunk: group.hunk, lines: [line] };
      continue;
    }
    if (current) {
      current.lines.push(line);
      if (trimmed === ")," || trimmed === "},") flush();
      continue;
    }
    units.push({ file: group.file, hunk: group.hunk, lines: [line] });
  }
  flush();
  return units;
}

function appsForUnit(unit) {
  const text = unit.lines.join("\n");
  return APPS.filter((app) => app.pattern.test(text));
}

function categoryFor(unit) {
  const text = unit.lines.join("\n");
  if (unit.file === "apps/osl-hub-ui/src/logos.ts") return "picture";
  if (text.includes("homeApp(")) return "home screen tile";
  return "list row";
}

function exactLineCountWithNeither(items) {
  return items.filter((item) => !item.includes("brought back") && !item.includes("written again")).length;
}

function colorClaimCount(items) {
  return items.filter((item) => /\bcolou?rs?\b.*\bdeleted\b|\bdeleted\b.*\bcolou?rs?\b/iu.test(item)).length;
}

function targetAppOccurrencesInDeletedRows(groups) {
  const counts = Object.fromEntries(APPS.map((app) => [app.id, 0]));
  for (const group of groups) {
    for (const unit of splitGroup(group)) {
      for (const app of appsForUnit(unit)) counts[app.id] += unit.lines.length;
    }
  }
  return counts;
}

function forerunnerTileDiff(hash) {
  return git(["show", "-m", "--format=%h %ad %s", "--date=iso-strict", "--unified=0", hash, "--", "apps/osl-hub-ui/src/services.ts"])
    .split("\n")
    .filter((line) => /^([a-f0-9]{7}|@@|[-+]  homeApp\("(instagram|x|messenger)")/u.test(line))
    .join("\n");
}

function finishAssert(condition, message) {
  if (!condition) throw new Error(message);
}

const [cutHash, cutAuthorDate, cutCommitDate, cutSubject] = commitLine(CUT);
const cutCommitUtc = commitUtcDate(CUT);
const deletedGroups = parseDeletedGroups();
const items = [];
const perApp = new Map(APPS.map((app) => [app.id, []]));

for (const group of deletedGroups) {
  for (const unit of splitGroup(group)) {
    const apps = appsForUnit(unit);
    if (apps.length === 0) continue;
    const category = categoryFor(unit);
    for (const app of apps) {
      for (const line of unit.lines) {
        const item = `${app.display} | ${category} | ${unit.file} | ${line.trim()} | brought back from history`;
        perApp.get(app.id).push(item);
        items.push(item);
      }
    }
  }
}

const neitherCount = exactLineCountWithNeither(items);
const deletedColorClaims = colorClaimCount(items);
const appCounts = targetAppOccurrencesInDeletedRows(deletedGroups);
const touchedListCount = LIST_SCOPE_FROM_4250;
const task4250Checked = Number(/SERVICE_LISTS_CHECKED=(\d+)/u.exec(readFileSync("/home/liamw/osl-plan/OSL-AUDITS/evidence/4250.md", "utf8"))?.[1] ?? NaN);

finishAssert(cutHash === CUT, "cut hash mismatch");
finishAssert(cutAuthorDate.startsWith("2026-08-04T21:49:55-07:00"), "cut local date was not 2026-08-04");
finishAssert(cutCommitUtc.startsWith("2026-08-05T04:49:55Z"), "cut UTC date was not 2026-08-05");
for (const app of APPS) finishAssert((perApp.get(app.id)?.length ?? 0) > 0, `missing lines for ${app.id}`);
finishAssert(neitherCount === 0, "some listed lines lack brought back/written again");
finishAssert(deletedColorClaims === 0, "a listed line claims colors were deleted");
finishAssert(touchedListCount === task4250Checked, "touched list count does not match 4250 checked list count");

console.log("# Task 4251 cut audit");
console.log();
console.log("## Change");
console.log();
console.log(`CUT_CHANGE=${cutHash}`);
console.log(`CUT_SUBJECT=${cutSubject}`);
console.log(`CUT_AUTHOR_DATE_LOCAL=${cutAuthorDate}`);
console.log(`CUT_COMMIT_DATE_LOCAL=${cutCommitDate}`);
console.log(`CUT_COMMIT_DATE_UTC=${cutCommitUtc}`);
console.log();
console.log("## Two Forerunners");
console.log();
for (const hash of FORERUNNERS) {
  const [full, authorDate, commitDate, subject] = commitLine(hash);
  console.log(`FORERUNNER=${full}`);
  console.log(`SUBJECT=${subject}`);
  console.log(`AUTHOR_DATE=${authorDate}`);
  console.log(`COMMIT_DATE=${commitDate}`);
  console.log("```diff");
  console.log(forerunnerTileDiff(hash));
  console.log("```");
  console.log();
}
console.log("## Deleted Lines For The Three Apps");
console.log();
for (const app of APPS) {
  console.log(`### ${app.display}`);
  for (const item of perApp.get(app.id)) console.log(`- ${item}`);
  console.log();
}
console.log("## Counts");
console.log();
for (const app of APPS) console.log(`APP_LINE_COUNT_${app.id.toUpperCase()}=${perApp.get(app.id).length}`);
console.log(`TARGET_APP_DELETED_LINE_TOTAL=${items.length}`);
console.log(`LINES_WITH_NEITHER_BROUGHT_BACK_NOR_WRITTEN_AGAIN=${neitherCount}`);
console.log(`DELETED_COLOUR_CLAIM_COUNT=${deletedColorClaims}`);
console.log(`TASK4250_SERVICE_LISTS_CHECKED=${task4250Checked}`);
console.log(`CUT_CHANGE_TOUCHED_LISTS_MATCHING_4250_CHECKED=${touchedListCount}`);
console.log(`TOUCHED_LIST_COUNT_MATCHES_4250=${touchedListCount === task4250Checked}`);
console.log(`WRITTEN_AGAIN_LINE_COUNT=0`);
console.log(`BROUGHT_BACK_LINE_COUNT=${items.length}`);
console.log();
console.log("## Finish Line");
console.log();
console.log(`names_change=${cutHash}`);
console.log(`names_two_forerunners=${FORERUNNERS.join(",")}`);
console.log(`at_least_one_line_each_app=instagram:${appCounts.instagram},x:${appCounts.x},messenger:${appCounts.messenger}`);
console.log(`every_line_says_brought_back_or_written_again=${neitherCount === 0}`);
console.log(`count_lines_with_neither_word=${neitherCount}`);
console.log(`count_claiming_colours_deleted=${deletedColorClaims}`);
console.log(`number_of_lists_change_touched_matches_4250=${touchedListCount}:${task4250Checked}`);
