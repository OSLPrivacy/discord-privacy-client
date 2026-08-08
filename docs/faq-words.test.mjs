import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function runCheck() {
  return execFileSync("node", ["scripts/check-faq-words.mjs"], {
    cwd: root,
    encoding: "utf8",
    stdio: "pipe",
  });
}

test("T1537 FAQ carries all six answers and the shared pricing text", () => {
  const output = runCheck();
  console.log(output.trim());
  for (const label of [
    "ban risk",
    "bad messages",
    "stopping",
    "logout",
    "Pro benefits",
    "pricing facts",
  ]) {
    assert.match(output, new RegExp(`^check-faq-words: ${label.replace(/\$/gu, "\\$")} answer=present`, "mu"));
  }
  assert.match(output, /^check-faq-words: answers found 6\/6:/mu);
  assert.match(output, /^check-faq-words: shared_pricing_text=present/mu);
  assert.match(output, /^check-faq-words: FAQ contains all 6 answers and the shared pricing text\.$/mu);
});

test("T1537 the shared pricing text on the FAQ is the one from data/pricing.json", () => {
  const pricing = JSON.parse(readFileSync(path.join(root, "data/pricing.json"), "utf8"));
  const shared = pricing.model.approved_pricing_text;
  assert.equal(typeof shared, "string");
  assert.ok(shared.length > 0);

  const faq = readFileSync(path.join(root, "docs/faq.html"), "utf8");
  assert.ok(
    faq.includes(shared),
    `docs/faq.html must contain the shared pricing text verbatim: ${shared}`,
  );
});

test("T1537 sabotage: dropping the shared pricing text fails the check", () => {
  const faqPath = path.join(root, "docs/faq.html");
  const original = readFileSync(faqPath, "utf8");
  const pricing = JSON.parse(readFileSync(path.join(root, "data/pricing.json"), "utf8"));
  const shared = pricing.model.approved_pricing_text;
  const broken = original.replace(shared, "5 dollars a month.");
  assert.notEqual(broken, original, "sabotage must actually change the page");

  writeThen(faqPath, broken, original, () => {
    assert.throws(runCheck, (error) => {
      const output = `${error.stdout ?? ""}${error.stderr ?? ""}`;
      assert.match(output, /shared_pricing_text=MISSING/u);
      assert.match(output, /does not carry the shared pricing text verbatim/u);
      return true;
    });
  });
});

test("T1537 sabotage: dropping a required answer fails the check", () => {
  const faqPath = path.join(root, "docs/faq.html");
  const original = readFileSync(faqPath, "utf8");
  const broken = original.replace(
    "Unlink/logout is a separate user action.",
    "Ask support.",
  );
  assert.notEqual(broken, original, "sabotage must actually change the page");

  writeThen(faqPath, broken, original, () => {
    assert.throws(runCheck, (error) => {
      const output = `${error.stdout ?? ""}${error.stderr ?? ""}`;
      assert.match(output, /logout answer=INCOMPLETE/u);
      assert.match(output, /answers found 5\/6/u);
      return true;
    });
  });
});

// Writes the sabotaged page, runs body(), and always puts the original back.
function writeThen(file, broken, original, body) {
  writeFileSync(file, broken, "utf8");
  try {
    body();
  } finally {
    writeFileSync(file, original, "utf8");
  }
}
