import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const documentPath = new URL("./osl-spaces-will-not-do.md", import.meta.url);
const source = readFileSync(documentPath, "utf8");
const omissions = [
  ...source.matchAll(/^## (\d+)\. (.+)\n([\s\S]*?)(?=^## |$(?![\s\S]))/gmu),
];

test("the deliberate-omissions list has all fourteen binding constraints", () => {
  assert.equal(omissions.length, 14);
  assert.deepEqual(
    omissions.map(([, number]) => Number(number)),
    Array.from({ length: 14 }, (_, index) => index + 1),
  );
});

test("every deliberate omission names its enforcing task", () => {
  for (const [, number, title, body] of omissions) {
    assert.match(
      body,
      /\*\*Enforced by:\*\* (?:T\d+-[A-Z]\d+(?:, )?)+(?:\.|\n)/u,
      `omission ${number} (${title}) must name at least one enforcing task`,
    );
  }
});
