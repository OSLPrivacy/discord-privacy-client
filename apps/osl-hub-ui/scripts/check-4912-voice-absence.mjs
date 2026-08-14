import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const uiRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function listFiles(root, predicate) {
  const files = [];
  for (const entry of readdirSync(root)) {
    const full = path.join(root, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      if (["dist", "node_modules", "screenshots", "scripts"].includes(entry)) continue;
      files.push(...listFiles(full, predicate));
    } else if (predicate(full)) {
      files.push(full);
    }
  }
  return files;
}

const shippedScreens = [
  ...listFiles(uiRoot, (file) => path.dirname(file) === uiRoot && file.endsWith(".html")),
  ...listFiles(path.join(uiRoot, "src"), (file) =>
    /\.(?:ts|tsx|js|mjs)$/u.test(file)
    && !/\.(?:test|spec)\.(?:ts|tsx|js|mjs)$/u.test(file)
    && !file.endsWith(".d.ts")),
].sort();

const voiceControlPattern = /<(?:button|a|input|select|textarea|label|summary)\b(?=[\s\S]{0,260}\b(?:voice|voice\s*note|call|microphone|mic|audio\s*room|record\s*audio|record\s*voice)\b)[\s\S]*?(?:>|<\/(?:button|a|select|textarea|label|summary)>)/giu;
const violations = shippedScreens.flatMap((file) =>
  [...readFileSync(file, "utf8").matchAll(voiceControlPattern)].map((match) => ({
    file: path.relative(uiRoot, file),
    control: match[0].replace(/\s+/gu, " ").slice(0, 180),
  })));

console.log(`TASK4912_SHIPPED_SCREEN_FILES=${shippedScreens.length}`);
console.log(`TASK4912_VOICE_CONTROLS=${violations.length}`);
for (const violation of violations) console.log(`TASK4912_VOICE_CONTROL file=${violation.file} control=${violation.control}`);
assert.equal(
  violations.length,
  0,
  `voice client is absent, but shipped voice controls were found:\n${violations.map((violation) => `${violation.file}: ${violation.control}`).join("\n")}`,
);
console.log("TASK4912_VOICE_ABSENCE=PASS");
