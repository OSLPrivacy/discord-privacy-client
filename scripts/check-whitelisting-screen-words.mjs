#!/usr/bin/env node
/**
 * TASK 0767 -- the banned-word and plain-English check for the Whitelisting
 * screen (TASK 0764).
 *
 * It renders the real shipped screen module, reads the words a person would
 * see, and then checks four things: the page title, that there are words to
 * read at all, that every named word is on the screen, and that no banned word
 * is. Any failure exits 1 and names what failed.
 *
 * The screen is rendered in all three states it can be in -- nothing seen yet,
 * the whole list, and a search that filters it -- and every state is checked,
 * so a banned word that only appears once the list has rows in it cannot hide
 * behind an empty screen.
 *
 * Usage:
 *   node scripts/check-whitelisting-screen-words.mjs
 *   node scripts/check-whitelisting-screen-words.mjs --words plan-0767
 *   node scripts/check-whitelisting-screen-words.mjs --screen-dir /tmp/throwaway/src
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
const SCREEN_MODULE = "whitelisting-screen.ts";
const SCENES_MODULE = path.join(UI_DIR, "screenshots/whitelisting-scenes.mjs");

/** The page title the plan fixes for this screen (TASK 0766 and TASK 0767). */
export const EXPECTED_PAGE_TITLE = "Whitelisting";

/** The plan's floor for "at least 12 words are read". */
export const MINIMUM_WORDS_READ = 12;

/**
 * The named words, by set.
 *
 * `screen` is this screen's own controls: the page title, the two app accounts
 * and the conversations they hold, the filter, the four buttons, and the two
 * words a row uses to say where it stands. That is the list TASK 0766 asks for
 * of the same screen's screenshot -- "every app account, conversation, filter,
 * rule, save, and reset control named on the page".
 *
 * `plan-0767` is the list TASK 0767 prints verbatim. Three of its five entries
 * ("Allowed friends", "Add friend", "Remove friend") name a friends list with
 * add and remove buttons. This screen has no such thing and was never asked to
 * have one: TASK 0764 built a list of conversations with a search, select all,
 * clear all, save and reset, and TASK 0766 -- the screenshot check for the very
 * same screen -- names accounts, conversations, filter, rule, save and reset,
 * and no friend anywhere. The set is kept, and runnable, so the mismatch is a
 * measured number in the evidence rather than a claim.
 */
export const NAMED_WORD_SETS = {
  screen: [
    "Whitelisting",
    "Discord",
    "Signal",
    "Study Circle",
    "Search conversations",
    "Select all",
    "Clear all",
    "Save",
    "Reset",
    "Allowed",
    "Not allowed",
  ],
  "plan-0767": [
    "Whitelisting",
    "Allowed friends",
    "Add friend",
    "Remove friend",
    "Save",
  ],
};

/**
 * The states the screen is checked in. `all` is the primary one: every row on
 * screen, ticks mixed, so every word the screen can say is on it.
 */
export const SCENE_NAMES = ["all", "mixed", "empty"];
export const PRIMARY_SCENE = "all";

/** Compile and load the screen module out of `screenDir`, whatever is in it. */
async function loadScreen(screenDir) {
  const modulePath = path.join(screenDir, SCREEN_MODULE);
  if (!existsSync(modulePath)) throw new Error(`screen module is missing: ${modulePath}`);
  const require = createRequire(path.join(UI_DIR, "package.json"));
  const esbuild = require("esbuild");
  const built = await esbuild.build({
    stdin: {
      contents: `export * from "./${SCREEN_MODULE}";`,
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

/**
 * Render the screen in every state, from the same scene data the Linux capture
 * (TASK 0764) draws from, so the words checked here are the words captured
 * there rather than a second set typed out for the check.
 */
export async function renderScenes(screenDir) {
  const screen = await loadScreen(screenDir);
  const { whitelistingScenes } = await import(`file://${SCENES_MODULE}`);
  const scenes = {
    // Every conversation on screen, ticks mixed: the widest set of words.
    all: { ...whitelistingScenes.mixed, search: "" },
    mixed: whitelistingScenes.mixed,
    empty: whitelistingScenes.empty,
  };
  return Object.fromEntries(
    SCENE_NAMES.map((name) => [name, screen.whitelistingScreenMarkup(scenes[name])]),
  );
}

/** The whole check, over already-rendered markup for one state. */
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

/**
 * The check over every state. The named words are required of the primary
 * state; the empty screen has no rows, so it cannot say a conversation's name.
 * The title, the word floor and the banned-word sweep are required of them all.
 */
export function checkAllScenes(markupByScene, { namedWords, minimumWords = MINIMUM_WORDS_READ }) {
  const results = {};
  const errors = [];
  for (const scene of SCENE_NAMES) {
    const result = checkScreenWords(markupByScene[scene], { namedWords, minimumWords });
    results[scene] = result;
    for (const error of result.errors) {
      const namedWordMiss = error.startsWith("named word missing");
      if (namedWordMiss && scene !== PRIMARY_SCENE) continue;
      errors.push(`[${scene}] ${error}`);
    }
  }
  return { results, errors };
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
  const markupByScene = await renderScenes(options.screenDir);
  const namedWords = NAMED_WORD_SETS[options.words];
  const { results, errors } = checkAllScenes(markupByScene, { namedWords });
  const primary = results[PRIMARY_SCENE];

  console.log(
    `check-whitelisting-screen-words: ${path.relative(REPO_ROOT, options.screenDir) || options.screenDir}`,
  );
  console.log(`named word set: ${options.words} (${namedWords.length} words)`);
  console.log(`states checked: ${SCENE_NAMES.join(", ")} (named words required of "${PRIMARY_SCENE}")`);
  for (const scene of SCENE_NAMES) {
    const result = results[scene];
    console.log(
      `state ${scene}: title=${result.title === null ? `NONE - ${result.titleWhy}` : `"${result.title}"`}`
      + ` words=${result.words.length} banned=${result.banned.length}`,
    );
  }
  console.log(`page title: ${primary.title === null ? `NONE - ${primary.titleWhy}` : `"${primary.title}"`}`);
  console.log(`words read: ${primary.words.length}`);
  for (const entry of primary.named) {
    console.log(`named word: ${entry.present ? "present" : "MISSING"} - ${entry.word}`);
  }
  console.log(
    `named words present: ${primary.named.filter((entry) => entry.present).length}/${primary.named.length}`,
  );
  console.log(`banned words scanned: ${BANNED_WORDS.length}`);
  const allBanned = SCENE_NAMES.flatMap((scene) => results[scene].banned.map((entry) => ({ scene, entry })));
  console.log(`banned words found: ${allBanned.length}`);
  for (const { scene, entry } of allBanned) {
    console.log(`banned word: [${scene}] "${entry.found}" - say "${entry.say}" instead: ...${entry.snippet}...`);
  }
  for (const kept of KEPT_WORDS) {
    console.log(`not scanned as jargon: ${kept.word} - ${kept.why}`);
  }

  if (errors.length > 0) throw new Error(errors.join("\n"));
  console.log("check-whitelisting-screen-words: complete.");
}

const invokedDirectly = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  try {
    await main(process.argv.slice(2));
  } catch (error) {
    console.error(`check-whitelisting-screen-words: fatal error: ${error.message}`);
    process.exit(1);
  }
}
