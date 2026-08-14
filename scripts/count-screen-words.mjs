#!/usr/bin/env node
// TASK 3161 - connect the language choice to every screen.
//
// Counts how many of the words a screen module's chosen language owns are
// STILL literally written inside that screen's own TypeScript source,
// rather than being read at runtime from the backend's screen-words file.
//
// The word list is not hand-copied here: it is read straight out of
// `crates/ipc/src/screen_words/{en,es}.json`, the single source of truth
// both the backend commands and the frontend tests already read from. A
// screen is "connected" for a given word only when that word's literal
// text (in either shipped language) appears nowhere in the screen's own
// source once comments are stripped.
//
// Usage: node scripts/count-screen-words.mjs [--verbose]

import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const REPO_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const SCREENS_DIR = path.join(REPO_ROOT, "apps", "osl-hub-ui", "src");
const WORDS_DIR = path.join(REPO_ROOT, "crates", "ipc", "src", "screen_words");

const verbose = process.argv.includes("--verbose");

function stripComments(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//gu, "")
    .replace(/(^|[^:])\/\/.*$/gmu, "$1");
}

function screenFiles() {
  return readdirSync(SCREENS_DIR)
    .filter((name) => /-screen\.ts$/u.test(name) && !name.endsWith(".test.ts"))
    .sort();
}

function screenKeyFor(fileName) {
  return fileName.replace(/-screen\.ts$/u, "").replace(/-/gu, "_");
}

function loadWordsFile(name) {
  return JSON.parse(readFileSync(path.join(WORDS_DIR, name), "utf8"));
}

function main() {
  const en = loadWordsFile("en.json");
  const es = loadWordsFile("es.json");
  const files = screenFiles();

  if (files.length === 0) {
    console.error("OSL: no *-screen.ts modules found — nothing to check");
    process.exit(1);
  }

  let totalHeld = 0;
  const unconnectedScreens = [];

  for (const file of files) {
    const screenKey = screenKeyFor(file);
    const englishWords = en[screenKey];
    const spanishWords = es[screenKey];
    if (!englishWords || !spanishWords) {
      unconnectedScreens.push(`${file} (no "${screenKey}" entry in screen_words/en.json or es.json)`);
      continue;
    }

    const source = stripComments(readFileSync(path.join(SCREENS_DIR, file), "utf8"));
    const allValues = [...Object.values(englishWords), ...Object.values(spanishWords)];
    for (const value of allValues) {
      if (source.includes(value)) {
        totalHeld += 1;
        if (verbose) console.log(`OSL: still held in ${file}: ${JSON.stringify(value)}`);
      }
    }
  }

  console.log(`OSL screens scanned: ${files.join(", ")}`);
  console.log(`OSL screens connected to a screen_words entry: ${files.length - unconnectedScreens.length}/${files.length}`);
  console.log(`OSL words still held inside screens: ${totalHeld}`);
  for (const unconnected of unconnectedScreens) {
    console.log(`OSL: NOT connected — ${unconnected}`);
  }

  if (totalHeld > 0 || unconnectedScreens.length > 0) {
    process.exit(1);
  }
}

main();
