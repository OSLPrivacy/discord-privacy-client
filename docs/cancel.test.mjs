import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const page = readFileSync(path.join(root, "cancel.html"), "utf8");

function linkTextForHref(href) {
  const escapedHref = href.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const match = page.match(new RegExp(`<a\\s+href=["']${escapedHref}["'][^>]*>([^<]+)</a>`, "iu"));
  return match?.[1] ?? null;
}

test("TASK 1524 cancel page lists no-payment, no-Pro-code, Download, and payment-help routes", () => {
  const statements = [
    ["no payment", "No payment was taken."],
    ["no Pro code", "No Pro code was created."],
  ];
  const routes = [
    ["Download", "docs/download.html"],
    ["payment help", "payment-help.html"],
  ];

  for (const [label, text] of statements) {
    assert.match(page, new RegExp(text.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
    console.log(`TASK 1524 statement: ${label} -> ${text}`);
  }

  for (const [label, href] of routes) {
    const text = linkTextForHref(href);
    assert.ok(text, `cancel.html must link to ${href}`);
    assert.ok(existsSync(path.join(root, href)), `${href} route must exist`);
    console.log(`TASK 1524 route: ${label} -> ${href} (${text})`);
  }
});
