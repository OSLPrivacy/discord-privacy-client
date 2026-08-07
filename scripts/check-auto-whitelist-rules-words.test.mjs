/**
 * TASK 0743 -- proof that the auto-whitelist rules words check measures
 * something.
 *
 * A words check that reports "zero banned words" is worthless unless it can be
 * shown to find one, and "every named word present" is worthless unless taking
 * a named word out turns it red. Both are done here against throwaway copies
 * of the real screen; the real screen file is fingerprinted before and after
 * to show it was never edited.
 */

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  BANNED_WORDS,
  bannedWordReport,
  pageTitle,
  readWords,
  requiredWordReport,
  visibleText,
} from "./plain-english-words.mjs";
import {
  EXPECTED_PAGE_TITLE,
  MINIMUM_WORDS_READ,
  NAMED_WORD_SETS,
  checkScreenWords,
  renderScreen,
} from "./check-auto-whitelist-rules-words.mjs";

const SCRIPTS = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPTS, "..");
const SCREEN_DIR = path.join(REPO_ROOT, "apps/osl-hub-ui/src");
const SCREEN_FILE = path.join(SCREEN_DIR, "auto-whitelist-rules-screen.ts");
const DATA_FILE = path.join(SCREEN_DIR, "auto-whitelist-rules-screen-data.ts");
const CHECK = path.join(SCRIPTS, "check-auto-whitelist-rules-words.mjs");

function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

/** Run the check as it would be run by hand. Never throws; returns what it printed. */
function runCheck(args) {
  try {
    const stdout = execFileSync(process.execPath, [CHECK, ...args], {
      cwd: REPO_ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    return { code: 0, stdout, stderr: "" };
  } catch (error) {
    return { code: error.status ?? 1, stdout: error.stdout ?? "", stderr: error.stderr ?? "" };
  }
}

/**
 * A throwaway copy of the screen with one edit made to it. The copy is a real
 * directory of real files, so the check compiles and renders the broken screen
 * exactly the way it renders the real one.
 */
function throwawayScreen(edit) {
  const dir = mkdtempSync(path.join(tmpdir(), "task-0743-throwaway-"));
  copyFileSync(DATA_FILE, path.join(dir, path.basename(DATA_FILE)));
  const source = readFileSync(SCREEN_FILE, "utf8");
  const edited = edit(source);
  assert.notEqual(edited, source, "the throwaway edit changed nothing");
  writeFileSync(path.join(dir, path.basename(SCREEN_FILE)), edited);
  return dir;
}

test("reads the words, the title and the named words off the real screen", async () => {
  const markup = await renderScreen(SCREEN_DIR);
  const result = checkScreenWords(markup, { namedWords: NAMED_WORD_SETS.screen });

  assert.equal(result.title, EXPECTED_PAGE_TITLE);
  assert.ok(result.words.length >= MINIMUM_WORDS_READ, `${result.words.length} words read`);
  assert.deepEqual(result.named.filter((entry) => !entry.present), []);
  assert.deepEqual(result.banned, []);
  assert.deepEqual(result.errors, []);
  console.log(`TASK0743_WORDS_READ=${result.words.length}`);
  console.log(`TASK0743_TITLE=${result.title}`);
  console.log(`TASK0743_BANNED_SCANNED=${BANNED_WORDS.length}`);
});

test("the plan's own named-word list is measured, not assumed", async () => {
  const markup = await renderScreen(SCREEN_DIR);
  const result = checkScreenWords(markup, { namedWords: NAMED_WORD_SETS["plan-0743"] });
  const missing = result.named.filter((entry) => !entry.present).map((entry) => entry.word);

  assert.deepEqual(missing, ["Friends", "New friends", "Add rule", "Remove rule"]);
  console.log(`TASK0743_PLAN_NAMED_MISSING=${missing.join("|")}`);
  console.log(`TASK0743_PLAN_NAMED_PRESENT=${result.named.length - missing.length}/${result.named.length}`);
});

test("the zero is a measured zero: a planted banned word is found and named", async () => {
  const dir = throwawayScreen((source) =>
    source.replace(
      "What OSL does the first time a new place of each kind turns up.",
      "OSL will parse the metadata of each new place prior to applying this config.",
    ),
  );
  try {
    const markup = await renderScreen(dir);
    const result = checkScreenWords(markup, { namedWords: NAMED_WORD_SETS.screen });
    const words = result.banned.map((entry) => entry.word);

    assert.deepEqual(words, ["parse", "metadata", "prior to", "config"]);
    assert.deepEqual(
      result.banned.map((entry) => entry.say),
      ["read", "the extra details kept alongside a message", "before", "settings"],
    );
    assert.equal(result.errors.length, 4);
    console.log(`TASK0743_PLANTED_BANNED=${words.join("|")}`);

    const run = runCheck(["--screen-dir", dir]);
    assert.equal(run.code, 1);
    assert.match(run.stdout, /banned words found: 4/u);
    assert.match(run.stderr, /banned word "metadata" - say "the extra details kept alongside a message" instead/u);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a throwaway copy missing one named word makes the check fail", async () => {
  const before = sha256(SCREEN_FILE);
  const cases = [
    {
      word: "Reset",
      edit: (source) => source.replace('data-rule-action="reset">Reset</button>', 'data-rule-action="reset">Undo</button>'),
    },
    {
      word: "Save rules",
      edit: (source) => source.replace('data-rule-action="save">Save rules</button>', 'data-rule-action="save">Keep rules</button>'),
    },
  ];

  for (const single of cases) {
    const dir = throwawayScreen(single.edit);
    try {
      const run = runCheck(["--screen-dir", dir]);
      assert.equal(run.code, 1, `${single.word}: expected the check to fail`);
      assert.match(run.stdout, new RegExp(`named word: MISSING - ${single.word}`, "u"));
      assert.match(run.stderr, new RegExp(`named word missing from the screen: ${single.word}`, "u"));
      assert.match(run.stdout, new RegExp(`named words present: 9/10`, "u"));
      console.log(`TASK0743_RED_ON_MISSING=${single.word}`);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }

  const after = sha256(SCREEN_FILE);
  assert.equal(after, before, "the real screen file was edited");
  console.log(`TASK0743_SCREEN_SHA256_UNCHANGED=${after}`);

  const green = runCheck([]);
  assert.equal(green.code, 0, green.stderr);
  assert.match(green.stdout, /named words present: 10\/10/u);
  assert.match(green.stdout, /banned words found: 0/u);
});

test("the readers behind the check hold up on their own", () => {
  // A tag becomes a space: two words either side of a tag must not be glued
  // into one word that nobody reads.
  assert.equal(visibleText("<p>Save <b>rules</b>&amp;nothing else</p>"), "Save rules &nothing else");
  assert.equal(readWords("38 place kinds - one rule each").length, 5);
  assert.equal(readWords("38 -- 7").length, 0);

  assert.equal(
    pageTitle('<section aria-label="Auto-whitelist rules"><h2>Auto-whitelist rules</h2></section>').title,
    "Auto-whitelist rules",
  );
  assert.equal(pageTitle("<section aria-label=\"Rules\"><p>Auto-whitelist rules</p></section>").title, null);
  assert.match(
    pageTitle('<section aria-label="Rules"><h2>Auto-whitelist rules</h2></section>').why,
    /disagree/u,
  );

  assert.deepEqual(
    requiredWordReport("Save rules and Reset", ["Save rules", "Reset", "Add rule"]),
    [
      { word: "Save rules", present: true },
      { word: "Reset", present: true },
      { word: "Add rule", present: false },
    ],
  );

  // Whole words only: a banned word must not fire on a longer innocent word.
  assert.deepEqual(bannedWordReport("cache", BANNED_WORDS).map((entry) => entry.word), ["cache"]);
  assert.deepEqual(bannedWordReport("cachet catapult postpone", BANNED_WORDS), []);
  assert.deepEqual(bannedWordReport("Utilising the API endpoints", BANNED_WORDS).map((entry) => entry.word), [
    "utilise",
    "API",
    "endpoint",
  ]);
});
