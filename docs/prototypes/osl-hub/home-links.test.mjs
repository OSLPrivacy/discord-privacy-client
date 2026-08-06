import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const homePath = fileURLToPath(new URL("./index.html", import.meta.url));
const homeDir = path.dirname(homePath);
const expected = new Map([
  ["setup", "Setup"],
  ["scrub", "Scrub"],
  ["autoscrub", "AutoScrub"],
  ["consent", "Consent"],
  ["stopping", "Stopping"],
  ["login-recovery", "Login recovery"],
]);

function decodeHtml(value) {
  return value
    .replaceAll("&amp;", "&")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&#039;", "'");
}

function slugHeading(heading) {
  return heading
    .trim()
    .toLowerCase()
    .replace(/<[^>]*>/gu, "")
    .replace(/[`*_]/gu, "")
    .replace(/[^\p{Letter}\p{Number}\s-]/gu, "")
    .replace(/\s+/gu, "-");
}

function markdownFragments(document) {
  return new Set([...document.matchAll(/^#{1,6}\s+(.+)$/gmu)].map((match) => slugHeading(match[1])));
}

function htmlFragments(document) {
  return new Set([...document.matchAll(/\sid=["']([^"']+)["']/giu)].map((match) => match[1]));
}

function fragmentsFor(filePath, document) {
  if (filePath.endsWith(".md")) return markdownFragments(document);
  if (filePath.endsWith(".html")) return htmlFragments(document);
  return new Set();
}

test("docs home links setup, Scrub, AutoScrub, consent, stopping, and login recovery to live destinations", () => {
  const home = readFileSync(homePath, "utf8");
  const links = [...home.matchAll(/<a\b(?=[^>]*\bdata-doc-home-link=["']([^"']+)["'])(?=[^>]*\bhref=["']([^"']+)["'])[^>]*>([\s\S]*?)<\/a>/giu)]
    .map((match) => ({
      key: match[1],
      href: decodeHtml(match[2]),
      label: decodeHtml(match[3].replace(/<[^>]*>/gu, "").trim().replace(/\s+/gu, " ")),
    }));

  assert.equal(links.length, expected.size, `expected ${expected.size} docs-home links, found ${links.length}`);

  const seen = new Set();
  for (const link of links) {
    assert.equal(link.label, expected.get(link.key), `${link.key} label`);
    assert.ok(!seen.has(link.key), `duplicate docs-home link ${link.key}`);
    seen.add(link.key);

    const [rawTarget, rawFragment] = link.href.split("#");
    assert.ok(rawTarget && rawFragment, `${link.key} must link to a file and fragment`);
    assert.ok(!/^[a-z][a-z0-9+.-]*:/iu.test(rawTarget), `${link.key} must be a local docs link`);

    const destination = path.resolve(homeDir, rawTarget);
    assert.ok(existsSync(destination), `${link.key} destination file exists: ${path.relative(path.dirname(homeDir), destination)}`);

    const targetDocument = readFileSync(destination, "utf8");
    const fragments = fragmentsFor(destination, targetDocument);
    assert.ok(fragments.has(rawFragment), `${link.key} destination fragment exists: #${rawFragment}`);

    console.log(`${link.key}: ${link.label} -> ${path.relative(homeDir, destination)}#${rawFragment}`);
  }

  for (const key of expected.keys()) assert.ok(seen.has(key), `missing docs-home link ${key}`);
  console.log(`live docs-home destinations: ${links.length}`);
});
