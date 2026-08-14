#!/usr/bin/env node
/**
 * Run the banned-word and plain-English check on a real OSL screen.
 *
 *   node scripts/check-screen-words.mjs new-friend-defaults
 *
 * The markup is produced by BUNDLING AND CALLING the screen's own module, not
 * by reading a saved copy of its HTML. A fixture would pass with the screen
 * deleted; this exits non-zero the moment the module is gone.
 *
 * Exit codes: 0 every item of the finish line passed, 1 something failed,
 * 2 the screen could not be built at all.
 */

import { createRequire } from "node:module";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { rmSync } from "node:fs";
import process from "node:process";

import { checkScreenWords, formatReport, screenCopyMissingWord } from "./lib/screen-words.mjs";

const REPO_ROOT = resolve(new URL("..", import.meta.url).pathname);
const UI_ROOT = join(REPO_ROOT, "apps", "osl-hub-ui");

/** esbuild is a dependency of apps/osl-hub-ui, not of the repo root. */
const esbuild = createRequire(join(UI_ROOT, "package.json"))("esbuild");

/**
 * One entry per "check <screen> words" task. `requiredWords`, `expectedTitle`
 * and `minimumWords` are copied from that task's done-when line in
 * OSL-AUDITS/todo/05-ui-settings.txt and nowhere else.
 */
export const SCREENS = Object.freeze({
  "new-friend-defaults": {
    task: "0735",
    // No line number: the plan file grows `done:`/`proof:` lines as tasks land,
    // so line numbers drift. Find it by task id.
    planLine: "OSL-AUDITS/todo/05-ui-settings.txt, TASK 0735",
    module: join(UI_ROOT, "src", "new-friend-defaults.ts"),
    entry: `
      import { initialNewFriendDefaults, newFriendDefaultsMarkup } from "MODULE";
      export const markup = newFriendDefaultsMarkup(initialNewFriendDefaults());
    `,
    expectedTitle: "New friend defaults",
    requiredWords: ["New friend defaults", "Messages", "Pictures", "Allow", "Block", "Save"],
    minimumWords: 12,
    /**
     * THROWAWAY ONLY, never shipped. TASK 0735 names Messages, Pictures, Allow
     * and Block; the screen TASK 0732 built has none of the four, because
     * `ipc::app_preferences::NewFriendDefaults` stores no per-content-kind
     * allow/block for a new friend. --repaired splices in the two groups those
     * four words would live in, so the check can be shown going green, and
     * green-then-red when one word is taken back out. Read it as the size of
     * the gap, not as the repair.
     */
    repairedCopy: (markup) =>
      markup.replace(
        '<div class="nfd-actions">',
        '<fieldset class="nfd-group"><legend>Messages from a new friend</legend>' +
          "<label>Allow</label><label>Block</label></fieldset>" +
          '<fieldset class="nfd-group"><legend>Pictures from a new friend</legend>' +
          "<label>Allow</label><label>Block</label></fieldset>" +
          '<div class="nfd-actions">',
      ),
  },
});

export async function renderScreen(name) {
  const screen = SCREENS[name];
  if (!screen) throw new Error(`unknown screen: ${name}. Known: ${Object.keys(SCREENS).join(", ")}`);
  const outFile = join(tmpdir(), `osl-screen-words-${name}-${process.pid}.mjs`);
  await esbuild.build({
    stdin: {
      contents: screen.entry.replaceAll("MODULE", screen.module),
      resolveDir: UI_ROOT,
      sourcefile: `${name}-screen-words-entry.mjs`,
      loader: "js",
    },
    bundle: true,
    platform: "node",
    format: "esm",
    outfile: outFile,
    loader: { ".css": "empty", ".svg": "text", ".png": "dataurl" },
    logLevel: "silent",
  });
  try {
    const built = await import(`file://${outFile}`);
    return built.markup;
  } finally {
    rmSync(outFile, { force: true });
  }
}

export async function checkScreen(name) {
  const screen = SCREENS[name];
  const markup = await renderScreen(name);
  return {
    screen,
    markup,
    report: checkScreenWords({
      markup,
      expectedTitle: screen.expectedTitle,
      requiredWords: screen.requiredWords,
      minimumWords: screen.minimumWords,
    }),
  };
}

async function main() {
  const name = process.argv[2];
  if (!name) {
    console.error(`usage: node scripts/check-screen-words.mjs <${Object.keys(SCREENS).join("|")}>`);
    process.exit(2);
  }
  const breakWord = process.argv.includes("--break-word")
    ? process.argv[process.argv.indexOf("--break-word") + 1]
    : null;

  let checked;
  try {
    checked = await checkScreen(name);
  } catch (error) {
    console.error(`check-screen-words: could not build ${name}: ${error.message}`);
    process.exit(2);
  }

  const prefix = `TASK_${checked.screen.task}`;
  const repaired = process.argv.includes("--repaired");
  let baseMarkup = checked.markup;
  if (repaired) {
    if (typeof checked.screen.repairedCopy !== "function") {
      console.error(`check-screen-words: ${name} has no --repaired throwaway copy`);
      process.exit(2);
    }
    baseMarkup = checked.screen.repairedCopy(checked.markup);
    console.log(`${prefix}_REPAIRED_THROWAWAY_COPY in use -- NOT the shipped screen`);
  }

  if (breakWord) {
    const copy = screenCopyMissingWord(baseMarkup, breakWord);
    const report = checkScreenWords({
      markup: copy,
      expectedTitle: checked.screen.expectedTitle,
      requiredWords: checked.screen.requiredWords,
      minimumWords: checked.screen.minimumWords,
    });
    console.log(`${prefix}_THROWAWAY_COPY missing 1 named word: ${JSON.stringify(breakWord)}`);
    console.log(formatReport(`${prefix}_THROWAWAY`, report));
    process.exit(report.ok ? 1 : 0);
  }

  const report = repaired
    ? checkScreenWords({
        markup: baseMarkup,
        expectedTitle: checked.screen.expectedTitle,
        requiredWords: checked.screen.requiredWords,
        minimumWords: checked.screen.minimumWords,
      })
    : checked.report;

  console.log(`${prefix} check-screen-words ${name} (plan ${checked.screen.planLine})`);
  console.log(`${prefix}_SOURCE ${checked.screen.module}`);
  console.log(formatReport(repaired ? `${prefix}_REPAIRED` : prefix, report));
  process.exit(report.ok ? 0 : 1);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await main();
}
