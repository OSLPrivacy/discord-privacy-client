/**
 * The USB dead-man setting is deliberately conservative.  This module only
 * describes and renders the choice; the native dead-man binding owns persisting
 * it and acting on device removal.
 */
export type DeadmanAction = "lock" | "wipe";

export interface DeadmanSelection {
  action: DeadmanAction;
  wipeConfirmed: boolean;
}

export const DEADMAN_WIPE_CONFIRMATION = "WIPE";

export const DEADMAN_LIMITS = [
  "Tails runs the whole operating system from the stick. OSL runs on Windows, which can swap, hibernate, and cache data.",
  "OSL can wipe what OSL holds, but cannot guarantee Windows has not already written a copy.",
  "This is strong against physical seizure, not a guarantee against forensic recovery.",
  "OSL cannot detect removal while this machine is suspended or hibernating.",
] as const;

/**
 * Converts an untrusted form choice into the only choices the native layer may
 * receive.  A wipe request without the exact acknowledgement fails closed to
 * the recoverable lock action.
 */
export function selectDeadmanAction(
  requested: DeadmanAction,
  typedConfirmation: string,
): DeadmanSelection {
  const wipeConfirmed = requested === "wipe" && typedConfirmation === DEADMAN_WIPE_CONFIRMATION;
  return {
    action: wipeConfirmed ? "wipe" : "lock",
    wipeConfirmed,
  };
}

export function renderDeadmanScreen(selection: DeadmanSelection): string {
  const wipeSelected = selection.action === "wipe";
  const wipeReady = wipeSelected && selection.wipeConfirmed;
  return `<section class="settings-panel deadman-settings" aria-labelledby="deadman-title">
    <h1 id="deadman-title">USB dead-man switch</h1>
    <p>Choose what happens on this device when its bound USB stick is removed.</p>
    <fieldset>
      <legend>On USB removal</legend>
      <label><input type="radio" name="deadman-action" value="lock"${wipeSelected ? "" : " checked"}/> Lock OSL <span>Recommended and recoverable.</span></label>
      <label><input type="radio" name="deadman-action" value="wipe"${wipeSelected ? " checked" : ""}/> Wipe OSL data from this device <span>Destructive and cannot be undone.</span></label>
    </fieldset>
    <label for="deadman-wipe-confirmation">Type ${DEADMAN_WIPE_CONFIRMATION} to enable wipe</label>
    <input id="deadman-wipe-confirmation" name="wipeConfirmation" type="text" autocomplete="off" autocapitalize="none" spellcheck="false"${wipeSelected && !wipeReady ? " aria-invalid=\"true\"" : ""}/>
    <p class="deadman-selection-status" role="status">${wipeSelected && !wipeReady ? "Wipe is not enabled until the confirmation matches exactly. Lock will remain active." : wipeReady ? "Wipe is enabled for this bound USB stick." : "Lock is enabled for this bound USB stick."}</p>
    <aside class="deadman-limits" aria-label="Important limits">
      <h2>Important limits</h2>
      ${DEADMAN_LIMITS.map((limit) => `<p>${limit}</p>`).join("")}
    </aside>
  </section>`;
}
