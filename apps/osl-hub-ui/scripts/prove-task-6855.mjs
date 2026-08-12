#!/usr/bin/env node
/**
 * TASK 6855 - prove coach-tip dismissal persists privately.
 *
 * For every mutation this script makes a SEPARATE THROWAWAY COPY of the files
 * TASK 6854's check reads, edits the real product source inside that copy, runs
 * the unmodified TASK 6854 check there, and then deletes the copy. Nothing is
 * mutated in the working tree and no copy outlives its mutation.
 *
 * A mutation counts as caught only when BOTH hold in its own copy:
 *   1. `check-task-6854.ts` exits 1, and
 *   2. `locate-coach-tip-6855.ts` names a fault of the mutated family together
 *      with the tip id and the profile id it was observed in.
 *
 * Then a pristine copy must run the same two programs green, with the first-use
 * flows restored, before the proof will pass.
 *
 * Finally the proof re-runs itself three times with a starvation injected -- a
 * mutant that does not mutate, an observer that does not observe, a restoration
 * that is not restored -- and requires each of those runs to exit 1. A red proof
 * that cannot go red is decoration.
 *
 *   node scripts/prove-task-6855.mjs
 *   OSL6855_STARVE=mutant|observer|restoration node scripts/prove-task-6855.mjs
 */

import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const APP_DIR = fileURLToPath(new URL("..", import.meta.url));
const REPO_DIR = fileURLToPath(new URL("../../..", import.meta.url));
const VITE_NODE = join(APP_DIR, "node_modules/vite-node/vite-node.mjs");
const COACH_TIPS_REL = "apps/osl-hub-ui/src/coach-tips.ts";

/** Exactly the files check-task-6854.ts imports or reads, and nothing else. */
const COPIED_FILES = [
  "apps/osl-hub-ui/package.json",
  "apps/osl-hub-ui/tsconfig.json",
  "apps/osl-hub-ui/scripts/check-task-6854.ts",
  "apps/osl-hub-ui/scripts/locate-coach-tip-6855.ts",
  COACH_TIPS_REL,
  "apps/osl-hub-ui/src/secure-local-store.ts",
  "apps/osl-hub/src/coach_tips.rs",
  "apps/osl-hub/src/identity_registry.rs",
  "apps/osl-hub/src/main.rs",
  "crates/ipc/src/commands.rs",
  "crates/ipc/src/main_password.rs",
];

const STARVE = process.env.OSL6855_STARVE ?? "";
const IS_CHILD = process.env.OSL6855_CHILD === "1";
const STARVE_MODES = ["mutant", "observer", "restoration"];
if (STARVE && !STARVE_MODES.includes(STARVE)) {
  throw new Error(`TASK6855_UNKNOWN_STARVE_MODE mode=${STARVE}`);
}

const MODULE_STATE_ANCHOR = "function definition(id: CoachTipId): CoachTipDefinition {";
const PERSIST_ANCHOR = `  async #persist(document: CoachTipProfileDocument): Promise<void> {
    await this.#store.setItem(ENCRYPTED_PROFILE_UI_STATE_KEY, serializeDocument(document));
  }`;
const LOAD_ANCHOR = "    this.#document = parseDocument(await this.#store.getItem(ENCRYPTED_PROFILE_UI_STATE_KEY));";
const CONSTRUCTOR_ANCHOR = `  constructor(store: EncryptedProfileStateStore) {
    this.#store = store;
  }`;
const ELIGIBLE_ANCHOR = `    return tip.context === context
      && availableControls.includes(tip.controlSelector)
      && (this.#document.dismissed[id] ?? 0) < tip.revision;`;
const DISMISS_ANCHOR = `  async dismiss(id: CoachTipId): Promise<void> {
    const tip = definition(id);`;
const DISMISS_ALL_ANCHOR = "  async dismissAll(): Promise<void> {";
const RESET_ANCHOR = `  async reset(): Promise<void> {
    const next = emptyDocument();`;
const MARKUP_ANCHOR = `  markup(id: CoachTipId, context: CoachTipContext, availableControls: readonly string[]): string {
    if (!this.isEligible(id, context, availableControls)) return "";`;
const STORE_ANCHOR = `  return new SecureLocalStore({
    storage,
    key,
    namespace: "osl-encrypted-profile-v1",
    randomBytes,
  });`;

/**
 * Ten mutations over the five families the finish line names. Each one edits the
 * shipped coach-tip source the way a real regression would, not a test switch.
 */
const MUTATIONS = [
  {
    name: "persistence-never-writes-dismissal",
    family: "persistence",
    expectedFaults: ["restart-forgot-dismissal", "encrypted-sync-lost-dismissal"],
    edits: [[PERSIST_ANCHOR, `  async #persist(document: CoachTipProfileDocument): Promise<void> {
    void document;
  }`]],
  },
  {
    name: "persistence-restart-discards-stored-document",
    family: "persistence",
    expectedFaults: ["restart-forgot-dismissal", "encrypted-sync-lost-dismissal"],
    edits: [[LOAD_ANCHOR, `    await this.#store.getItem(ENCRYPTED_PROFILE_UI_STATE_KEY);
    this.#document = emptyDocument();`]],
  },
  {
    name: "isolation-dismissals-shared-through-module-state",
    family: "isolation",
    expectedFaults: ["dismissal-leaked-to-other-profile"],
    edits: [
      [MODULE_STATE_ANCHOR, `let sharedDismissed6855: Partial<Record<CoachTipId, number>> = {};

${MODULE_STATE_ANCHOR}`],
      [LOAD_ANCHOR, `    const parsed = parseDocument(await this.#store.getItem(ENCRYPTED_PROFILE_UI_STATE_KEY));
    sharedDismissed6855 = { ...sharedDismissed6855, ...parsed.dismissed };
    this.#document = { ...parsed, dismissed: { ...sharedDismissed6855 } };`],
      [PERSIST_ANCHOR, `  async #persist(document: CoachTipProfileDocument): Promise<void> {
    sharedDismissed6855 = { ...sharedDismissed6855, ...document.dismissed };
    await this.#store.setItem(ENCRYPTED_PROFILE_UI_STATE_KEY, serializeDocument(document));
  }`],
    ],
  },
  {
    name: "isolation-device-wide-dismissal-cache",
    family: "isolation",
    expectedFaults: ["dismissal-leaked-to-other-profile"],
    edits: [
      [MODULE_STATE_ANCHOR, `const deviceWideDismissals6855 = new Set<string>();

${MODULE_STATE_ANCHOR}`],
      [ELIGIBLE_ANCHOR, `    return tip.context === context
      && availableControls.includes(tip.controlSelector)
      && !deviceWideDismissals6855.has(id)
      && (this.#document.dismissed[id] ?? 0) < tip.revision;`],
      [DISMISS_ANCHOR, `${DISMISS_ANCHOR}
    deviceWideDismissals6855.add(id);`],
      [DISMISS_ALL_ANCHOR, `${DISMISS_ALL_ANCHOR}
    for (const tip of coachTipCatalog) deviceWideDismissals6855.add(tip.id);`],
      [RESET_ANCHOR, `${RESET_ANCHOR}
    deviceWideDismissals6855.clear();`],
    ],
  },
  {
    name: "privacy-plaintext-behavioral-record-in-store",
    family: "privacy",
    expectedFaults: ["plaintext-tip-meaning-in-store"],
    edits: [[STORE_ANCHOR, `  const sealed = new SecureLocalStore({
    storage,
    key,
    namespace: "osl-encrypted-profile-v1",
    randomBytes,
  });
  return {
    getItem: (logicalKey: string) => sealed.getItem(logicalKey),
    setItem: async (logicalKey: string, value: string) => {
      storage.setItem("osl-coach-tip-usage", \`coach-tip dismiss impression \${value}\`);
      await sealed.setItem(logicalKey, value);
    },
  };`]],
  },
  {
    name: "privacy-uploads-behavioral-analytics",
    family: "privacy",
    expectedFaults: ["behavioral-analytics-uploaded"],
    edits: [[DISMISS_ANCHOR, `${DISMISS_ANCHOR}
    await fetch("https://analytics.example.invalid/coach-tip-impression", {
      method: "POST",
      body: JSON.stringify({ tip: id, event: "dismiss-click", revision: tip.revision }),
    }).catch(() => undefined);`]],
  },
  {
    name: "reset-does-not-restore-anything",
    family: "reset",
    expectedFaults: ["reset-did-not-restore-tip"],
    edits: [[RESET_ANCHOR, `  async reset(): Promise<void> {
    const next = this.#document;`]],
  },
  {
    name: "reset-restores-only-the-first-tip",
    family: "reset",
    expectedFaults: ["reset-did-not-restore-tip"],
    edits: [[RESET_ANCHOR, `  async reset(): Promise<void> {
    const next = {
      ...this.#document,
      dismissed: { ...this.#document.dismissed, "protect-message": undefined },
    } as CoachTipProfileDocument;`]],
  },
  {
    name: "usability-hidden-tip-disables-control",
    family: "usability",
    expectedFaults: ["control-blocked-when-tip-hidden"],
    edits: [[MARKUP_ANCHOR, `  markup(id: CoachTipId, context: CoachTipContext, availableControls: readonly string[]): string {
    if (!this.isEligible(id, context, availableControls)) {
      const hidden = definition(id);
      if (hidden.context !== context || !availableControls.includes(hidden.controlSelector)) return "";
      return \`<span data-coach-tip-hidden="\${hidden.id}"><style>\${hidden.controlSelector}{display:none}</style></span><span data-coach-tip-disable="\${hidden.controlSelector}" disabled></span>\`;
    }`]],
  },
  {
    name: "usability-hidden-tip-keeps-occupying-control-region",
    family: "usability",
    expectedFaults: ["control-region-occupied-when-tip-hidden"],
    edits: [[MARKUP_ANCHOR, `  markup(id: CoachTipId, context: CoachTipContext, availableControls: readonly string[]): string {
    if (!this.isEligible(id, context, availableControls)) {
      const hidden = definition(id);
      if (hidden.context !== context || !availableControls.includes(hidden.controlSelector)) return "";
      return \`<aside class="coach-tip coach-tip-collapsed" data-coach-tip="\${hidden.id}" aria-hidden="true"></aside>\`;
    }`]],
  },
];

/** The starve-mutant control: a mutation that only adds a comment. */
const INERT_MUTATION = {
  name: "starved-mutant-comment-only",
  family: "starvation-control",
  expectedFaults: ["restart-forgot-dismissal"],
  edits: [[MODULE_STATE_ANCHOR, `// TASK 6855 starvation control: this edit changes no behaviour.
${MODULE_STATE_ANCHOR}`]],
};

const workRoot = mkdtempSync(join(tmpdir(), "osl-6855-copies-"));
// No copy may outlive this process, including on an unhandled failure.
process.on("exit", () => { rmSync(workRoot, { recursive: true, force: true }); });
const created = [];
const discarded = [];
let copySequence = 0;

function makeCopy(label) {
  copySequence += 1;
  const dir = join(workRoot, `${String(copySequence).padStart(2, "0")}-${label}`);
  mkdirSync(dir);
  created.push(dir);
  for (const relative of COPIED_FILES) {
    const source = join(REPO_DIR, relative);
    if (!existsSync(source)) throw new Error(`TASK6855_COPY_SOURCE_MISSING file=${relative}`);
    const destination = join(dir, relative);
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(source, destination);
    if (!existsSync(destination)) throw new Error(`TASK6855_COPY_INCOMPLETE file=${relative}`);
  }
  symlinkSync(join(APP_DIR, "node_modules"), join(dir, "apps/osl-hub-ui/node_modules"), "dir");
  return dir;
}

function discardCopy(dir) {
  rmSync(dir, { recursive: true, force: true });
  if (existsSync(dir)) throw new Error(`TASK6855_COPY_NOT_DISCARDED dir=${dir}`);
  discarded.push(dir);
}

function applyMutation(dir, mutation) {
  const file = join(dir, COACH_TIPS_REL);
  let source = readFileSync(file, "utf8");
  for (const [anchor, replacement] of mutation.edits) {
    const occurrences = source.split(anchor).length - 1;
    if (occurrences !== 1) {
      throw new Error(`TASK6855_MUTANT_STARVED mutation=${mutation.name} anchor_occurrences=${occurrences}`);
    }
    source = source.replace(anchor, replacement);
  }
  const before = readFileSync(file, "utf8");
  if (source === before) throw new Error(`TASK6855_MUTANT_STARVED mutation=${mutation.name} reason=no-op`);
  writeFileSync(file, source);
}

function runIn(dir, script, extraEnv = {}) {
  const result = spawnSync(process.execPath, [VITE_NODE, script], {
    cwd: join(dir, "apps/osl-hub-ui"),
    encoding: "utf8",
    env: { ...process.env, OSL6855_CHILD: "1", OSL6855_STARVE: "", ...extraEnv },
  });
  return { status: result.status, output: `${result.stdout ?? ""}${result.stderr ?? ""}` };
}

function namedFaults(output) {
  return [...output.matchAll(/^TASK6855_NAMED tip=(\S+) profile=(\S+) fault=(\S+)$/gmu)]
    .map(([, tip, profile, fault]) => ({ tip, profile, fault }));
}

const KNOWN_TIPS = new Set(["protect-message", "private-scan", "switch-profile"]);
const KNOWN_PROFILES = new Set(["profile-a-device-1", "profile-a-device-2", "profile-b"]);

const failures = [];

for (const mutation of MUTATIONS) {
  const dir = makeCopy(mutation.name);
  try {
    // starve=mutant leaves the copy pristine, so the mutation is never made.
    // A mutation whose anchor no longer exists is a starved mutant, not a pass:
    // it is recorded as a failure so the run still reaches its cleanup.
    if (STARVE !== "mutant") {
      try {
        applyMutation(dir, mutation);
      } catch (error) {
        failures.push(`${error.message} family=${mutation.family}`);
        console.log(`TASK6855_MUTANT family=${mutation.family} mutation=${mutation.name} gate_exit=starved fault=none tips=none profiles=none named=0`);
        continue;
      }
    }

    const gate = runIn(dir, "scripts/check-task-6854.ts");
    const assertion = /AssertionError[^\n]*?:\s*([^\n]+)/u.exec(gate.output)?.[1]?.trim()
      ?? /^\s*Error:\s*([^\n]+)/mu.exec(gate.output)?.[1]?.trim()
      ?? "";

    const locate = runIn(dir, "scripts/locate-coach-tip-6855.ts", {
      OSL6855_STARVE_OBSERVER: STARVE === "observer" ? "1" : "0",
    });
    const found = namedFaults(locate.output);
    const matching = found.filter((fault) => mutation.expectedFaults.includes(fault.fault)
      && KNOWN_TIPS.has(fault.tip)
      && KNOWN_PROFILES.has(fault.profile));

    if (gate.status !== 1) {
      failures.push(`TASK6855_MUTATION_NOT_CAUGHT mutation=${mutation.name} family=${mutation.family} gate_exit=${gate.status}`);
    }
    if (matching.length === 0) {
      failures.push(`TASK6855_MUTATION_NOT_NAMED mutation=${mutation.name} family=${mutation.family} expected=${mutation.expectedFaults.join("|")} named=${found.length}`);
    }

    const tips = [...new Set(matching.map((fault) => fault.tip))].sort().join(",") || "none";
    const profiles = [...new Set(matching.map((fault) => fault.profile))].sort().join(",") || "none";
    console.log(`TASK6855_MUTANT family=${mutation.family} mutation=${mutation.name} gate_exit=${gate.status} fault=${[...new Set(matching.map((f) => f.fault))].join(",") || "none"} tips=${tips} profiles=${profiles} named=${matching.length}`);
    if (assertion) console.log(`  gate_assertion="${assertion}"`);
  } finally {
    discardCopy(dir);
  }
}

// The starve-mutant control: an edit that changes nothing must NOT read as caught.
{
  const dir = makeCopy(INERT_MUTATION.name);
  try {
    try {
      applyMutation(dir, INERT_MUTATION);
    } catch (error) {
      failures.push(error.message);
    }
    const gate = runIn(dir, "scripts/check-task-6854.ts");
    const locate = runIn(dir, "scripts/locate-coach-tip-6855.ts");
    const found = namedFaults(locate.output);
    if (gate.status !== 0 || found.length !== 0) {
      failures.push(`TASK6855_INERT_CONTROL_UNSOUND gate_exit=${gate.status} named=${found.length}`);
    }
    console.log(`TASK6855_INERT_CONTROL gate_exit=${gate.status} named_faults=${found.length}`);
  } finally {
    discardCopy(dir);
  }
}

// Restoration: an unmutated copy must run both programs green with first-use back.
{
  const dir = makeCopy("restored-pristine");
  try {
    // starve=restoration hands the restoration phase a still-broken copy.
    if (STARVE === "restoration") {
      try {
        applyMutation(dir, MUTATIONS.find((mutation) => mutation.name === "reset-does-not-restore-anything"));
      } catch (error) {
        failures.push(error.message);
      }
    }
    const gate = runIn(dir, "scripts/check-task-6854.ts");
    const locate = runIn(dir, "scripts/locate-coach-tip-6855.ts");
    const found = namedFaults(locate.output);
    const expectedFirstUse = {
      TASK6854_FRESH_CONTEXT_TIPS: "3",
      TASK6854_WRONG_CONTEXT_TIPS: "0",
      TASK6854_AFTER_SINGLE_DISMISS: "2",
      TASK6854_AFTER_DISMISS_ALL: "0",
      TASK6854_AFTER_RESTART: "0",
      TASK6854_AFTER_ENCRYPTED_SYNC: "0",
      TASK6854_INDEPENDENT_PROFILE_TIPS: "3",
      TASK6854_AFTER_RESET: "3",
      TASK6854_USABLE_CONTROLS_WITHOUT_TIP: "3",
      TASK6854_STORE_OBSERVER_MEANING: "0",
      TASK6854_NETWORK_OBSERVER_MEANING: "0",
      TASK6854_LOG_OBSERVER_MEANING: "0",
    };
    if (gate.status !== 0) failures.push(`TASK6855_RESTORATION_GATE_RED exit=${gate.status}`);
    for (const [key, value] of Object.entries(expectedFirstUse)) {
      if (!gate.output.includes(`${key}=${value}`)) {
        failures.push(`TASK6855_RESTORATION_FLOW_MISSING ${key} expected=${value}`);
      }
    }
    if (locate.status !== 0 || found.length !== 0) {
      failures.push(`TASK6855_RESTORATION_FAULTS named=${found.length} exit=${locate.status}`);
    }
    console.log(`TASK6855_RESTORED gate_exit=${gate.status} locator_exit=${locate.status} named_faults=${found.length} first_use_flows=${Object.keys(expectedFirstUse).length}`);
  } finally {
    discardCopy(dir);
  }
}

const remaining = readdirSync(workRoot);
rmSync(workRoot, { recursive: true, force: true });
if (remaining.length !== 0) failures.push(`TASK6855_COPIES_REMAINING=${remaining.length}`);
if (existsSync(workRoot)) failures.push(`TASK6855_WORK_ROOT_NOT_DISCARDED dir=${workRoot}`);

console.log(`TASK6855_MUTATIONS=${MUTATIONS.length}`);
console.log(`TASK6855_FAMILIES=${[...new Set(MUTATIONS.map((mutation) => mutation.family))].join(",")}`);
console.log(`TASK6855_COPIES_CREATED=${created.length}`);
console.log(`TASK6855_COPIES_DISCARDED=${discarded.length}`);
console.log(`TASK6855_COPIES_REMAINING=${remaining.length}`);

// The proof must itself be falsifiable: starve a mutant, the observer, or the
// restoration and this same program has to go red.
if (!IS_CHILD && !STARVE) {
  for (const mode of STARVE_MODES) {
    const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url)], {
      cwd: APP_DIR,
      encoding: "utf8",
      env: { ...process.env, OSL6855_CHILD: "1", OSL6855_STARVE: mode },
    });
    const output = `${child.stdout ?? ""}${child.stderr ?? ""}`;
    const reason = /TASK6855_(MUTATION_NOT_CAUGHT|MUTATION_NOT_NAMED|RESTORATION_[A-Z_]+)[^\n]*/u.exec(output)?.[0] ?? "none";
    if (child.status !== 1 || !output.includes("TASK6855_RESULT status=red")) {
      failures.push(`TASK6855_STARVATION_NOT_FATAL mode=${mode} exit=${child.status}`);
    }
    console.log(`TASK6855_STARVATION mode=${mode} exit=${child.status} reason="${reason}"`);
  }
}

if (failures.length > 0) {
  for (const failure of failures) console.log(failure);
  console.log(`TASK6855_RESULT status=red failures=${failures.length}${STARVE ? ` starved=${STARVE}` : ""}`);
  process.exit(1);
}
console.log("TASK6855_RESULT status=green");
