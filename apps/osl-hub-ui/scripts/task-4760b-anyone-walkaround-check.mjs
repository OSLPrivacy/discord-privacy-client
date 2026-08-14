// TASK 4760b - THE NAMED CHECK. Walks the three doors round the "Anyone"
// consent gate and fails if any of them opens.
//
//   node scripts/task-4760b-anyone-walkaround-check.mjs
//
// Exit 0: every door was shut, the stored setting never moved, and no discovery
// card was published.
// Exit 1: printing `the setting reached anyone without consent` when the stored
// setting reached `anyone` or a card was published without a sealed consent
// stamp behind it; printing a different line when a refusal came back in the
// wrong words (that is a wording fault, not a breach, and must not be reported
// as one).
//
// The three doors, and the guard behind each:
//
//   1. The setting-change path called directly with `anyone` and no stamp.
//      Guard: the `value === DISCOVERY_ANYONE` branch in
//      `chooseDiscoverySetting`, and the stamp guard below.
//   2. `anyone` written straight into the stored settings file, then a restart.
//      Guard: THE guard -- the `anyoneConsentStampIsWhole(stamp)` line at the
//      top of `moveDiscoverySettingToAnyone`, the one statement in the app that
//      can put `anyone` into the stored setting.
//   3. The old Strip cycler. Searched for first; no copy survives, so the
//      tagged replica is clicked instead.
//
// Nothing here reads a document or a comment. Every number below comes from
// running the real module, bundled straight out of src/.
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync, appendFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import * as esbuild from "esbuild";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const AT = "2026-08-07T18:20:00.000Z";
const ANYONE_NEEDS_GATE = "discovery: anyone needs the consent gate";
const CORRECTED_REASON = `${ANYONE_NEEDS_GATE} (stored setting corrected to never)`;
const BREACH_LINE = "the setting reached anyone without consent";
const REPLICA_TAG = "OSL-OLD-STRIP-CYCLER-REPLICA";

const out = [];
const breaches = [];
const wordingFaults = [];
function say(line) {
  out.push(line);
}

/** Bundle the real module for node, so the check runs the shipped code. */
async function loadWalkaroundModule() {
  const dir = mkdtempSync(join(tmpdir(), "osl-4760b-bundle-"));
  const outfile = join(dir, "walkaround.mjs");
  await esbuild.build({
    entryPoints: [join(ROOT, "src", "discovery-anyone-walkaround.ts")],
    bundle: true,
    platform: "node",
    format: "esm",
    outfile,
    loader: { ".css": "empty", ".svg": "text", ".png": "dataurl" },
    logLevel: "silent",
  });
  return import(pathToFileURL(outfile).href);
}

// ---------------------------------------------------------------------------
// Door 3's search runs first: is there a copy of the old cycler to click?
// ---------------------------------------------------------------------------

const SEARCH_ROOTS = ["src", "screenshots", "scripts"];
const CODE_SUFFIXES = [".ts", ".tsx", ".mts", ".js", ".mjs", ".cjs", ".html", ".css"];

function codeFiles() {
  const found = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir)) {
      if (entry === "node_modules" || entry === "dist" || entry === "evidence") continue;
      const full = join(dir, entry);
      if (statSync(full).isDirectory()) walk(full);
      else if (CODE_SUFFIXES.some((suffix) => entry.endsWith(suffix))) found.push(full);
    }
  };
  for (const root of SEARCH_ROOTS) walk(join(ROOT, root));
  return found.sort();
}

/**
 * A copy of the old Strip cycler is code that steps the discovery setting from
 * one of the four values to the next. Anything carrying the replica tag was
 * planted by this task; anything else that matches would be a survivor.
 */
function searchForStripCyclers(files) {
  const replicas = [];
  const survivors = [];
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    // `cycler` as a whole word. Without the boundary this also catches
    // `bindWindowLifecycleRealignment`, which is not a discovery control.
    const mentionsCycler = /cycler\b/iu.test(text);
    const stepsTheSetting = /oldStripCyclerNextValue|% *OLD_STRIP_CYCLER_ORDER\.length/u.test(text);
    if (!mentionsCycler && !stepsTheSetting) continue;
    const where = relative(ROOT, file);
    if (text.includes(REPLICA_TAG)) replicas.push(where);
    else survivors.push(where);
  }
  return { replicas, survivors };
}

// ---------------------------------------------------------------------------

function record(label, { settingBefore, settingAfter, cardsPublished, refusal, expectRefusal = ANYONE_NEEDS_GATE }) {
  if (settingAfter === "anyone") {
    breaches.push(`${label} left the stored setting on anyone (was ${settingBefore})`);
  }
  if (cardsPublished !== 0) {
    breaches.push(`${label} published ${cardsPublished} discovery card(s)`);
  }
  if (settingAfter !== settingBefore) {
    breaches.push(`${label} moved the stored setting ${settingBefore} -> ${settingAfter}`);
  }
  if (refusal !== expectRefusal) {
    wordingFaults.push(`${label} was refused with ${JSON.stringify(refusal)}, expected ${JSON.stringify(expectRefusal)}`);
  }
}

const module_ = await loadWalkaroundModule();
const {
  bindOldStripCycler: _unusedBinding,
  clickOldStripCycler,
  createDiscoveryCardLedger,
  createDiscoverySettingStore,
  directAnyoneProbes,
  oldStripCyclerNextValue,
  publishDiscoveryCard,
  publishedCardCount,
  publishAfterRealConsent,
  readDiscoverySetting,
  restartDiscoveryFromSettingsFileText,
  runDirectAnyoneProbe,
  serialiseDiscoverySettingsFile,
} = module_;

void _unusedBinding;

// === Attempt 1 =============================================================
// The setting-change path, called directly with `anyone` and no consent stamp.

const START_1 = "allowed";
const probes = directAnyoneProbes();
const probeRows = [];
for (const probe of probes) {
  const outcome = runDirectAnyoneProbe(probe, START_1, AT);
  record(`attempt 1 [${outcome.probe}]`, { ...outcome, expectRefusal: outcome.expect });
  probeRows.push(
    `${outcome.probe} ok=${outcome.ok} refusal="${outcome.refusal}" as_promised=${outcome.refusal === outcome.expect}`
    + ` before=${outcome.settingBefore} after=${outcome.settingAfter}`
    + ` equal=${outcome.settingAfter === outcome.settingBefore} cards=${outcome.cardsPublished}`,
  );
}
// The headline for attempt 1 is the setting-change path itself, asked for
// `anyone` with no consent stamp anywhere near it.
const attempt1 = runDirectAnyoneProbe(probes[0], START_1, AT);
say(
  `TASK4760B_ATTEMPT_1 direct_setting_change_path probes=${probes.length} all_refused=${probeRows.every((row) => row.includes("ok=false"))}`
  + ` refusal="${attempt1.refusal}" exact_words=${attempt1.refusal === ANYONE_NEEDS_GATE}`
  + ` setting_before=${attempt1.settingBefore} setting_after=${attempt1.settingAfter}`
  + ` equal=${attempt1.settingAfter === attempt1.settingBefore} cards_published=${attempt1.cardsPublished}`,
);
for (const row of probeRows) say(`TASK4760B_ATTEMPT_1_PROBE ${row}`);

// === Attempt 2 =============================================================
// `anyone` written straight into the stored settings file, then a restart.

const profileDir = mkdtempSync(join(tmpdir(), "osl-4760b-profile-"));
mkdirSync(profileDir, { recursive: true });
const settingsPath = join(profileDir, "discovery-settings.json");
const logPath = join(profileDir, "discovery-corrections.log");

// The profile as it stood before anyone touched it: a fresh one, on `never`.
const START_2 = "never";
const honestStore = createDiscoverySettingStore({ setting: START_2 });
writeFileSync(settingsPath, serialiseDiscoverySettingsFile(honestStore), "utf8");
const beforeTamperText = readFileSync(settingsPath, "utf8");
const beforeTamperSetting = JSON.parse(beforeTamperText).setting;

// The tamper: `anyone` written straight in, by hand, with no stamp beside it.
const tamperedText = beforeTamperText.replace(`"setting": "${START_2}"`, '"setting": "anyone"');
writeFileSync(settingsPath, tamperedText, "utf8");
const onDiskTampered = JSON.parse(readFileSync(settingsPath, "utf8")).setting;

// The restart: re-read the file from disk, build the store from it, write any
// correction back to disk, and log the reason.
// The log starts empty and on disk, so a restart that logs nothing reads back
// as nothing rather than crashing this check. A check that dies instead of
// reporting cannot say what it found.
writeFileSync(logPath, "", "utf8");
const restart = restartDiscoveryFromSettingsFileText(readFileSync(settingsPath, "utf8"), AT);
if (restart.fileChanged) writeFileSync(settingsPath, restart.correctedText, "utf8");
for (const line of restart.logLines) appendFileSync(logPath, `${line}\n`, "utf8");

const ledger2 = createDiscoveryCardLedger();
publishDiscoveryCard(restart.store, ledger2, "@someone", AT);
const cards2 = publishedCardCount(ledger2);

// Read the file back off disk after the restart, and read it again through a
// second restart, to prove the correction stuck rather than being re-corrected
// every time.
const afterRestartText = readFileSync(settingsPath, "utf8");
const afterRestartOnDisk = JSON.parse(afterRestartText).setting;
const secondRestart = restartDiscoveryFromSettingsFileText(afterRestartText, AT);
const loggedText = readFileSync(logPath, "utf8");

record("attempt 2 [tampered settings file, restart]", {
  settingBefore: beforeTamperSetting,
  settingAfter: restart.settingAfterRestart,
  cardsPublished: cards2,
  refusal: restart.corrections[0] ?? "",
  expectRefusal: CORRECTED_REASON,
});
if (afterRestartOnDisk !== "never") {
  breaches.push(`attempt 2 left ${JSON.stringify(afterRestartOnDisk)} in the settings file on disk, expected "never"`);
}
if (!loggedText.includes(ANYONE_NEEDS_GATE)) {
  wordingFaults.push(`attempt 2 logged no reason carrying ${JSON.stringify(ANYONE_NEEDS_GATE)}`);
}

say(
  `TASK4760B_ATTEMPT_2 tampered_settings_file_then_restart file=${basename(settingsPath)}`
  + ` refusal="${restart.corrections[0] ?? ""}"`
  + ` exact_words=${(restart.corrections[0] ?? "").startsWith(ANYONE_NEEDS_GATE)}`
  + ` setting_before=${beforeTamperSetting} setting_after=${restart.settingAfterRestart}`
  + ` equal=${restart.settingAfterRestart === beforeTamperSetting} cards_published=${cards2}`,
);
say(
  `TASK4760B_ATTEMPT_2_FILE on_disk_before=${JSON.stringify(beforeTamperSetting)}`
  + ` on_disk_tampered=${JSON.stringify(onDiskTampered)}`
  + ` on_disk_after_restart=${JSON.stringify(afterRestartOnDisk)}`
  + ` corrected_to_never=${afterRestartOnDisk === "never"} file_rewritten=${restart.fileChanged}`
  + ` second_restart_setting=${secondRestart.settingAfterRestart}`
  + ` second_restart_corrections=${secondRestart.corrections.length}`,
);
const loggedLines = loggedText.trim() === "" ? [] : loggedText.trim().split("\n");
say(`TASK4760B_ATTEMPT_2_LOG path=${basename(logPath)} lines=${loggedLines.length} reason="${loggedText.trim()}"`);

// The same tamper from every other starting value, and with a forged stamp
// beside it. The correction always goes to `never` -- the safest of the four --
// and never to the value the tampered file claimed.
const tamperRows = [];
for (const [shape, text] of [
  ["anyone, no stamp", JSON.stringify({ setting: "anyone" })],
  ["anyone, null stamp", JSON.stringify({ setting: "anyone", consentStamp: null })],
  ["anyone, stamp with no seal", JSON.stringify({ setting: "anyone", consentStamp: { recordedDate: AT, wordingVersion: "anyone-consent-v1+cdd02ec0" } })],
  ["anyone, forged seal", JSON.stringify({ setting: "anyone", consentStamp: { recordedDate: AT, wordingVersion: "anyone-consent-v1+cdd02ec0", seal: "deadbeef" } })],
  ["anyone, stamp with no date", JSON.stringify({ setting: "anyone", consentStamp: { wordingVersion: "anyone-consent-v1+cdd02ec0", seal: "e2dee5a1" } })],
  ["anyone, stamp with no wording version", JSON.stringify({ setting: "anyone", consentStamp: { recordedDate: AT, seal: "e2dee5a1" } })],
  ["anyone, stamp is a string", JSON.stringify({ setting: "anyone", consentStamp: "yes" })],
  ["anyone, stamp is true", JSON.stringify({ setting: "anyone", consentStamp: true })],
  ["ANYONE in capitals", JSON.stringify({ setting: "ANYONE" })],
  ["anyone with trailing space", JSON.stringify({ setting: "anyone " })],
  ["file is not JSON at all", "setting=anyone"],
]) {
  const shapeDir = restartDiscoveryFromSettingsFileText(text, AT);
  const shapeLedger = createDiscoveryCardLedger();
  publishDiscoveryCard(shapeDir.store, shapeLedger, "@someone", AT);
  const cards = publishedCardCount(shapeLedger);
  if (shapeDir.settingAfterRestart === "anyone") breaches.push(`attempt 2 shape [${shape}] restarted on anyone`);
  if (cards !== 0) breaches.push(`attempt 2 shape [${shape}] published ${cards} card(s)`);
  tamperRows.push(`${shape} -> ${shapeDir.settingAfterRestart} cards=${cards}`);
}
say(`TASK4760B_ATTEMPT_2_SHAPES ${tamperRows.length} shapes | ${tamperRows.join(" | ")}`);

// === Attempt 3 =============================================================
// The old Strip cycler, if any copy of it still exists.

const files = codeFiles();
const { replicas, survivors } = searchForStripCyclers(files);
say(
  `TASK4760B_ATTEMPT_3_SEARCH roots=${SEARCH_ROOTS.join(",")} files_read=${files.length}`
  + ` surviving_copies=${survivors.length} planted_replicas=${replicas.length}`
  + ` replicas=${replicas.join(",") || "none"} survivors=${survivors.join(",") || "none"}`,
);

// No copy survives, so the tagged replica is what gets clicked. It is stepped
// all the way round the cycle, so the click that asks for `anyone` is a real
// click on a real control and not a special case.
const START_3 = "never";
const store3 = createDiscoverySettingStore({ setting: START_3 });
const ledger3 = createDiscoveryCardLedger();
const clicks = [];
let askedForAnyone = null;
for (let click = 0; click < 6; click += 1) {
  const before = readDiscoverySetting(store3);
  const result = clickOldStripCycler(store3);
  publishDiscoveryCard(store3, ledger3, "@someone", AT);
  const after = readDiscoverySetting(store3);
  clicks.push(`click${click + 1} from=${result.from} asked=${result.asked} ok=${result.ok} refusal="${result.refusal}" after=${after}`);
  if (result.asked === "anyone") {
    askedForAnyone = { before, result, after, cards: publishedCardCount(ledger3) };
    if (after === "anyone") breaches.push("attempt 3 [old Strip cycler] cycled into anyone");
    if (after !== before) breaches.push(`attempt 3 [old Strip cycler] moved the setting ${before} -> ${after}`);
    if (result.refusal !== ANYONE_NEEDS_GATE) {
      wordingFaults.push(`attempt 3 was refused with ${JSON.stringify(result.refusal)}, expected ${JSON.stringify(ANYONE_NEEDS_GATE)}`);
    }
  }
}
const cards3 = publishedCardCount(ledger3);
if (cards3 !== 0) breaches.push(`attempt 3 published ${cards3} discovery card(s)`);
if (askedForAnyone === null) wordingFaults.push("attempt 3 never reached the click that asks for anyone");

say(
  `TASK4760B_ATTEMPT_3 old_strip_cycler_replica clicks=${clicks.length}`
  + ` refusal="${askedForAnyone?.result.refusal ?? ""}" exact_words=${askedForAnyone?.result.refusal === ANYONE_NEEDS_GATE}`
  + ` setting_before=${askedForAnyone?.before} setting_after=${askedForAnyone?.after}`
  + ` equal=${askedForAnyone?.after === askedForAnyone?.before} cards_published=${cards3}`,
);
for (const click of clicks) say(`TASK4760B_ATTEMPT_3_CLICK ${click}`);
say(`TASK4760B_ATTEMPT_3_CYCLE_ORDER ${["never", "allowed", "shared-room", "anyone"].map((v) => `${v}->${oldStripCyclerNextValue(v)}`).join(" ")}`);

// === The card counter is not decoration ====================================

const live = publishAfterRealConsent("allowed", AT);
if (live.cardsPublished !== 1 || live.settingAfter !== "anyone") {
  wordingFaults.push(
    `the card counter never counts: a real ticked gate published ${live.cardsPublished} card(s)`
    + ` and left the setting on ${live.settingAfter}`,
  );
}
say(
  `TASK4760B_CARD_COUNTER_LIVE real_gate_ticked_and_turned_on setting_before=${live.settingBefore}`
  + ` setting_after=${live.settingAfter} cards_published=${live.cardsPublished}`
  + ` wording_version=${live.wordingVersion}`,
);

// === The tally =============================================================

const totalCards = attempt1.cardsPublished + cards2 + cards3;
const attemptsRefused = [
  attempt1.refusal === ANYONE_NEEDS_GATE && attempt1.settingAfter === attempt1.settingBefore,
  (restart.corrections[0] ?? "").startsWith(ANYONE_NEEDS_GATE) && restart.settingAfterRestart === beforeTamperSetting,
  askedForAnyone?.result.refusal === ANYONE_NEEDS_GATE && askedForAnyone?.after === askedForAnyone?.before,
].filter(Boolean).length;
say(
  `TASK4760B_TALLY attempts=3 refused_with_the_exact_words=${attemptsRefused}`
  + ` breaches=${breaches.length} wording_faults=${wordingFaults.length} cards_published_total=${totalCards}`,
);
if (attemptsRefused !== 3) wordingFaults.push(`only ${attemptsRefused} of 3 attempts were refused with the exact words`);

for (const line of out) console.log(line);

if (breaches.length > 0) {
  console.log(BREACH_LINE);
  for (const breach of breaches) console.log(`TASK4760B_BREACH ${breach}`);
  console.log(`TASK4760B_RESULT walked round the gate: ${breaches.length} breach(es)`);
  process.exit(1);
}
if (wordingFaults.length > 0) {
  console.log("TASK4760B_WORDING_FAULT a door was shut but not in the words the gate promises");
  for (const fault of wordingFaults) console.log(`TASK4760B_WORDING_FAULT ${fault}`);
  process.exit(1);
}
console.log(`TASK4760B_RESULT all 3 attempts refused with "${ANYONE_NEEDS_GATE}", 0 cards published`);
process.exit(0);
