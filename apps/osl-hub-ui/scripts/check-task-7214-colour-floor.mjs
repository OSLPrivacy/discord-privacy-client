#!/usr/bin/env node
/**
 * TASK 7214's source-level gate.  The capture tests need to count decoded RGB
 * triples; a compressed PNG byte census, or a census of individual decoded
 * bytes, is the defect this gate prevents from being reintroduced.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const APP_DIR = fileURLToPath(new URL("..", import.meta.url));
const SCREENSHOTS = join(APP_DIR, "screenshots");
const TESTS = [
  "task-0354-create-password-capture.test.mjs",
  "task-0357-restore-account-capture.test.mjs",
  "task-0373-cover-insertion-capture.test.mjs",
];
const failures = [];

for (const file of TESTS) {
  const source = readFileSync(join(SCREENSHOTS, file), "utf8");
  if (/new Set\(png\)|UNIQUE_BYTES|unique byte values/iu.test(source)) {
    failures.push(`TASK7214_FAIL file=${file} quantity=compressed bytes`);
  }
  if (!source.includes("countDistinctRgb(readPng(png))")) {
    failures.push(`TASK7214_FAIL file=${file} quantity=decoded RGB colours`);
  }
  if (!source.includes("DISTINCT_RGB_FLOOR = 32")) {
    failures.push(`TASK7214_FAIL file=${file} quantity=decoded RGB floor`);
  }
}

const pixelsFile = "screenshots/lib/png-pixels.mjs";
const pixels = readFileSync(join(APP_DIR, pixelsFile), "utf8");
if (!/colors\.add\(\(pixels\[offset\] << 16\) \| \(pixels\[offset \+ 1\] << 8\) \| pixels\[offset \+ 2\]\);/u.test(pixels)
  || /new Set\(pixels\)|colors\.add\(pixels\[offset\]\)/u.test(pixels)) {
  failures.push(`TASK7214_FAIL file=${pixelsFile} quantity=decoded bytes`);
}

if (failures.length) {
  console.error(failures.join("\n"));
  process.exitCode = 1;
} else {
  console.log(`TASK7214_PASS converted_tests=${TESTS.length} quantity=decoded RGB colours floor=32`);
}
