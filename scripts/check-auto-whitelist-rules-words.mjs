#!/usr/bin/env node
/**
 * TASK 0743 -- the banned-word and plain-English check for the auto-whitelist
 * rules screen (TASK 0740).
 *
 * It renders the real screen module, reads the words a person would see, and
 * then checks four things: the page title, that there are words to read at
 * all, that every named word is on the screen, and that no banned word is.
 * Any failure exits 1 and names what failed.
 *
 * Usage:
 *   node scripts/check-auto-whitelist-rules-words.mjs
 *   node scripts/check-auto-whitelist-rules-words.mjs --words plan-0743
 *   node scripts/check-auto-whitelist-rules-words.mjs --screen-dir /tmp/throwaway/src
 *
 * `--screen-dir` is what makes the red proof possible: point it at a throwaway
 * copy of the screen with one named word taken out and the check goes red.
 */

import { existsSync } from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

import {
  BANNED_WORDS,
  KEPT_WORDS,
  bannedWordReport,
  pageTitle,
  readWords,
  requiredWordReport,
  visibleText,
} from "./plain-english-words.mjs";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const UI_DIR = path.join(REPO_ROOT, "apps/osl-hub-ui");
const DEFAULT_SCREEN_DIR = path.join(UI_DIR, "src");
const SCREEN_MODULE = "auto-whitelist-rules-screen.ts";
const SCREEN_DATA_MODULE = "auto-whitelist-rules-screen-data.ts";

/** The page title the plan fixes for this screen (TASK 0742 and TASK 0743). */
export const EXPECTED_PAGE_TITLE = "Auto-whitelist rules";

/** The plan's floor for "at least 12 words are read". */
export const MINIMUM_WORDS_READ = 12;

/**
 * The named words, by set.
 *
 * `screen` is this screen's own controls: the page title plus the nine
 * controls TASK 0742 requires of the same screen's screenshot.
 *
 * `plan-0743` is the list TASK 0743 prints verbatim. Four of its six entries
 * ("Friends", "New friends", "Add rule", "Remove rule") name a friends list
 * and per-rule add/remove buttons that this screen does not have and was never
 * asked to have -- TASK 0740 built one fixed row per place kind, with Save
 * rules and Reset. The set is kept, and runnable, so the mismatch is a
 * measured number in the evidence rather than a claim.
 */
export const NAMED_WORD_SETS = {
  screen: [
    "Auto-whitelist rules",
    "direct message",
    "group",
    "server",
    "channel",
    "thread",
    "email",
    "post",
    "Save rules",
    "Reset",
  ],
  "plan-0743": [
    "Auto-whitelist rules",
    "Friends",
    "New friends",
    "Add rule",
    "Remove rule",
    "Save",
  ],
};

/** Compile and load the screen module out of `screenDir`, whatever is in it. */
async function loadScreen(screenDir) {
  for (const module of [SCREEN_MODULE, SCREEN_DATA_MODULE]) {
    if (!existsSync(path.join(screenDir, module))) {
      throw new Error(`screen module is missing: ${path.join(screenDir, module)}`);
    }
  }
  const require = createRequire(path.join(UI_DIR, "package.json"));
  const esbuild = require("esbuild");
  const built = await esbuild.build({
    stdin: {
      contents: [
        `export * from "./${SCREEN_MODULE}";`,
        `export * from "./${SCREEN_DATA_MODULE}";`,
      ].join("\n"),
      resolveDir: screenDir,
      loader: "ts",
    },
    bundle: true,
    format: "esm",
    platform: "node",
    write: false,
  });
  const source = built.outputFiles[0].text;
  return import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
}

/** Render the screen the way the screenshot does: mixed rules, all four choices. */
export async function renderScreen(screenDir) {
  const screen = await loadScreen(screenDir);
  return screen.renderAutoWhitelistRulesScreen(
    screen.autoWhitelistRulesState(screen.AUTO_WHITELIST_RULES_SCREEN_SAVED),
  );
}

/** The whole check, over already-rendered markup. */
export function checkScreenWords(markup, { namedWords, minimumWords = MINIMUM_WORDS_READ }) {
  const text = visibleText(markup);
  const words = readWords(text);
  const title = pageTitle(markup);
  const named = requiredWordReport(text, namedWords);
  const banned = bannedWordReport(text, BANNED_WORDS);

  const errors = [];
  if (title.title === null) {
    errors.push(`no page title: ${title.why}`);
  } else if (title.title !== EXPECTED_PAGE_TITLE) {
    errors.push(`page title is "${title.title}", not "${EXPECTED_PAGE_TITLE}"`);
  }
  if (words.length < minimumWords) {
    errors.push(`only ${words.length} words read, ${minimumWords} needed`);
  }
  for (const entry of named.filter((candidate) => !candidate.present)) {
    errors.push(`named word missing from the screen: ${entry.word}`);
  }
  for (const entry of banned) {
    errors.push(`banned word "${entry.found}" - say "${entry.say}" instead: ...${entry.snippet}...`);
  }

  return { text, title: title.title, titleWhy: title.why, words, named, banned, errors };
}

function parseArguments(argv) {
  const options = { screenDir: DEFAULT_SCREEN_DIR, words: "screen" };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--screen-dir") {
      options.screenDir = path.resolve(argv[++index] ?? "");
    } else if (flag === "--words") {
      options.words = argv[++index] ?? "";
      if (!NAMED_WORD_SETS[options.words]) {
        throw new Error(
          `unknown named-word set: ${options.words} (have ${Object.keys(NAMED_WORD_SETS).join(", ")})`,
        );
      }
    } else {
      throw new Error(`unknown argument: ${flag}`);
    }
  }
  return options;
}

async function main(argv) {
  const options = parseArguments(argv);
  const markup = await renderScreen(options.screenDir);
  const namedWords = NAMED_WORD_SETS[options.words];
  const result = checkScreenWords(markup, { namedWords });

  console.log(`check-auto-whitelist-rules-words: ${path.relative(REPO_ROOT, options.screenDir) || options.screenDir}`);
  console.log(`named word set: ${options.words} (${namedWords.length} words)`);
  console.log(`page title: ${result.title === null ? `NONE - ${result.titleWhy}` : `"${result.title}"`}`);
  console.log(`words read: ${result.words.length}`);
  for (const entry of result.named) {
    console.log(`named word: ${entry.present ? "present" : "MISSING"} - ${entry.word}`);
  }
  console.log(`named words present: ${result.named.filter((entry) => entry.present).length}/${result.named.length}`);
  console.log(`banned words scanned: ${BANNED_WORDS.length}`);
  console.log(`banned words found: ${result.banned.length}`);
  for (const entry of result.banned) {
    console.log(`banned word: "${entry.found}" - say "${entry.say}" instead: ...${entry.snippet}...`);
  }
  for (const kept of KEPT_WORDS) {
    console.log(`not scanned as jargon: ${kept.word} - ${kept.why}`);
  }

  if (result.errors.length > 0) throw new Error(result.errors.join("\n"));
  console.log("check-auto-whitelist-rules-words: complete.");
}

const invokedDirectly = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  try {
    await main(process.argv.slice(2));
  } catch (error) {
    console.error(`check-auto-whitelist-rules-words: fatal error: ${error.message}`);
    process.exit(1);
  }
}
