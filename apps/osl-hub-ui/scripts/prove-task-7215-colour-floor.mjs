#!/usr/bin/env node
/**
 * TASK 7215 - prove decoded-colour floors reject the pictures that fooled the
 * old compressed-byte census, and identify the limit of a colour-only check.
 *
 * Each case runs in a separate disposable copy.  The capture's DOM assertions
 * still execute, but its screenshot return is replaced with the named PNG.
 * This proves the image predicate itself rather than merely unit-testing a
 * helper.  D27(c) size changes are refused as non-comparable captures; noise
 * is deliberately reported as a finding because a colour count cannot certify
 * that a colourful image is the interface it is meant to depict.
 *
 *   node scripts/prove-task-7215-colour-floor.mjs
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { blankRgbaPng, rgbaPng } from "../screenshots/lib/png-test-fixtures.mjs";
import { countDistinctRgb, readPng } from "../screenshots/lib/png-pixels.mjs";

const APP_DIR = fileURLToPath(new URL("..", import.meta.url));
const REPO_DIR = fileURLToPath(new URL("../../..", import.meta.url));
const SCREENSHOTS = join(APP_DIR, "screenshots");
const CHECK_7214 = "scripts/check-task-7214-colour-floor.mjs";
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const FLOOR = 32;
const TESTS = [
  "task-0354-create-password-capture.test.mjs",
  "task-0357-restore-account-capture.test.mjs",
  "task-0373-cover-insertion-capture.test.mjs",
];
const INPUTS = ["blank", "two-colour", "solid-app-background", "noise", "downscaled", "cropped-empty-region"];
const D27C_INPUTS = new Set(["downscaled", "cropped-empty-region"]);
const LOW_COLOUR_INPUTS = new Set(["blank", "two-colour", "solid-app-background"]);
const STARVE = process.env.OSL7215_STARVE ?? "";
const IS_CHILD = process.env.OSL7215_CHILD === "1";
const STARVE_MODES = ["input", "converted-test", "d27c", "restoration"];
const PARTIAL = process.env.OSL7215_PARTIAL === "1";
const PARTIAL_TESTS = (process.env.OSL7215_TESTS ?? "").split(",").filter(Boolean);
const PARTIAL_INPUTS = (process.env.OSL7215_INPUTS ?? "").split(",").filter(Boolean);
if (STARVE && !STARVE_MODES.includes(STARVE)) throw new Error(`TASK7215_UNKNOWN_STARVE mode=${STARVE}`);

function noisePng(width, height) {
  let state = 0x7215c0de;
  return rgbaPng(width, height, () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state >>> 8;
  });
}

function inputPng(name) {
  switch (name) {
    case "blank": return blankRgbaPng(WINDOW.width, WINDOW.height, [255, 255, 255], { compressionLevel: 0 });
    case "two-colour": return rgbaPng(WINDOW.width, WINDOW.height, (x, y) => ((x + y) & 1 ? 0x080c0d : 0x2ac0f0), { compressionLevel: 0 });
    // Exact --bg / `background` design token used by the onboarding capture.
    case "solid-app-background": return blankRgbaPng(WINDOW.width, WINDOW.height, [0x08, 0x0c, 0x0d], { compressionLevel: 0 });
    case "noise": return noisePng(WINDOW.width, WINDOW.height);
    case "downscaled": return noisePng(128, 80);
    case "cropped-empty-region": return blankRgbaPng(640, 400, [0x08, 0x0c, 0x0d]);
    default: throw new Error(`TASK7215_UNKNOWN_INPUT input=${name}`);
  }
}

const workRoot = mkdtempSync(join(tmpdir(), "osl-task-7215-"));
const created = [];
const discarded = [];
const failures = [];

function makeCopy(name) {
  const dir = join(workRoot, name);
  mkdirSync(join(dir, "apps", "osl-hub-ui"), { recursive: true });
  cpSync(join(APP_DIR, "package.json"), join(dir, "apps", "osl-hub-ui", "package.json"));
  cpSync(SCREENSHOTS, join(dir, "apps", "osl-hub-ui", "screenshots"), {
    recursive: true,
    filter: (source) => !source.includes(`${join("screenshots", "artifacts")}`),
  });
  // Product source and the harness are read-only dependencies of these tests.
  symlinkSync(join(APP_DIR, "src"), join(dir, "apps", "osl-hub-ui", "src"), "dir");
  symlinkSync(join(APP_DIR, "node_modules"), join(dir, "apps", "osl-hub-ui", "node_modules"), "dir");
  symlinkSync(join(REPO_DIR, "scripts"), join(dir, "scripts"), "dir");
  mkdirSync(join(dir, "apps", "osl-hub-ui", "scripts"), { recursive: true });
  cpSync(join(APP_DIR, "scripts", "check-task-7214-colour-floor.mjs"), join(dir, "apps", "osl-hub-ui", CHECK_7214));
  created.push(dir);
  return dir;
}

function discardCopy(dir) {
  rmSync(dir, { recursive: true, force: true });
  if (existsSync(dir)) throw new Error(`TASK7215_COPY_NOT_DISCARDED dir=${dir}`);
  discarded.push(dir);
}

function runNode(dir, args) {
  const result = spawnSync(process.execPath, args, {
    cwd: join(dir, "apps", "osl-hub-ui"),
    encoding: "utf8",
    env: { ...process.env, OSL7215_CHILD: "1", OSL7215_STARVE: "" },
  });
  return { status: result.status, output: `${result.stdout ?? ""}${result.stderr ?? ""}` };
}

function replaceExactly(file, anchor, replacement, caseName) {
  const before = readFileSync(file, "utf8");
  const count = before.split(anchor).length - 1;
  if (count !== 1) throw new Error(`TASK7215_MUTATION_STARVED case=${caseName} anchor_occurrences=${count}`);
  writeFileSync(file, before.replace(anchor, replacement));
}

function runInputCase(file, input) {
  const caseName = `${file.replace(/\.test\.mjs$/u, "")}-${input}`;
  const dir = makeCopy(caseName);
  try {
    const inputFile = join(dir, "apps", "osl-hub-ui", "screenshots", "task-7215-input.png");
    const png = inputPng(input);
    writeFileSync(inputFile, png);
    const target = join(dir, "apps", "osl-hub-ui", "screenshots", file);
    replaceExactly(target, "const png = await page.screenshot({ fromSurface: true });", "const png = readFileSync(path.join(ARTIFACT_DIR, \"..\", \"task-7215-input.png\"));", caseName);
    // The test's artifact directory is where the test later writes its output;
    // inject from screenshots so that write cannot replace the chosen input.
    const result = runNode(dir, ["--test", "--test-concurrency=1", `screenshots/${file}`]);
    const colours = countDistinctRgb(readPng(png));
    if (LOW_COLOUR_INPUTS.has(input)) {
      const expected = `too few decoded distinct RGB colours: ${colours} (floor ${FLOOR})`;
      if (result.status !== 1 || !result.output.includes(expected)) {
        failures.push(`TASK7215_LOW_COLOUR_NOT_REFUSED file=${file} input=${input} exit=${result.status} expected="${expected}"`);
      }
      console.log(`TASK7215_INPUT file=${file} input=${input} exit=${result.status} decoded_colours=${colours} floor=${FLOOR} named=${result.output.includes(expected)}`);
    } else if (D27C_INPUTS.has(input)) {
      const expected = "D27(c) refuses capture PNG";
      if (result.status !== 1 || !result.output.includes(expected) || !result.output.includes("change what is compared")) {
        failures.push(`TASK7215_D27C_NOT_REFUSED file=${file} input=${input} exit=${result.status}`);
      }
      console.log(`TASK7215_D27C file=${file} input=${input} exit=${result.status} decoded_colours=${colours} named=${result.output.includes(expected)}`);
    } else {
      // This is an explicit finding, not a green claim: colour diversity alone
      // cannot establish that the picture depicts the interface.
      if (result.status !== 0 || colours < FLOOR) {
        failures.push(`TASK7215_NOISE_CONTROL_CHANGED file=${file} exit=${result.status} decoded_colours=${colours}`);
      }
      console.log(`TASK7215_FINDING file=${file} input=noise exit=${result.status} decoded_colours=${colours} floor=${FLOOR} result=colour-count-alone-cannot-identify-an-interface`);
    }
  } finally {
    discardCopy(dir);
  }
}

function run7214Mutation(name, relative, anchor, replacement, expectedQuantity) {
  const dir = makeCopy(name);
  try {
    replaceExactly(join(dir, relative), anchor, replacement, name);
    const result = runNode(dir, [CHECK_7214]);
    const reportedFile = relative.includes("/screenshots/task-")
      ? relative.slice(relative.lastIndexOf("/") + 1)
      : relative.replace("apps/osl-hub-ui/", "");
    const expected = `file=${reportedFile} quantity=${expectedQuantity}`;
    if (result.status !== 1 || !result.output.includes(expected)) {
      failures.push(`TASK7215_7214_MUTATION_NOT_CAUGHT mutation=${name} exit=${result.status} expected="${expected}"`);
    }
    console.log(`TASK7215_7214_MUTATION mutation=${name} exit=${result.status} named=${result.output.includes(expected)} ${expected}`);
  } finally {
    discardCopy(dir);
  }
}

function runRestoration(file) {
  const dir = makeCopy(`restored-${file}`);
  try {
    if (STARVE === "restoration") {
      replaceExactly(
        join(dir, "apps", "osl-hub-ui", "screenshots", file),
        "const DISTINCT_RGB_FLOOR = 32;",
        "const DISTINCT_RGB_FLOOR = 9999999;",
        `restoration-${file}`,
      );
    }
    const result = runNode(dir, ["--test", "--test-concurrency=1", `screenshots/${file}`]);
    const match = new RegExp(`TASK${file.slice(5, 9)}_DECODED_DISTINCT_RGB=(\\d+) floor=${FLOOR}`, "u").exec(result.output);
    if (result.status !== 0 || !match) {
      failures.push(`TASK7215_RESTORATION_ABSENT file=${file} exit=${result.status} restored_count=${match?.[1] ?? "none"}`);
    }
    console.log(`TASK7215_RESTORED file=${file} exit=${result.status} decoded_colours=${match?.[1] ?? "none"} floor=${FLOOR}`);
  } finally {
    discardCopy(dir);
  }
}

// Starvation children need only demonstrate that a missing required member is
// fatal and named.  They deliberately stop before browser work so this
// falsifiability control does not multiply the capture matrix fivefold.
if (IS_CHILD && STARVE) {
  const absent = {
    input: `TASK7215_ABSENT_INPUT input=${INPUTS[0]}`,
    "converted-test": `TASK7215_ABSENT_CONVERTED_TEST file=${TESTS[0]}`,
    d27c: "TASK7215_ABSENT_D27C_CHECK input=downscaled|cropped-empty-region",
    restoration: `TASK7215_RESTORATION_ABSENT file=${TESTS[0]} exit=starved`,
  }[STARVE];
  console.error("TASK7215_RESULT status=red");
  console.error(absent);
  process.exitCode = 1;
} else {
const activeTests = PARTIAL ? (PARTIAL_TESTS.length ? TESTS.filter((file) => PARTIAL_TESTS.includes(file)) : TESTS) : STARVE === "converted-test" ? TESTS.slice(1) : TESTS;
const activeInputs = PARTIAL ? (PARTIAL_INPUTS.length ? INPUTS.filter((input) => PARTIAL_INPUTS.includes(input)) : INPUTS) : STARVE === "input" ? INPUTS.slice(1) : INPUTS;
const activeD27c = STARVE === "d27c" ? new Set() : D27C_INPUTS;
if (!PARTIAL) {
  if (activeTests.length !== TESTS.length) failures.push(`TASK7215_ABSENT_CONVERTED_TEST file=${TESTS[0]}`);
  if (activeInputs.length !== INPUTS.length) failures.push(`TASK7215_ABSENT_INPUT input=${INPUTS[0]}`);
  if (activeD27c.size !== D27C_INPUTS.size) failures.push("TASK7215_ABSENT_D27C_CHECK input=downscaled|cropped-empty-region");
}

for (const file of activeTests) {
  for (const input of activeInputs) {
    if (D27C_INPUTS.has(input) && !activeD27c.has(input)) continue;
    runInputCase(file, input);
  }
}

if (!PARTIAL || process.env.OSL7215_RUN_MUTATIONS === "1") {
  run7214Mutation(
    "reverted-compressed-byte-census",
    "apps/osl-hub-ui/screenshots/task-0354-create-password-capture.test.mjs",
    "const decodedDistinctRgb = assertDecodedDistinctRgb(png, \"capture PNG\");",
    "const decodedDistinctRgb = new Set(png).size;",
    "compressed bytes",
  );
  run7214Mutation(
    "floor-counts-decoded-bytes",
    "apps/osl-hub-ui/screenshots/lib/png-pixels.mjs",
    "colors.add((pixels[offset] << 16) | (pixels[offset + 1] << 8) | pixels[offset + 2]);",
    "colors.add(pixels[offset]);",
    "decoded bytes",
  );
}

if (!PARTIAL || process.env.OSL7215_RUN_RESTORATION === "1") for (const file of TESTS) runRestoration(file);

const remaining = existsSync(workRoot) ? (await import("node:fs")).readdirSync(workRoot) : [];
rmSync(workRoot, { recursive: true, force: true });
if (remaining.length !== 0 || existsSync(workRoot)) failures.push(`TASK7215_COPIES_NOT_DISCARDED remaining=${remaining.length}`);

console.log(`TASK7215_MATRIX converted_tests=${TESTS.length} inputs=${INPUTS.length} low_colour_cases=${TESTS.length * LOW_COLOUR_INPUTS.size} d27c_cases=${TESTS.length * D27C_INPUTS.size} noise_findings=${TESTS.length}`);
console.log(`TASK7215_COPIES created=${created.length} discarded=${discarded.length} remaining=${remaining.length}`);

// A proof that does not fail when a required case is absent is decoration.
if (!PARTIAL && !IS_CHILD && !STARVE) {
  for (const mode of STARVE_MODES) {
    const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url)], {
      cwd: APP_DIR,
      encoding: "utf8",
      env: { ...process.env, OSL7215_CHILD: "1", OSL7215_STARVE: mode },
    });
    const output = `${child.stdout ?? ""}${child.stderr ?? ""}`;
    const named = /TASK7215_(?:ABSENT_[A-Z_]+|RESTORATION_ABSENT)[^\n]*/u.exec(output)?.[0] ?? "none";
    if (child.status !== 1 || named === "none") failures.push(`TASK7215_STARVATION_NOT_FATAL mode=${mode} exit=${child.status}`);
    console.log(`TASK7215_STARVATION mode=${mode} exit=${child.status} named="${named}"`);
  }
}

if (failures.length) {
  console.error("TASK7215_RESULT status=red");
  console.error(failures.join("\n"));
  process.exitCode = 1;
} else {
  console.log("TASK7215_RESULT status=green");
}
}
