import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = new Set(process.argv.slice(2));
const injectGreyVoiceButton = args.has("--inject-grey-voice-button");

const gapListPath = path.join(repoRoot, "docs", "design", "osl-release-gap-list.md");
const anonymousCredentialsPath = path.join(repoRoot, "docs", "design", "anonymous-credentials.md");
const uiRoot = path.join(repoRoot, "apps", "osl-hub-ui");

function readText(filePath) {
  return readFileSync(filePath, "utf8");
}

function parseJsonBlock(markdown, heading) {
  const start = markdown.indexOf(heading);
  assert.notEqual(start, -1, `${heading} section is missing`);
  const match = markdown.slice(start).match(/```json\s+([\s\S]*?)\s+```/u);
  assert.ok(match, `${heading} JSON block is missing`);
  return JSON.parse(match[1]);
}

function listFiles(root, predicate) {
  const out = [];
  for (const entry of readdirSync(root)) {
    const full = path.join(root, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      if (entry === "node_modules" || entry === "dist" || entry === "screenshots") continue;
      out.push(...listFiles(full, predicate));
    } else if (predicate(full)) {
      out.push(full);
    }
  }
  return out;
}

function shippedScreenFiles() {
  const topLevelHtml = listFiles(uiRoot, (file) =>
    path.dirname(file) === uiRoot && file.endsWith(".html")
  );
  const productionSource = listFiles(path.join(uiRoot, "src"), (file) =>
    /\.(?:ts|tsx|js|mjs)$/u.test(file)
    && !/\.(?:test|spec)\.(?:ts|tsx|js|mjs)$/u.test(file)
    && !file.endsWith(".d.ts")
  );
  return [...topLevelHtml, ...productionSource].sort();
}

const voiceControlPattern =
  /<(?:button|a|input|select|textarea|label|summary)\b(?=[\s\S]{0,260}\b(?:voice|voice\s*note|call|microphone|mic|audio\s*room|record\s*audio|record\s*voice)\b)[\s\S]*?(?:>|<\/(?:button|a|select|textarea|label|summary)>)/giu;
const perSpeakerKeyClaimPattern =
  /\b(?:per[-\s]?speaker|speaker)\s+(?:media\s+)?keys?\b|\bvoice[-\s]?keys?\b|\bper[-\s]?speaker\s+encryption\b/giu;
const recordingClaimPattern =
  /\b(?:record(?:ing|ed)?\s+(?:audio|voice|call)|(?:audio|voice|call)\s+record(?:ing|ed)?|recordings?\s*,\s*transcripts?|recordings?\s+or\s+transcripts?)\b/giu;

function scanScreenText(file, text) {
  return [
    ...[...text.matchAll(voiceControlPattern)].map((match) => ({
      file,
      kind: "voice-control",
      text: match[0],
    })),
    ...[...text.matchAll(perSpeakerKeyClaimPattern)].map((match) => ({
      file,
      kind: "per-speaker-keys-claim",
      text: match[0],
    })),
    ...[...text.matchAll(recordingClaimPattern)].map((match) => ({
      file,
      kind: "recording-claim",
      text: match[0],
    })),
  ];
}

function screenViolations({ withGreyVoiceButton = false } = {}) {
  const files = shippedScreenFiles();
  const violations = [];
  for (const file of files) {
    let text = readText(file);
    if (withGreyVoiceButton && file.endsWith(path.join("apps", "osl-hub-ui", "index.html"))) {
      text += '\n<button class="button" type="button" disabled aria-disabled="true">Voice</button>\n';
    }
    violations.push(...scanScreenText(path.relative(repoRoot, file), text));
  }
  return { files, violations };
}

function assertGapList() {
  const gapList = parseJsonBlock(readText(gapListPath), "## Machine-readable gap list");
  const voiceEntries = gapList.entries.filter((entry) => entry.capability === "voice");
  const onlyVoiceEntry = voiceEntries[0] ?? {};
  const referenceText = JSON.stringify(onlyVoiceEntry);
  assert.equal(voiceEntries.length, 1, "gap list must hold exactly one voice entry");
  assert.match(referenceText, /Ruling 10/u);
  assert.match(referenceText, /PLAN-48H\.md merged plan/u);
  assert.match(referenceText, /File 21/u);
  assert.equal(onlyVoiceEntry.releaseState, "absent");
  assert.equal(onlyVoiceEntry.clientSurface, "absent");
  assert.equal(onlyVoiceEntry.serverWork, "unaffected");
  return {
    voiceEntryCount: voiceEntries.length,
    namesRuling10: /Ruling 10/u.test(referenceText),
    namesPlan48hMergedPlan: /PLAN-48H\.md merged plan/u.test(referenceText),
    namesFile21: /File 21/u.test(referenceText),
  };
}

function assertServerLearnsCopy() {
  const text = readText(anonymousCredentialsPath);
  const copyPattern = /The server learns "an authorized user fetched content X at\s+time Y," not "user U fetched content X at time Y\."/gu;
  const locations = [...text.matchAll(copyPattern)];
  const blockPattern = /<!-- OSL-4700-GATE: server-learns-honesty START -->[\s\S]*?The server learns "an authorized user fetched content X at\s+time Y," not "user U fetched content X at time Y\."[\s\S]*?<!-- OSL-4700-GATE: server-learns-honesty END -->/u;
  assert.equal(locations.length, 1, "server-learns honesty copy must appear exactly once");
  assert.match(text, blockPattern, "server-learns honesty copy must live inside the 4700 gate block");
  return {
    serverLearnsCopyLocations: locations.length,
    serverLearnsOutside4700GateBlock: blockPattern.test(text) ? 0 : locations.length,
  };
}

function assertShippedScreens() {
  const { files, violations } = screenViolations();
  assert.equal(violations.length, 0, violations.map((violation) =>
    `${violation.kind} in ${violation.file}: ${violation.text.slice(0, 160).replace(/\s+/gu, " ")}`
  ).join("\n"));
  return {
    scannedScreenFiles: files.length,
    shippedScreenVoiceControlsOrClaims: violations.length,
  };
}

function assertGreyVoiceButtonFails() {
  const { violations } = screenViolations({ withGreyVoiceButton: true });
  assert.ok(
    violations.some((violation) => violation.kind === "voice-control" && /disabled|aria-disabled|Voice/u.test(violation.text)),
    "injected grey voice button must be caught",
  );
  return {
    greyVoiceButtonMutantViolations: violations.length,
  };
}

function main() {
  const gap = assertGapList();
  const serverLearns = assertServerLearnsCopy();
  const screens = assertShippedScreens();

  if (injectGreyVoiceButton) {
    const mutant = screenViolations({ withGreyVoiceButton: true });
    console.log(`gap-list voice entries: ${gap.voiceEntryCount}`);
    console.log(`shipped screen files scanned: ${screens.scannedScreenFiles}`);
    console.log(`shipped screen voice controls or claims: ${screens.shippedScreenVoiceControlsOrClaims}`);
    console.log(`throwaway grey voice button violations: ${mutant.violations.length}`);
    for (const violation of mutant.violations) {
      console.log(`${violation.kind}: ${violation.file}: ${violation.text.slice(0, 160).replace(/\s+/gu, " ")}`);
    }
    throw new Error("throwaway grey voice button made the absence check fail");
  }

  const mutant = assertGreyVoiceButtonFails();
  console.log(`gap-list voice entries: ${gap.voiceEntryCount}`);
  console.log(`names Ruling 10: ${gap.namesRuling10}`);
  console.log(`names PLAN-48H.md merged plan: ${gap.namesPlan48hMergedPlan}`);
  console.log(`names File 21: ${gap.namesFile21}`);
  console.log(`shipped screen files scanned: ${screens.scannedScreenFiles}`);
  console.log(`shipped screen voice controls or claims: ${screens.shippedScreenVoiceControlsOrClaims}`);
  console.log(`server-learns honesty copy locations: ${serverLearns.serverLearnsCopyLocations}`);
  console.log(`server-learns honesty copy outside 4700 gate block: ${serverLearns.serverLearnsOutside4700GateBlock}`);
  console.log(`throwaway grey voice button violations: ${mutant.greyVoiceButtonMutantViolations}`);
  console.log("release voice absence check: PASS");
}

main();
