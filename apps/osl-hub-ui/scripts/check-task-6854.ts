import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  CoachTipController,
  coachTipCatalog,
  encryptedCoachTipProfileStore,
  type CoachTipContext,
  type EncryptedProfileStateStore,
} from "../src/coach-tips";

class MemoryStorage implements Storage {
  readonly values = new Map<string, string>();
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

const breakMode = process.argv.find((argument) => argument.startsWith("--break="))?.slice(8) ?? "";
const expectedBreakModes = new Set([
  "starve-context",
  "dismiss",
  "dismiss-all",
  "reset",
  "restart",
  "sync",
  "independent-profile",
  "observer",
  "session-only",
]);
if (breakMode && !expectedBreakModes.has(breakMode)) throw new Error(`unknown break mode: ${breakMode}`);

const rawKeyA = new Uint8Array(32).fill(0x2a);
const rawKeyB = new Uint8Array(32).fill(0x6b);
let nonce = 1;
const randomBytes = (bytes: Uint8Array): Uint8Array => {
  bytes.fill(nonce);
  nonce = nonce === 255 ? 1 : nonce + 1;
  return bytes;
};

function encryptedCopy(source: MemoryStorage): MemoryStorage {
  const copy = new MemoryStorage();
  for (const [key, value] of source.values) copy.setItem(key, value);
  return copy;
}

function visibleTips(controller: CoachTipController, starve = false): number {
  return coachTipCatalog.filter((tip) => controller.markup(
    tip.id,
    tip.context,
    starve ? [] : [tip.controlSelector],
  ) !== "").length;
}

function wrongContextTips(controller: CoachTipController): number {
  const wrongContext: Record<CoachTipContext, CoachTipContext> = {
    "protected-workspace": "privacy-scan",
    "privacy-scan": "profile-picker",
    "profile-picker": "protected-workspace",
  };
  return coachTipCatalog.filter((tip) => controller.markup(
    tip.id,
    wrongContext[tip.context],
    [tip.controlSelector],
  ) !== "").length;
}

async function controllerFor(storage: MemoryStorage, key: Uint8Array): Promise<CoachTipController> {
  const controller = new CoachTipController(await encryptedCoachTipProfileStore(storage, key, randomBytes));
  await controller.load();
  return controller;
}

const profileADevice1Storage = new MemoryStorage();
const profileA = await controllerFor(profileADevice1Storage, rawKeyA);
const freshTips = visibleTips(profileA, breakMode === "starve-context");
const wrongContextCount = wrongContextTips(profileA);
assert.equal(freshTips, coachTipCatalog.length, "a fresh profile must see every eligible tip in its exact context");
assert.equal(wrongContextCount, 0, "tips must not appear outside their exact context");

if (breakMode !== "dismiss") await profileA.dismiss("protect-message");
const afterOneDismiss = visibleTips(profileA);
assert.equal(afterOneDismiss, coachTipCatalog.length - 1, "an explicit single dismissal must hide exactly that tip");

let sessionOnlyStore: EncryptedProfileStateStore | null = null;
let currentSession = profileA;
if (breakMode === "session-only") {
  let raw: string | null = null;
  sessionOnlyStore = {
    getItem: async () => raw,
    setItem: async (_key, value) => { raw = value; },
  };
}
if (sessionOnlyStore) {
  const ephemeral = new CoachTipController(sessionOnlyStore);
  await ephemeral.load();
  await ephemeral.dismissAll();
  currentSession = ephemeral;
} else if (breakMode !== "dismiss-all") {
  await profileA.dismissAll();
}
const afterDismissAll = visibleTips(currentSession);
assert.equal(afterDismissAll, 0, "dismiss all must hide every tip in the current profile");

const restartStorage = breakMode === "restart" ? new MemoryStorage() : profileADevice1Storage;
const restartedA = await controllerFor(restartStorage, rawKeyA);
const restartTips = visibleTips(restartedA);
assert.equal(restartTips, 0, "dismissals must survive restart");

const profileADevice2Storage = breakMode === "sync"
  ? new MemoryStorage()
  : encryptedCopy(profileADevice1Storage);
const syncedA = await controllerFor(profileADevice2Storage, rawKeyA);
const syncTips = visibleTips(syncedA);
assert.equal(syncTips, 0, "the encrypted profile sync copy must restore dismissals");

const profileBStorage = breakMode === "independent-profile"
  ? profileADevice1Storage
  : new MemoryStorage();
const profileB = await controllerFor(profileBStorage, breakMode === "independent-profile" ? rawKeyA : rawKeyB);
const independentProfileTips = visibleTips(profileB);
assert.equal(independentProfileTips, coachTipCatalog.length, "a second profile must remain independent");

if (breakMode !== "reset") await syncedA.reset();
const resetTips = visibleTips(syncedA);
assert.equal(resetTips, coachTipCatalog.length, "reset must restore every eligible tip");

const controls = [
  '<button id="local-protected-toggle" type="button">Protect</button>',
  '<button data-scrub-route-scan type="button">Start scan</button>',
  '<button data-switch-identity="profile-b" type="button">Switch</button>',
];
const absentTipMarkup = coachTipCatalog.map((tip, index) => `${controls[index]}${restartedA.markup(
  tip.id,
  tip.context,
  [tip.controlSelector],
)}`);
const usableControls = absentTipMarkup.filter((markup) => markup.includes("<button") && !markup.includes("disabled") && !markup.includes("data-coach-tip=")).length;
assert.equal(usableControls, coachTipCatalog.length, "every underlying control must remain usable when its tip is absent");

const networkObserver: string[] = [];
const logObserver: string[] = [];
if (breakMode === "observer") networkObserver.push("POST /tip-impression");
const observedStore = [...profileADevice1Storage.values.entries()].flat();
const meaningPattern = /coach|tip|dismiss|protect-message|private-scan|switch-profile|impression|behavio(?:u)?r|usage|click/iu;
const storeMeaning = observedStore.filter((value) => meaningPattern.test(value)).length;
const networkMeaning = networkObserver.filter((value) => meaningPattern.test(value)).length;
const logMeaning = logObserver.filter((value) => meaningPattern.test(value)).length;
assert.equal(storeMeaning, 0, "local and sync storage observers must recover zero tip meaning");
assert.equal(networkMeaning, 0, "network observers must recover zero tip or behavior meaning");
assert.equal(logMeaning, 0, "log observers must recover zero tip or behavior meaning");

const readRelative = (relative: string): string => readFileSync(
  fileURLToPath(new URL(relative, import.meta.url)),
  "utf8",
);
const nativeProfileState = readRelative("../../osl-hub/src/coach_tips.rs");
const rendererProfileState = readRelative("../src/coach-tips.ts");
const identityRegistry = readRelative("../../osl-hub/src/identity_registry.rs");
const rotationRegistry = readRelative("../../../crates/ipc/src/main_password.rs");
const transferRegistry = readRelative("../../../crates/ipc/src/commands.rs");
const nativeMain = readRelative("../../osl-hub/src/main.rs");
assert.match(nativeProfileState, /COACH_TIP_STATE_FILE:\s*&str\s*=\s*"profile_ui_state_v1\.json"/u);
assert.match(nativeProfileState, /COACH_TIP_PLAINTEXT_BYTES:\s*usize\s*=\s*512/u);
assert.match(nativeProfileState, /encrypt_at_rest\(&plaintext, key\)/u);
assert.match(identityRegistry, /crate::coach_tips::COACH_TIP_STATE_FILE/u);
assert.match(rotationRegistry, /"profile_ui_state_v1\.json"/u);
assert.match(transferRegistry, /"profile_ui_state_v1\.json"/u);
assert.match(transferRegistry, /crypto::aead::seal\(&key, &nonce, OSL_EXPORT_MAGIC, &plaintext\)/u);
assert.match(transferRegistry, /bytes = crate::main_password::maybe_encrypt\(&bytes\)/u);
assert.match(nativeMain, /async fn get_coach_tip_state/u);
assert.match(nativeMain, /async fn save_coach_tip_state/u);
assert.doesNotMatch(nativeProfileState, /tracing::|println!|eprintln!/u);
assert.doesNotMatch(rendererProfileState, /\bfetch\s*\(|new\s+XMLHttpRequest|navigator\.sendBeacon|console\./iu);
assert.doesNotMatch("profile_ui_state_v1.json", meaningPattern);
const encryptedProfileRegistrations = 3;

console.log(`TASK6854_FRESH_CONTEXT_TIPS=${freshTips}`);
console.log(`TASK6854_WRONG_CONTEXT_TIPS=${wrongContextCount}`);
console.log(`TASK6854_AFTER_SINGLE_DISMISS=${afterOneDismiss}`);
console.log(`TASK6854_AFTER_DISMISS_ALL=${afterDismissAll}`);
console.log(`TASK6854_AFTER_RESTART=${restartTips}`);
console.log(`TASK6854_AFTER_ENCRYPTED_SYNC=${syncTips}`);
console.log(`TASK6854_INDEPENDENT_PROFILE_TIPS=${independentProfileTips}`);
console.log(`TASK6854_AFTER_RESET=${resetTips}`);
console.log(`TASK6854_USABLE_CONTROLS_WITHOUT_TIP=${usableControls}`);
console.log(`TASK6854_STORE_OBSERVER_MEANING=${storeMeaning}`);
console.log(`TASK6854_NETWORK_OBSERVER_MEANING=${networkMeaning}`);
console.log(`TASK6854_LOG_OBSERVER_MEANING=${logMeaning}`);
console.log(`TASK6854_ENCRYPTED_PROFILE_REGISTRATIONS=${encryptedProfileRegistrations}`);
