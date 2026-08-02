import { describe, expect, it } from "vitest";
import { renderRecoveryStatesSettings } from "./recovery-states";

const CONTRACT_ROWS = [
  ["normal-locked-device", "Normal locked device", "Everything on that device, by entering the main password.", "Nothing."],
  ["stealth-credential", "Stealth credential used", "The normal account, by relaunching and entering the main password.", "Nothing; stealth opens a decoy and does not destroy data."],
  ["password-recovery-capable", "Forgotten password with a recovery-capable marker and password recovery phrase", "A new main password; encrypted local state is re-keyed before it is opened.", "Nothing caused by the password reset itself."],
  ["password-recovery-legacy", "Forgotten password with a pre-recovery marker that still protects encrypted files", "No password reset is permitted.", "The user may explicitly choose Fresh Start; that removes the local account rather than silently orphaning encrypted state."],
  ["no-saved-recovery-phrases", "No saved recovery phrases", "Only what the remembered main password opens on the original device.", "Future password and device recovery."],
  ["lost-device-identity-phrase", "Lost or destroyed device with only the identity phrase", "The identity and its osl_ user ID.", "Local message history, peer and verification state, whitelist, burn state, device-local activation, and expired or undelivered inbound data."],
  ["copied-profile", "Profile copied to another device", "Nothing by copying files alone.", "The copied local store remains unavailable until an authorised Device Transfer completes."],
  ["burn-credential", "Burn credential used", "The identity only if its identity phrase was saved, and then only as keys.", "OSL's local account core, local history and protected state; it does not erase recipients' already opened messages or the user's original third-party account data."],
  ["wrong-password-threshold", "Configured wrong-password threshold reached", "The same limited recovery as a burn credential.", "The same local state as burn. This must never be a hidden threshold."],
  ["fresh-start", "Fresh Start", "Nothing local.", "The same local OSL state as burn. Remote unregister/delete work is separately reported and can remain pending or fail."],
  ["outside-osl-control", "Recipient already opened a message, third-party cover text already sent, or host-OS remnants", "Not applicable.", "OSL cannot retrieve or erase these items."],
] as const;

describe("T15-T42 recovery-state table", () => {
  it("renders every lifecycle-contract row with its recoverable and lost verdict", () => {
    const markup = renderRecoveryStatesSettings();
    const renderedRows = [...markup.matchAll(/<tr data-recovery-state="([^"]+)">([\s\S]*?)<\/tr>/gu)];

    expect(renderedRows).toHaveLength(CONTRACT_ROWS.length);
    for (const [index, [id, state, recoverable, lost]] of CONTRACT_ROWS.entries()) {
      const [, renderedId, contents] = renderedRows[index]!;
      expect(renderedId).toBe(id);
      expect(contents).toContain(`<th scope="row">${state}</th>`);
      expect(contents).toContain(`<td>${recoverable}</td>`);
      expect(contents).toContain(`<td>${lost}</td>`);
    }
  });
});
