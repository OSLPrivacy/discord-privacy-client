import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const todoRoot = process.env.TASK4276_TODO_ROOT || "/home/liamw/osl-plan/OSL-AUDITS/todo";

// The two owner choices already made (DECISIONS.txt, R1 and R2):
//   R1 - X, Instagram and Messenger go back into scope, recorded by TASK 4085.
//   R2 - mailbox reading is allowed, recorded by TASK 4086.
// A task is "frozen for no reason" if its wording still waits on one of
// these two choices going a certain way, using the exact stale phrasing
// those two tasks defined for a not-yet-made choice. Case-sensitive on
// purpose: it matches wording that OPENS a do:/done when: clause ("Only if
// 4085 said back in ...", "Only if 4086 said put it back ..."), not the
// lower-case mid-sentence mentions inside 4085/4085b/4086/4086b/4276b that
// describe the phrase itself as a break-test fixture.
const stalePhrases = [
  "Only if 4085 said back in",
  "Only if 4086 said put it back",
];

function fail(message) {
  console.error(`TASK4276_FAIL ${message}`);
  process.exitCode = 1;
}

function* filesUnder(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      yield* filesUnder(path);
    } else if (stat.isFile()) {
      yield path;
    }
  }
}

function staleMatches(dir) {
  const matches = [];
  for (const path of filesUnder(dir)) {
    const rel = relative(dir, path);
    const lines = readFileSync(path, "utf8").split(/\r?\n/u);
    let task = "(no TASK header before match)";
    for (const [index, line] of lines.entries()) {
      if (/^TASK \d+[a-z]? - /u.test(line)) task = line;
      for (const phrase of stalePhrases) {
        if (line.includes(phrase)) {
          matches.push({ rel, line: index + 1, task, phrase, text: line.trim() });
        }
      }
    }
  }
  return matches;
}

console.log(`TASK4276_TODO_ROOT=${todoRoot}`);
console.log(`TASK4276_CHOICES=three-apps-back-in(4085),mailbox-reading-allowed(4086)`);

const before = staleMatches(todoRoot);
for (const match of before) {
  console.error(`TASK4276_WAITING_MATCH ${match.rel}:${match.line} ${JSON.stringify(match.task)} ${JSON.stringify(match.phrase)} ${JSON.stringify(match.text)}`);
}
console.log(`TASK4276_WAITING_BEFORE_COUNT=${before.length}`);

const after = staleMatches(todoRoot);
console.log(`TASK4276_WAITING_AFTER_COUNT=${after.length}`);
console.log(`TASK4276_BEFORE_AFTER=${before.length},${after.length}`);

if (after.length !== 0) fail(`stale wording still waits on an already made choice (${after.length} match(es))`);
