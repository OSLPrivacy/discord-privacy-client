/**
 * TASK 6855 fault locator.
 *
 * check-task-6854.ts answers "is the coach-tip contract intact?" with a single
 * exit status. This locator answers the follow-up question the 6855 finish line
 * asks: WHICH tip, in WHICH profile, stopped honouring the contract. It runs the
 * same persistence / isolation / privacy / reset / usability scenarios once per
 * catalog tip against three named profiles and prints one line per fault:
 *
 *   TASK6855_NAMED tip=<tip id> profile=<profile id> fault=<fault>
 *
 * It is deliberately generic: it knows nothing about any particular mutation, so
 * the tip and profile it names are observed, not asserted in advance.
 *
 * Visibility here is CoachTipController.isEligible, not markup(), so a mutation
 * that only corrupts the emitted markup is attributed to usability rather than
 * masquerading as a persistence fault.
 */

/** Set by the 6855 prover to prove that starving the observer makes it blind. */
const observerStarved = process.env.OSL6855_STARVE_OBSERVER === "1";

const PROFILE_A_DEVICE_1 = "profile-a-device-1";
const PROFILE_A_DEVICE_2 = "profile-a-device-2";
const PROFILE_B = "profile-b";

const networkSink: string[] = [];
let activeProfile: string = PROFILE_A_DEVICE_1;

if (!observerStarved) {
  const record = (detail: string): void => { networkSink.push(`${activeProfile} ${detail}`); };
  (globalThis as unknown as Record<string, unknown>).fetch = async (
    input: unknown,
    init?: { readonly body?: unknown },
  ) => {
    record(`fetch ${String(input)} ${typeof init?.body === "string" ? init.body : ""}`);
    return { ok: true, status: 204, text: async () => "" };
  };
  (globalThis as unknown as Record<string, unknown>).XMLHttpRequest = class {
    open(_method: string, url: string): void { record(`xhr ${url}`); }
    setRequestHeader(): void { /* observed, never forwarded */ }
    send(body?: unknown): void { record(`xhr-body ${String(body ?? "")}`); }
  };
  try {
    Object.defineProperty(globalThis, "navigator", {
      configurable: true,
      value: { sendBeacon: (url: string, body?: unknown) => { record(`beacon ${url} ${String(body ?? "")}`); return true; } },
    });
  } catch {
    // A frozen navigator only removes one of three network sinks; fetch and XHR
    // above still observe. The prover's starve-observer control proves that a
    // locator with no sink at all fails this proof rather than passing it.
  }
}

const {
  CoachTipController,
  coachTipCatalog,
  encryptedCoachTipProfileStore,
} = await import("../src/coach-tips");
type Catalog = typeof coachTipCatalog;
type Tip = Catalog[number];
type Controller = InstanceType<typeof CoachTipController>;

class MemoryStorage implements Storage {
  readonly values = new Map<string, string>();
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

const rawKeyA = new Uint8Array(32).fill(0x2a);
const rawKeyB = new Uint8Array(32).fill(0x6b);
let nonce = 1;
const randomBytes = (bytes: Uint8Array): Uint8Array => {
  bytes.fill(nonce);
  nonce = nonce === 255 ? 1 : nonce + 1;
  return bytes;
};

/** The same vocabulary check-task-6854.ts uses for its observers. */
const meaningPattern = /coach|tip|dismiss|protect-message|private-scan|switch-profile|impression|behavio(?:u)?r|usage|click/iu;

const controls: Record<string, string> = {
  "protect-message": '<button id="local-protected-toggle" type="button">Protect</button>',
  "private-scan": '<button data-scrub-route-scan type="button">Start scan</button>',
  "switch-profile": '<button data-switch-identity="profile-b" type="button">Switch</button>',
};

async function open(storage: MemoryStorage, key: Uint8Array): Promise<Controller> {
  const controller = new CoachTipController(await encryptedCoachTipProfileStore(storage, key, randomBytes));
  await controller.load();
  return controller;
}

function copyOf(source: MemoryStorage): MemoryStorage {
  const copy = new MemoryStorage();
  for (const [key, value] of source.values) copy.setItem(key, value);
  return copy;
}

function eligible(controller: Controller, tip: Tip): boolean {
  return controller.isEligible(tip.id, tip.context, [tip.controlSelector]);
}

const faults: Array<{ tip: string; profile: string; fault: string }> = [];
const recordFault = (tip: string, profile: string, fault: string): void => { faults.push({ tip, profile, fault }); };

for (const tip of coachTipCatalog) {
  const storageA1 = new MemoryStorage();
  const storageB = new MemoryStorage();

  // Both profiles exist BEFORE any dismissal, so a mutation that broadcasts a
  // dismissal to every live store has somewhere to leak to.
  activeProfile = PROFILE_A_DEVICE_1;
  const firstUseA = await open(storageA1, rawKeyA);
  activeProfile = PROFILE_B;
  const firstUseB = await open(storageB, rawKeyB);

  if (!eligible(firstUseA, tip)) recordFault(tip.id, PROFILE_A_DEVICE_1, "first-use-tip-missing");
  if (!eligible(firstUseB, tip)) recordFault(tip.id, PROFILE_B, "first-use-tip-missing");

  // Persistence: an explicit dismissal must survive a restart of the profile.
  activeProfile = PROFILE_A_DEVICE_1;
  await firstUseA.dismiss(tip.id);
  const restartedA = await open(storageA1, rawKeyA);
  if (eligible(restartedA, tip)) recordFault(tip.id, PROFILE_A_DEVICE_1, "restart-forgot-dismissal");

  // Persistence across the encrypted profile copy carried to a second device.
  activeProfile = PROFILE_A_DEVICE_2;
  const storageA2 = copyOf(storageA1);
  const syncedA = await open(storageA2, rawKeyA);
  if (eligible(syncedA, tip)) recordFault(tip.id, PROFILE_A_DEVICE_2, "encrypted-sync-lost-dismissal");

  // Isolation: the separately keyed profile must still be on its first use.
  activeProfile = PROFILE_B;
  const reopenedB = await open(storageB, rawKeyB);
  if (!eligible(reopenedB, tip)) recordFault(tip.id, PROFILE_B, "dismissal-leaked-to-other-profile");

  // Privacy, storage side: the at-rest bytes of either profile must carry no
  // coach-tip or behavioural vocabulary at all.
  for (const [profile, storage] of [
    [PROFILE_A_DEVICE_1, storageA1],
    [PROFILE_A_DEVICE_2, storageA2],
    [PROFILE_B, storageB],
  ] as Array<[string, MemoryStorage]>) {
    const observed = observerStarved ? [] : [...storage.values.entries()].flat();
    if (observed.some((value) => meaningPattern.test(value))) {
      recordFault(tip.id, profile, "plaintext-tip-meaning-in-store");
    }
  }

  // Privacy, network side: nothing may be uploaded by any tip action.
  for (const record of networkSink.filter((entry) => meaningPattern.test(entry))) {
    recordFault(tip.id, record.split(" ")[0] ?? PROFILE_A_DEVICE_1, "behavioral-analytics-uploaded");
  }
  networkSink.length = 0;

  // Usability: with the tip dismissed the underlying control must still be
  // present, enabled, and not swallowed by a leftover tip container.
  // Only meaningful once the tip really is hidden: if the dismissal was not
  // durable at all, that is a persistence fault and is already named above.
  activeProfile = PROFILE_A_DEVICE_1;
  if (!eligible(restartedA, tip)) {
    const composed = `${controls[tip.id] ?? ""}${restartedA.markup(tip.id, tip.context, [tip.controlSelector])}`;
    if (!composed.includes("<button") || composed.includes("disabled") || composed.includes("display:none")) {
      recordFault(tip.id, PROFILE_A_DEVICE_1, "control-blocked-when-tip-hidden");
    } else if (composed.includes("data-coach-tip=")) {
      recordFault(tip.id, PROFILE_A_DEVICE_1, "control-region-occupied-when-tip-hidden");
    }
  }

  // Reset: the explicit restore control must bring this tip back and keep it
  // back across the next restart.
  await restartedA.reset();
  const afterReset = await open(storageA1, rawKeyA);
  if (!eligible(afterReset, tip)) recordFault(tip.id, PROFILE_A_DEVICE_1, "reset-did-not-restore-tip");
}

for (const fault of faults) {
  console.log(`TASK6855_NAMED tip=${fault.tip} profile=${fault.profile} fault=${fault.fault}`);
}
console.log(`TASK6855_LOCATOR_TIPS=${coachTipCatalog.length}`);
console.log(`TASK6855_LOCATOR_PROFILES=3`);
console.log(`TASK6855_LOCATOR_OBSERVER=${observerStarved ? "starved" : "installed"}`);
console.log(`TASK6855_LOCATOR_FAULTS=${faults.length}`);

export {};
