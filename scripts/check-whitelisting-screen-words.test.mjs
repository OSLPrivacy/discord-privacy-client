/**
 * TASK 0767 -- proof that the Whitelisting words check measures something.
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
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
  PRIMARY_SCENE,
  SCENE_NAMES,
  checkAllScenes,
  checkScreenWords,
  renderScenes,
} from "./check-whitelisting-screen-words.mjs";

const SCRIPTS = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPTS, "..");
const SCREEN_DIR = path.join(REPO_ROOT, "apps/osl-hub-ui/src");
const SCREEN_FILE = path.join(SCREEN_DIR, "whitelisting-screen.ts");
const CHECK = path.join(SCRIPTS, "check-whitelisting-screen-words.mjs");

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
 * directory holding a real file, so the check compiles and renders the broken
 * screen exactly the way it renders the real one.
 */
function throwawayScreen(edit) {
  const dir = mkdtempSync(path.join(tmpdir(), "task-0767-throwaway-"));
  const source = readFileSync(SCREEN_FILE, "utf8");
  const edited = edit(source);
  assert.notEqual(edited, source, "the throwaway edit changed nothing");
  writeFileSync(path.join(dir, path.basename(SCREEN_FILE)), edited);
  return dir;
}

test("reads the words, the title and the named words off the real screen", async () => {
  const scenes = await renderScenes(SCREEN_DIR);
  const result = checkScreenWords(scenes[PRIMARY_SCENE], { namedWords: NAMED_WORD_SETS.screen });

  assert.equal(result.title, EXPECTED_PAGE_TITLE);
  assert.ok(result.words.length >= MINIMUM_WORDS_READ, `${result.words.length} words read`);
  assert.deepEqual(result.named.filter((entry) => !entry.present), []);
  assert.deepEqual(result.banned, []);
  assert.deepEqual(result.errors, []);
  console.log(`TASK0767_WORDS_READ=${result.words.length}`);
  console.log(`TASK0767_TITLE=${result.title}`);
  console.log(`TASK0767_NAMED_PRESENT=${result.named.length}/${result.named.length}`);
  console.log(`TASK0767_BANNED_SCANNED=${BANNED_WORDS.length}`);
  console.log(`TASK0767_BANNED_FOUND=${result.banned.length}`);
});

test("every state the screen can be in is checked, not just the full one", async () => {
  const scenes = await renderScenes(SCREEN_DIR);
  const { results, errors } = checkAllScenes(scenes, { namedWords: NAMED_WORD_SETS.screen });

  assert.deepEqual(errors, []);
  for (const scene of SCENE_NAMES) {
    assert.equal(results[scene].title, EXPECTED_PAGE_TITLE, `${scene}: title`);
    assert.ok(results[scene].words.length >= MINIMUM_WORDS_READ, `${scene}: words`);
    assert.deepEqual(results[scene].banned, [], `${scene}: banned words`);
    console.log(
      `TASK0767_STATE_${scene.toUpperCase()}=words:${results[scene].words.length}`
      + ` banned:${results[scene].banned.length} title:${results[scene].title}`,
    );
  }
  // The states are genuinely different screens, not the same markup three times.
  assert.notEqual(results.all.words.length, results.mixed.words.length);
  assert.notEqual(results.mixed.words.length, results.empty.words.length);
});

test("the plan's own named-word list is measured, not assumed", async () => {
  const scenes = await renderScenes(SCREEN_DIR);
  const result = checkScreenWords(scenes[PRIMARY_SCENE], { namedWords: NAMED_WORD_SETS["plan-0767"] });
  const missing = result.named.filter((entry) => !entry.present).map((entry) => entry.word);

  assert.deepEqual(missing, ["Allowed friends", "Add friend", "Remove friend"]);
  console.log(`TASK0767_PLAN_NAMED_MISSING=${missing.join("|")}`);
  console.log(`TASK0767_PLAN_NAMED_PRESENT=${result.named.length - missing.length}/${result.named.length}`);

  const run = runCheck(["--words", "plan-0767"]);
  assert.equal(run.code, 1);
  assert.match(run.stdout, /named words present: 2\/5/u);
  assert.match(run.stderr, /named word missing from the screen: Add friend/u);
});

test("the zero is a measured zero: a planted banned word is found and named", async () => {
  const dir = throwawayScreen((source) =>
    source.replace(
      "Tick the conversations OSL may protect. OSL never touches a conversation that is not ticked here.",
      "OSL will parse the metadata of each conversation prior to applying this config.",
    ),
  );
  try {
    const scenes = await renderScenes(dir);
    const result = checkScreenWords(scenes[PRIMARY_SCENE], { namedWords: NAMED_WORD_SETS.screen });
    const words = result.banned.map((entry) => entry.word);

    assert.deepEqual(words, ["parse", "metadata", "prior to", "config"]);
    assert.deepEqual(
      result.banned.map((entry) => entry.say),
      ["read", "the extra details kept alongside a message", "before", "settings"],
    );
    console.log(`TASK0767_PLANTED_BANNED=${words.join("|")}`);

    const run = runCheck(["--screen-dir", dir]);
    assert.equal(run.code, 1);
    // Every state carries the header, so the planted words are found in all three.
    assert.match(run.stdout, /banned words found: 12/u);
    assert.match(
      run.stderr,
      /banned word "metadata" - say "the extra details kept alongside a message" instead/u,
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a throwaway copy missing one named word makes the check fail", async () => {
  const before = sha256(SCREEN_FILE);
  const cases = [
    {
      word: "Reset",
      edit: (source) => source.replace(">Reset</button>", ">Undo</button>")
        .replace(
          "Reset undoes unsaved ticks and puts the saved list back.",
          "Undo throws away unsaved ticks and puts the saved list back.",
        ),
    },
    {
      word: "Save",
      edit: (source) => source.replace(">Save</button>", ">Keep</button>")
        .replace("Save writes these ticks to this device.", "This writes these ticks to this device.")
        .replace("Reset undoes unsaved ticks and puts the saved list back.", "Reset undoes unsaved ticks."),
    },
    {
      word: "Select all",
      edit: (source) => source.replace(">Select all</button>", ">Tick every one</button>"),
    },
  ];

  for (const single of cases) {
    const dir = throwawayScreen(single.edit);
    try {
      const run = runCheck(["--screen-dir", dir]);
      assert.equal(run.code, 1, `${single.word}: expected the check to fail`);
      assert.match(run.stdout, new RegExp(`named word: MISSING - ${single.word}`, "u"));
      assert.match(run.stderr, new RegExp(`named word missing from the screen: ${single.word}`, "u"));
      assert.match(run.stdout, /named words present: 10\/11/u);
      console.log(`TASK0767_RED_ON_MISSING=${single.word}`);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }

  const after = sha256(SCREEN_FILE);
  assert.equal(after, before, "the real screen file was edited");
  console.log(`TASK0767_SCREEN_SHA256_UNCHANGED=${after}`);

  const green = runCheck([]);
  assert.equal(green.code, 0, green.stderr);
  assert.match(green.stdout, /named words present: 11\/11/u);
  assert.match(green.stdout, /banned words found: 0/u);
  console.log("TASK0767_GREEN_AFTER_RESTORE=1");
});

test("the readers behind the check hold up on their own", () => {
  // A tag becomes a space: two words either side of a tag must not be glued
  // into one word that nobody reads.
  assert.equal(visibleText("<p>Save <b>all</b>&amp;nothing else</p>"), "Save all &nothing else");
  assert.equal(readWords("3 of 8 conversations match").length, 3);
  assert.equal(readWords("8 -- 3").length, 0);

  // This screen names itself with aria-labelledby, so the title reader has to
  // follow the id to the heading it points at -- and refuse a dangling one.
  assert.equal(
    pageTitle('<section aria-labelledby="t"><h2 id="t">Whitelisting</h2></section>').title,
    "Whitelisting",
  );
  assert.equal(pageTitle('<section aria-labelledby="gone"><h2 id="t">Whitelisting</h2></section>').title, null);
  assert.match(
    pageTitle('<section aria-labelledby="gone"><h2 id="t">Whitelisting</h2></section>').why,
    /not on the screen/u,
  );
  assert.equal(pageTitle("<section><h2>Whitelisting</h2></section>").title, null);
  assert.equal(pageTitle('<section aria-labelledby="t"><p id="t">Whitelisting</p></section>').title, null);
  assert.match(
    pageTitle('<section aria-labelledby="t"><h2 id="x">Whitelisting</h2><p id="t">Other</p></section>').why,
    /disagree/u,
  );
  // The direct spelling still works, so the TASK 0743 screen is unaffected.
  assert.equal(
    pageTitle('<section aria-label="Auto-whitelist rules"><h2>Auto-whitelist rules</h2></section>').title,
    "Auto-whitelist rules",
  );

  assert.deepEqual(
    requiredWordReport("Select all and Clear all", ["Select all", "Clear all", "Add friend"]),
    [
      { word: "Select all", present: true },
      { word: "Clear all", present: true },
      { word: "Add friend", present: false },
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
