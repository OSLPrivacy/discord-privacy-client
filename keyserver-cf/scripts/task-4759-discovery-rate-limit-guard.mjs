#!/usr/bin/env node
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const PACKAGE_ROOT = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const ALLOWED_NATIVE_THRESHOLDS = new Set([5, 10, 120, 1200, 3600]);

function argValue(name, fallback) {
  const index = process.argv.indexOf(name);
  if (index === -1) return fallback;
  const raw = process.argv[index + 1];
  if (raw === undefined || !/^\d+$/u.test(raw)) {
    console.error(`${name} requires a numeric value`);
    process.exit(1);
  }
  return Number(raw);
}

const publishingThreshold = argValue("--publishing-threshold", 5);
const askingThreshold = argValue("--asking-threshold", 1200);

for (const threshold of [publishingThreshold, askingThreshold]) {
  if (!ALLOWED_NATIVE_THRESHOLDS.has(threshold)) {
    console.error(`unsupported native rate-limit threshold: ${threshold}`);
    process.exit(1);
  }
}

const discoverySources = [
  "src/endpoints/discovery-cards.ts",
  "src/lib/discovery-card.ts",
];

let limiterCalls = 0;
const unsupportedCalls = [];
for (const rel of discoverySources) {
  const source = readFileSync(path.join(PACKAGE_ROOT, rel), "utf8");
  for (const match of source.matchAll(/checkRateLimit\s*\(([\s\S]*?)\);/gu)) {
    limiterCalls += 1;
    const call = match[1] ?? "";
    for (const numberMatch of call.matchAll(/\b\d+\b/gu)) {
      const threshold = Number(numberMatch[0]);
      if (!ALLOWED_NATIVE_THRESHOLDS.has(threshold)) {
        unsupportedCalls.push(`${rel}:${threshold}`);
      }
    }
  }
}

console.log(`TASK4759 publish_threshold=${publishingThreshold}`);
console.log(`TASK4759 ask_threshold=${askingThreshold}`);
console.log(`TASK4759 discovery_limiter_calls=${limiterCalls}`);
console.log(`TASK4759 unsupported_discovery_limiter_calls=${unsupportedCalls.length}`);
if (unsupportedCalls.length > 0) {
  for (const call of unsupportedCalls) console.error(call);
  process.exit(1);
}
