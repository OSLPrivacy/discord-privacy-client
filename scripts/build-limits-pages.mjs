#!/usr/bin/env node

// Writes the known-problem list on audit.html and tested-limits.html from the
// limits record in data/tested-limits.json.
//
// The list between the `limits` markers, and the two numbers between the
// `limits-count` markers, are generated. Everything else on both pages is
// hand-written prose. Nothing here decides what a problem is: the record does,
// and scripts/check-audit-page-words.mjs re-derives that record from the
// capability registry and the support matrix and fails when a page disagrees
// with it. Running this script is how the pages are fixed; the check is what
// says whether they are right.

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const RECORD_PATH = path.join(REPO_ROOT, "data", "tested-limits.json");
const PAGES = ["audit.html", "tested-limits.html"];

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function anchorId(id) {
  return `limit-${id.replace(/[^a-z0-9]+/gi, "-")}`;
}

function renderItem(limit, indent) {
  const pad = " ".repeat(indent);
  const evidence = limit.evidence.map((file) => `<code>${escapeHtml(file)}</code>`).join(", ");
  return [
    `${pad}<li id="${anchorId(limit.id)}" data-limit="${escapeHtml(limit.id)}">`,
    `${pad}  <h3>${escapeHtml(limit.title)}</h3>`,
    `${pad}  <p data-limit-status>Status: ${escapeHtml(limit.status)}</p>`,
    `${pad}  <p data-limit-problem>${escapeHtml(limit.problem)}</p>`,
    `${pad}  <p data-limit-evidence>Recorded by ${escapeHtml(limit.source)}. Evidence: ${evidence}.</p>`,
    `${pad}</li>`,
  ].join("\n");
}

function replaceBlock(page, marker, body) {
  const start = `<!-- ${marker}:start -->`;
  const end = `<!-- ${marker}:end -->`;
  const startIndex = page.indexOf(start);
  const endIndex = page.indexOf(end);
  if (startIndex < 0 || endIndex <= startIndex) throw new Error(`missing ${marker} markers`);
  return `${page.slice(0, startIndex + start.length)}\n${body}\n${" ".repeat(page.slice(0, startIndex).split("\n").pop().length)}${page.slice(endIndex)}`;
}

function build() {
  const record = JSON.parse(readFileSync(RECORD_PATH, "utf8"));
  const total = record.limits.length;

  for (const page of PAGES) {
    const fullPath = path.join(REPO_ROOT, page);
    const original = readFileSync(fullPath, "utf8");
    const indent = original.split("\n").find((line) => line.includes("<!-- limits:start -->")).search(/\S/);
    const list = [
      `${" ".repeat(indent)}<ol class="limits">`,
      ...record.limits.map((limit) => renderItem(limit, indent + 2)),
      `${" ".repeat(indent)}</ol>`,
    ].join("\n");
    const countIndent = original.split("\n").find((line) => line.includes("<!-- limits-count:start -->")).search(/\S/);
    const counts =
      `${" ".repeat(countIndent)}<p>Known problems named on this page: ` +
      `<strong data-limit-count="page">${total}</strong>. Entries in the limits record ` +
      `<code>data/tested-limits.json</code>: <strong data-limit-count="record">${total}</strong>.</p>`;
    const next = replaceBlock(replaceBlock(original, "limits", list), "limits-count", counts);
    writeFileSync(fullPath, next);
    console.log(`build-limits-pages: ${page} <- ${total} known problems`);
  }
}

build();
