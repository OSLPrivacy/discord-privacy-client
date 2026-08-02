/**
 * The lifecycle contract is useful only when people can consult it before
 * choosing destructive account actions. This markup is designed to live in
 * Settings and uses the shared Settings disclosure styling.
 */
export interface RecoveryStateRow {
  id: string;
  state: string;
  recoverable: string;
  lost: string;
}

/** Mirrors the frozen lifecycle contract, §2. */
export const RECOVERY_STATE_ROWS: readonly RecoveryStateRow[] = [
  {
    id: "normal-locked-device",
    state: "Normal locked device",
    recoverable: "Everything on that device, by entering the main password.",
    lost: "Nothing.",
  },
  {
    id: "stealth-credential",
    state: "Stealth credential used",
    recoverable: "The normal account, by relaunching and entering the main password.",
    lost: "Nothing; stealth opens a decoy and does not destroy data.",
  },
  {
    id: "password-recovery-capable",
    state: "Forgotten password with a recovery-capable marker and password recovery phrase",
    recoverable: "A new main password; encrypted local state is re-keyed before it is opened.",
    lost: "Nothing caused by the password reset itself.",
  },
  {
    id: "password-recovery-legacy",
    state: "Forgotten password with a pre-recovery marker that still protects encrypted files",
    recoverable: "No password reset is permitted.",
    lost: "The user may explicitly choose Fresh Start; that removes the local account rather than silently orphaning encrypted state.",
  },
  {
    id: "no-saved-recovery-phrases",
    state: "No saved recovery phrases",
    recoverable: "Only what the remembered main password opens on the original device.",
    lost: "Future password and device recovery.",
  },
  {
    id: "lost-device-identity-phrase",
    state: "Lost or destroyed device with only the identity phrase",
    recoverable: "The identity and its osl_ user ID.",
    lost: "Local message history, peer and verification state, whitelist, burn state, device-local activation, and expired or undelivered inbound data.",
  },
  {
    id: "copied-profile",
    state: "Profile copied to another device",
    recoverable: "Nothing by copying files alone.",
    lost: "The copied local store remains unavailable until an authorised Device Transfer completes.",
  },
  {
    id: "burn-credential",
    state: "Burn credential used",
    recoverable: "The identity only if its identity phrase was saved, and then only as keys.",
    lost: "OSL's local account core, local history and protected state; it does not erase recipients' already opened messages or the user's original third-party account data.",
  },
  {
    id: "wrong-password-threshold",
    state: "Configured wrong-password threshold reached",
    recoverable: "The same limited recovery as a burn credential.",
    lost: "The same local state as burn. This must never be a hidden threshold.",
  },
  {
    id: "fresh-start",
    state: "Fresh Start",
    recoverable: "Nothing local.",
    lost: "The same local OSL state as burn. Remote unregister/delete work is separately reported and can remain pending or fail.",
  },
  {
    id: "outside-osl-control",
    state: "Recipient already opened a message, third-party cover text already sent, or host-OS remnants",
    recoverable: "Not applicable.",
    lost: "OSL cannot retrieve or erase these items.",
  },
];

export function renderRecoveryStatesSettings(): string {
  const rows = RECOVERY_STATE_ROWS.map((row) => `<tr data-recovery-state="${row.id}"><th scope="row">${row.state}</th><td>${row.recoverable}</td><td>${row.lost}</td></tr>`).join("");
  return `<details class="settings-disclosure recovery-states" open><summary><span><strong>Recovery and data loss</strong><small>What OSL can recover in each situation</small></span></summary><div><p>Check this before choosing a burn credential, Fresh Start, or a device transfer.</p><table><caption>What you can still recover and what is lost</caption><thead><tr><th scope="col">Situation</th><th scope="col">You can recover</th><th scope="col">Lost or unavailable</th></tr></thead><tbody>${rows}</tbody></table></div></details>`;
}
