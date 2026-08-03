/**
 * T15-A8 follow-up — the in-memory mirror of "a recovery kit exists that the
 * owner never confirmed saving".
 *
 * The durable authority is encrypted account state in the native hub
 * (`recovery_kit_status.json`). This layer holds the mirror the router reads,
 * and the whole point of the module is *when* that mirror moves.
 *
 * The defect this exists to prevent: the mirror used to be assigned only after
 * the native write came back, while the screen that changed it routed onward on
 * the same turn. Ticking "I saved my recovery kit" and pressing Continue
 * therefore re-derived the route from a mirror that still said "unsaved", and
 * the owner was thrown back to the gate reading "You never confirmed saving
 * your recovery kit" — on the one screen whose entire job is to be believed.
 * The write itself was fine; it had simply not landed yet.
 *
 * So: the mirror (and the `onboarding-resume` compatibility cache that
 * `resumeOnboardingRoute` also consults) move *synchronously*, before the round
 * trip, because they describe the decision the owner just made rather than the
 * state of a disk. The native write still decides what survives a restart, and
 * a failed write is reported to the caller so it can say so.
 */

import {
  clearRecoveryKitUnsaved,
  markRecoveryKitUnsaved,
  type OnboardingResumeStorage,
} from "./onboarding-resume";

export interface RecoveryKitUnsavedPorts {
  /** The compatibility cache `resumeOnboardingRoute` reads. */
  storage: OnboardingResumeStorage;
  /** The native authority. `null` means the answer could not be read. */
  read(): Promise<boolean | null>;
  /** The native write. `false` means nothing durable was recorded. */
  write(unsaved: boolean): Promise<boolean>;
}

export interface RecoveryKitUnsavedFlag {
  /** The mirror. Always the latest decision, never a pending one. */
  unsaved(): boolean;
  /** Adopt the native answer at launch. */
  load(): Promise<boolean>;
  /** Record a decision. Resolves with whether it reached durable storage. */
  set(unsaved: boolean): Promise<boolean>;
}

export function createRecoveryKitUnsavedFlag(ports: RecoveryKitUnsavedPorts): RecoveryKitUnsavedFlag {
  let unsaved = false;
  // Native writes are serialised so a slow earlier write can never land after a
  // newer one. Two writes are in flight in the ordinary case -- the checkbox
  // and then Continue -- and if they reordered on disk the owner would be sent
  // back to this gate on the next launch, having saved their kit.
  let writes: Promise<unknown> = Promise.resolve();

  const mirror = (value: boolean): void => {
    unsaved = value;
    if (value) markRecoveryKitUnsaved(ports.storage);
    else clearRecoveryKitUnsaved(ports.storage);
  };

  return {
    unsaved: () => unsaved,
    load: async () => {
      const stored = await ports.read().catch(() => null);
      if (stored === null) {
        // The native answer is unavailable. Do not touch the cache: a stale
        // "unsaved" mark there is the only surviving hint that a kit was never
        // saved, and dropping it would silently skip the step.
        unsaved = false;
        return false;
      }
      mirror(stored);
      return stored;
    },
    set: async (value: boolean) => {
      mirror(value);
      const persisted = writes.then(() => ports.write(value).catch(() => false));
      writes = persisted.catch(() => undefined);
      return persisted;
    },
  };
}
