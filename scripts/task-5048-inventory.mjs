import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";

const root = path.resolve(new URL("..", import.meta.url).pathname);
const screenPath = path.join(root, "docs/prototypes/osl-hub/index.html");
const appPath = path.join(root, "docs/prototypes/osl-hub/app.js");
const sections = ["Inbox", "People", "Privacy", "Connections", "Activity"];
const statements = [
  ["Inbox", "Private communication", "app.js"], ["Inbox", "Inbox", "index.html"],
  ["People", "Identity and trust", "app.js"], ["People", "People", "index.html"],
  ["Privacy", "Simple by default", "index.html"], ["Privacy", "Privacy", "index.html"],
  ["Connections", "Unlimited accounts in Free", "index.html"], ["Connections", "Connections", "index.html"],
  ["Activity", "Proof, not promises", "index.html"], ["Activity", "Activity", "index.html"],
];

function lineOf(source, statement) {
  const line = source.split("\n").findIndex((value) => value.includes(statement));
  return line < 0 ? null : line + 1;
}

function capture(screen, app) {
  const found = sections.filter((section) => screen.includes(`>${section}<`) || screen.includes(`>${section} `));
  assert.deepEqual(found, sections, `capture requires every section; found ${found.join(", ") || "none"}`);
  const inventory = statements.map(([section, statement, file]) => {
    const source = file === "index.html" ? screen : app;
    const line = lineOf(source, statement);
    assert.ok(line, `capture missing statement: ${statement}`);
    return `${section}: ${statement} — ${file}:${line}`;
  });
  return inventory;
}

const screen = readFileSync(screenPath, "utf8");
const app = readFileSync(appPath, "utf8");
const intact = capture(screen, app);
console.log(`INTACT_SECTIONS=${sections.length}`);
console.log(`INTACT_STATEMENTS=${intact.length}`);
for (const line of intact) console.log(line);

const temp = mkdtempSync(path.join(os.tmpdir(), "task-5048-"));
try {
  const stripped = path.join(temp, "index.html");
  writeFileSync(stripped, screen.replace(/<body[\s\S]*?<\/body>/u, "<body></body>"));
  assert.throws(() => capture(readFileSync(stripped, "utf8"), app), /capture requires every section/);
  console.log("STRIPPED_RESULT=FAIL (named missing inventory sections)");
} finally {
  rmSync(temp, { recursive: true, force: true });
}
