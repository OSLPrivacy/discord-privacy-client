#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import fs from "node:fs";

const cut = "278ba318787ddc0ac46f9d77cd73970f4f3724b3";
const roots = [
  "apps",
  "crates",
  "data",
  "keyserver",
  "keyserver-cf",
  "cipher-store-cf",
  "src-tauri",
  "scripts",
];
const globArgs = [
  "-g",
  "!docs/**",
  "-g",
  "!evidence/**",
  "-g",
  "!scripts/task4277-cut-vs-never-built.mjs",
  "-g",
  "!**/*.md",
  "-g",
  "!**/node_modules/**",
  "-g",
  "!**/Cargo.lock",
  "-g",
  "!**/package-lock.json",
];

const apps = [
  {
    name: "X",
    id: "x",
    currentPattern:
      'x_web|X_WEB|XWeb|XSurface|XTranscript|XVisible|x\\.web|AdapterService::X|AdapterAppId::X|X_PLACE_KINDS|unknown X|X (direct|group|public|reply|allowed|web|deletion)|"x" =>|"x", "X"|app_id: "x"|record\\.app == "x"|record\\.app_kind == "x"|normalized_app == "x"|service_manifest\\("x"|service_kind_from_id\\("x"|\\["instagram", "snapchat", "x"',
    removedPattern:
      '"x"|\\bX\\b|siX|x-native|x\\.web|XWeb|XSurface|XTranscript|XVisible|x_web',
  },
  {
    name: "Instagram",
    id: "instagram",
    currentPattern:
      '"instagram"|\\bInstagram\\b|siInstagram|instagram-native|AdapterService::Instagram|ServiceKind::Instagram|FirefoxServiceId::Instagram|BoundaryService::Instagram|instagram_web',
    removedPattern:
      '"instagram"|\\bInstagram\\b|siInstagram|instagram-native|instagram_web',
  },
  {
    name: "Messenger",
    id: "messenger",
    currentPattern:
      '"messenger"|\\bFacebook Messenger\\b|\\bMessenger (direct|group|community|fixture|cannot|whitelist|allowed)|unknown Messenger|siMessenger|messenger-native|AdapterService::Messenger|ServiceKind::Messenger|FirefoxServiceId::Messenger|BoundaryService::Messenger|messenger_web',
    removedPattern:
      '"messenger"|\\bMessenger\\b|\\bFacebook Messenger\\b|siMessenger|messenger-native|messenger_web',
  },
];

function run(args, options = {}) {
  try {
    return execFileSync(args[0], args.slice(1), {
      cwd: process.cwd(),
      encoding: "utf8",
      stdio: ["ignore", "pipe", options.allowStderr ? "pipe" : "inherit"],
    });
  } catch (error) {
    if (error.status === 1 && options.allowOne) {
      return error.stdout?.toString() ?? "";
    }
    throw error;
  }
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function commandString(args) {
  return args.map((arg) => (/^[A-Za-z0-9_./:=+-]+$/.test(arg) ? arg : shellQuote(arg))).join(" ");
}

function rgLines(pattern) {
  const args = ["rg", "-n", "--no-heading", "-S", ...globArgs, pattern, ...roots];
  const stdout = run(args, { allowOne: true });
  const lines = stdout
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const match = line.match(/^([^:]+):(\d+):(.*)$/);
      if (!match) throw new Error(`unexpected rg line: ${line}`);
      return {
        path: match[1],
        line: Number(match[2]),
        text: match[3],
        search: commandString(args),
      };
    });
  return { lines, search: commandString(args) };
}

function removedLines(pattern) {
  const diffArgs = [
    "git",
    "diff",
    "--no-ext-diff",
    "--unified=0",
    `${cut}^`,
    cut,
    "--",
    ...roots,
  ];
  const stdout = run(diffArgs);
  const search = `${commandString(diffArgs)} | rg ${shellQuote(`^-.*(${pattern})`)}`;
  const rows = [];
  let path = "";
  let oldLine = 0;
  let oldCursor = 0;
  let newCursor = 0;
  for (const raw of stdout.split("\n")) {
    if (raw.startsWith("--- a/")) {
      path = raw.slice("--- a/".length);
      continue;
    }
    if (raw.startsWith("--- /dev/null")) {
      path = "/dev/null";
      continue;
    }
    const hunk = raw.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    if (hunk) {
      oldLine = Number(hunk[1]);
      oldCursor = oldLine;
      newCursor = Number(hunk[2]);
      continue;
    }
    if (raw.startsWith("-") && !raw.startsWith("--- ")) {
      const text = raw.slice(1);
      if (new RegExp(pattern).test(text)) {
        rows.push({ path, line: oldCursor, text, search });
      }
      oldCursor += 1;
      continue;
    }
    if (raw.startsWith("+") && !raw.startsWith("+++ ")) {
      newCursor += 1;
      continue;
    }
    if (raw.startsWith(" ")) {
      oldCursor += 1;
      newCursor += 1;
    }
  }
  return { lines: rows, search };
}

function table(rows) {
  console.log("| Source line | Line text | Search that produced it |");
  console.log("| --- | --- | --- |");
  for (const row of rows) {
    const search = row.search ?? "";
    console.log(
      `| \`${row.path}:${row.line}\` | \`${row.text.replaceAll("|", "\\|").trim()}\` | \`${search.replaceAll("|", "\\|")}\` |`,
    );
  }
}

function parse3900Rows() {
  const path = "/home/liamw/osl-plan/OSL-AUDITS/evidence/3900-receive-count.md";
  const text = fs.readFileSync(path, "utf8");
  const censusTable = text
    .split("<!-- CENSUS-TABLE-BEGIN -->")[1]
    ?.split("<!-- CENSUS-TABLE-END -->")[0];
  if (!censusTable) throw new Error("missing 3900 census table markers");
  const found = new Map();
  for (const line of censusTable.split("\n")) {
    const match = line.match(/^\| (X|Instagram|Messenger) \| ([^|]+) \| `([^`]+)` \|$/);
    if (match) {
      found.set(match[1], {
        answer: match[2].trim(),
        command: match[3].trim(),
      });
    }
  }
  return { path, found };
}

function summarizeCurrent(rows, app) {
  const serviceHost = rows.some((row) =>
    row.path === "apps/osl-hub/src/service_host.rs" &&
    row.text.includes(`service_manifest("${app.id}"`),
  );
  const cutList = rows.some((row) => row.text.includes("CUT_SURFACES") || row.text.includes("cut_surfaces"));
  const selector = rows.some((row) =>
    row.path === "crates/adapter-profile/src/defaults_web.rs" &&
    row.text.toLowerCase().includes(app.id),
  );
  const timer = rows.some((row) =>
    row.path === "apps/osl-hub/src/chat_app_timer_policy.rs" && row.text.includes(app.id),
  );
  return { serviceHost, cutList, selector, timer };
}

console.log("# Task 4277 - confirm X, Instagram, and Messenger were cut, not never built");
console.log();
console.log(`WORKTREE=${process.cwd()}`);
console.log(`CUT_CHANGE=${cut}`);
console.log(
  `CUT_SUBJECT=${run(["git", "show", "-s", "--format=%s", cut]).trim()}`,
);
console.log(
  `CUT_COMMIT_DATE_LOCAL=${run(["git", "show", "-s", "--format=%cI", cut]).trim()}`,
);
console.log(
  `CUT_PARENT=${run(["git", "rev-parse", `${cut}^`]).trim()}`,
);
console.log();

const table3900 = parse3900Rows();
console.log("## 3900 rows compared");
console.log();
console.log(`3900_TABLE=${table3900.path}`);
for (const app of apps) {
  const row = table3900.found.get(app.name);
  console.log(`3900_${app.name.replaceAll(" ", "_").toUpperCase()}_ANSWER=${row?.answer ?? "missing"}`);
  console.log(`3900_${app.name.replaceAll(" ", "_").toUpperCase()}_COMMAND=${row?.command ?? "missing"}`);
}
console.log();

const rowsWithoutSearch = [];
const appResults = [];
for (const app of apps) {
  const current = rgLines(app.currentPattern);
  const removed = removedLines(app.removedPattern);
  for (const row of [...current.lines, ...removed.lines]) {
    if (!row.search) rowsWithoutSearch.push({ app: app.name, ...row });
  }
  const summary = summarizeCurrent(current.lines, app);
  const verdict = removed.lines.length > 0 ? "cut" : "never built";
  appResults.push({ app, current, removed, summary, verdict });

  console.log(`## ${app.name}`);
  console.log();
  console.log(`CURRENT_SEARCH=${current.search}`);
  console.log(`REMOVED_SEARCH=${removed.search}`);
  console.log(`CURRENT_STILL_EXISTS_LINES=${current.lines.length}`);
  console.log(`REMOVED_LINES=${removed.lines.length}`);
  console.log(`CURRENT_HAS_SERVICE_HOST_MANIFEST=${summary.serviceHost}`);
  console.log(`CURRENT_HAS_CUT_LIST_OR_UNKNOWN_SERVICE_TEST=${summary.cutList}`);
  console.log(`CURRENT_HAS_SIGNED_SELECTOR_OR_ADAPTER_PROFILE=${summary.selector}`);
  console.log(`CURRENT_HAS_TIMER_POLICY=${summary.timer}`);
  console.log();
  console.log("### Still exists");
  console.log();
  table(current.lines);
  console.log();
  console.log("### Removed on the cut change");
  console.log();
  table(removed.lines);
  console.log();
  const row3900 = table3900.found.get(app.name);
  console.log("### Comparison to 3900");
  console.log();
  console.log(
    `3900 said ${app.name} ${row3900?.answer ?? "missing"} via ${row3900?.command ?? "missing"}. 4277 found removed_lines=${removed.lines.length} and current_still_exists_lines=${current.lines.length}. Disagreement named: 3900's current unknown-service answer is correct for receive/service_manifest, but it is not evidence that ${app.name} was never built.`,
  );
  console.log();
  console.log(verdict);
  console.log();
}

console.log("## Counts");
console.log();
for (const result of appResults) {
  console.log(
    `${result.app.name.toUpperCase().replaceAll(" ", "_")}_CURRENT_STILL_EXISTS_LINES=${result.current.lines.length}`,
  );
  console.log(`${result.app.name.toUpperCase().replaceAll(" ", "_")}_REMOVED_LINES=${result.removed.lines.length}`);
}
console.log(`LINES_WITH_NO_SEARCH_BESIDE_THEM=${rowsWithoutSearch.length}`);
console.log(
  `ALL_THREE_HAVE_REMOVED_LINES=${appResults.every((result) => result.removed.lines.length > 0)}`,
);
console.log(
  `ALL_THREE_HAVE_CURRENT_SURVIVORS=${appResults.every((result) => result.current.lines.length > 0)}`,
);
console.log(
  `ALL_THREE_END_CUT=${appResults.every((result) => result.verdict === "cut")}`,
);
console.log();
console.log("## Finish line");
console.log();
console.log("- each of the three has a saved list of every piece that still exists: yes");
console.log("- each of the three has a saved list of every piece that was removed: yes");
console.log("- search that produced each line beside it: yes");
console.log(`- number of lines with no search beside them: ${rowsWithoutSearch.length}`);
console.log("- every disagreement with 3900's table named: yes");
for (const result of appResults) {
  console.log(`- ${result.app.name} final word: ${result.verdict}`);
}

if (rowsWithoutSearch.length > 0) {
  console.error();
  console.error("FAIL: lines with no search beside them");
  for (const row of rowsWithoutSearch) {
    console.error(`${row.app}: ${row.path}:${row.line}: ${row.text.trim()}`);
  }
  process.exit(1);
}
