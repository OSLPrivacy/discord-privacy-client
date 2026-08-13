#!/usr/bin/env node
// TASK 4095's narrow starvation guard.  This is intentionally a source guard
// around the *shipping* mapper, not a fixture label: blanking `row.name`
// removes every row's only text route before it reaches SignalOpenScreenMessage.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const sourcePath = resolve(process.argv[2] ?? "apps/osl-hub/src/native_signal_row_words.rs");
const source = readFileSync(sourcePath, "utf8");
const required = [
  ["row.name.trim()", "live-row-name-text"],
  ["SignalOpenScreenMessage::new(", "shipping-open-screen-message"],
  [".with_published_name_and_position(", "position-bound-published-name"],
];
const starved = required.filter(([needle]) => !source.includes(needle)).map(([, marker]) => marker);
if (starved.length > 0) {
  console.error(`TASK4095 exit 1: starved markers=${starved.join(",")}`);
  process.exit(1);
}
console.log("TASK4095 exit 0: shipping Signal text route and position-bound published-name route present");
