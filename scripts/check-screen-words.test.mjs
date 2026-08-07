/**
 * TASK 0735 -- the banned-word and plain-English check on the new-friend
 * default screen (the screen TASK 0732 built, gate of this task).
 *
 * Every number this file asserts is measured off the real screen module,
 * apps/osl-hub-ui/src/new-friend-defaults.ts, bundled and called. Nothing here
 * reads a saved copy of the HTML.
 *
 * READ THIS BEFORE TRUSTING THE GREEN: TASK 0735's done-when names six words
 * that have to be on the screen -- New friend defaults, Messages, Pictures,
 * Allow, Block, Save. The screen TASK 0732 built has two of them. The other
 * four describe a per-kind allow/block default that does not exist anywhere:
 * `ipc::app_preferences::NewFriendDefaults` (TASK 0249, the gate of 0732) holds
 * account_reach, auto_whitelist and verification_warnings, and no per-content
 * kind at all. So the four missing words are pinned here as a measured gap, not
 * asserted away. `node scripts/check-screen-words.mjs new-friend-defaults`
 * exits 1 today and says which four. Putting them on the screen is a build
 * task; this is a test task.
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
  BANNED_WORDS,
  checkScreenWords,
  findBannedWords,
  pageTitleFromMarkup,
  screenCopyMissingWord,
  visibleTextFromMarkup,
  wordsOf,
} from "./lib/screen-words.mjs";
import { SCREENS, checkScreen, renderScreen } from "./check-screen-words.mjs";

const SCREEN = SCREENS["new-friend-defaults"];

test("visible text is what a person reads, never an attribute value", () => {
  const markup = `<section aria-label="New friend defaults" data-hint="Messages Pictures Allow Block"><p>Only this.</p></section>`;
  assert.equal(visibleTextFromMarkup(markup), "Only this.");
  assert.equal(pageTitleFromMarkup(markup), null);

  // The trap this closes: a screen that names its controls only to a machine
  // would otherwise report the title and all four words as present.
  const report = checkScreenWords({
    markup,
    expectedTitle: "New friend defaults",
    requiredWords: ["Messages", "Pictures", "Allow", "Block"],
    minimumWords: 2,
  });
  assert.equal(report.ok, false);
  assert.equal(report.pageTitle, null);
  assert.deepEqual(report.missing, ["Messages", "Pictures", "Allow", "Block"]);
});

test("banned words are found whole, with the plain-English word to use instead", () => {
  assert.deepEqual(findBannedWords("Allowance for a low API call"), [
    { jargon: "API", plain: "connection", count: 1 },
  ]);
  // "Allow" must not match inside "Allowance", and "low" must not drag in a
  // banned word either; only the standalone API is reported.
  assert.equal(findBannedWords("Allow this friend").length, 0);
  assert.equal(
    findBannedWords("Configure the adapter, then enable the token.").map((entry) => entry.jargon).join(","),
    "token,configure,enable,adapter",
  );
  assert.ok(BANNED_WORDS.length >= 50, `banned list is ${BANNED_WORDS.length} entries`);
});

test("the screen module is what gets checked, not a saved copy", async () => {
  const markup = await renderScreen("new-friend-defaults");
  assert.ok(markup.includes('<h2 class="nfd-title">New friend defaults</h2>'));
  assert.ok(markup.includes("Save default"));
});

test("TASK 0735 measurements on the real new-friend default screen", async () => {
  const { report } = await checkScreen("new-friend-defaults");

  // 1. page title
  assert.equal(report.pageTitle, "New friend defaults");
  assert.equal(report.titleMatches, true);

  // 2. at least 12 words are read
  assert.ok(report.wordsRead >= 12, `only ${report.wordsRead} words read`);
  assert.equal(report.wordsRead, wordsOf(report.visibleText).length);

  // 3. named words -- two present, four missing. This is the gap, pinned.
  assert.deepEqual(report.present, ["New friend defaults", "Save"]);
  assert.deepEqual(report.missing, ["Messages", "Pictures", "Allow", "Block"]);

  // 4. zero banned words
  assert.equal(report.bannedCount, 0);
  assert.deepEqual(report.banned, []);

  // and therefore the finish line as written is NOT met
  assert.equal(report.ok, false);
});

test("a throwaway copy missing one named word makes the check fail", async () => {
  const markup = await renderScreen("new-friend-defaults");

  for (const word of ["New friend defaults", "Save"]) {
    const copy = screenCopyMissingWord(markup, word);
    const report = checkScreenWords({
      markup: copy,
      expectedTitle: SCREEN.expectedTitle,
      requiredWords: SCREEN.requiredWords,
      minimumWords: SCREEN.minimumWords,
    });
    assert.equal(report.ok, false);
    assert.ok(report.missing.includes(word), `removing ${word} did not make it missing`);
    assert.ok(
      report.failures.some((failure) => failure.includes(word)),
      `no failure line names ${word}`,
    );
  }

  // Removing the title word also takes the heading away, so the title check
  // fails on its own too -- one deletion, two red items.
  const titleless = screenCopyMissingWord(markup, "New friend defaults");
  assert.equal(pageTitleFromMarkup(titleless), null);
});

test("the break-it mutation cannot be faked on a word that was never there", async () => {
  const markup = await renderScreen("new-friend-defaults");
  assert.throws(() => screenCopyMissingWord(markup, "Pictures"), /not in the visible text/);
});

test("the check does go green when all six named words are on the screen", async () => {
  const markup = await renderScreen("new-friend-defaults");

  // A throwaway copy ONLY -- this is not committed to the screen. It shows the
  // four missing words are the whole distance between this screen and TASK
  // 0735's finish line, and that the check is capable of passing. Same copy the
  // CLI's --repaired flag builds.
  const withKinds = SCREEN.repairedCopy(markup);
  assert.notEqual(withKinds, markup);

  const report = checkScreenWords({
    markup: withKinds,
    expectedTitle: SCREEN.expectedTitle,
    requiredWords: SCREEN.requiredWords,
    minimumWords: SCREEN.minimumWords,
  });
  assert.deepEqual(report.missing, []);
  assert.equal(report.bannedCount, 0);
  assert.equal(report.ok, true, report.failures.join("; "));

  // and it is still not a check that passes on anything: take one word back out
  // of the repaired copy and it goes red again.
  const broken = screenCopyMissingWord(withKinds, "Block");
  const brokenReport = checkScreenWords({
    markup: broken,
    expectedTitle: SCREEN.expectedTitle,
    requiredWords: SCREEN.requiredWords,
    minimumWords: SCREEN.minimumWords,
  });
  assert.equal(brokenReport.ok, false);
  assert.deepEqual(brokenReport.missing, ["Block"]);
});

test("a banned word planted in the screen copy is found and named", async () => {
  const markup = await renderScreen("new-friend-defaults");
  const withJargon = markup.replace("Approved chats only", "Configure the adapter");
  assert.notEqual(withJargon, markup);
  const report = checkScreenWords({
    markup: withJargon,
    expectedTitle: SCREEN.expectedTitle,
    requiredWords: [],
    minimumWords: SCREEN.minimumWords,
  });
  assert.equal(report.bannedCount, 2);
  assert.deepEqual(
    report.banned.map((entry) => `${entry.jargon}->${entry.plain}`),
    ["configure->set", "adapter->app connection"],
  );
  assert.equal(report.ok, false);
});
