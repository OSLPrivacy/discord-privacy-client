import {
  cancelOslPublicNameCheck,
  checkOslPublicName,
  claimOslUsername,
  isNormalizedOslUsername,
  type HubPublicNameCheck,
  type HubUsernameClaim,
} from "./adapters";

export interface PublicNameCommands {
  check(name: string): Promise<HubPublicNameCheck | null>;
  claim(name: string): Promise<HubUsernameClaim | null>;
  cancel(): Promise<boolean>;
}

export const PUBLIC_NAME_COMMANDS: PublicNameCommands = {
  check: checkOslPublicName,
  claim: claimOslUsername,
  cancel: cancelOslPublicNameCheck,
};

export type PublicNamePhase = "idle" | "checking" | "ready" | "claiming" | "claimed" | "refused";

function escape(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;");
}

/**
 * The proof is deliberately represented only by the exact name it covered.
 * Its nonce and account binding remain in the native process and never enter
 * renderer state. Changing one byte of the entry immediately drops readiness.
 */
export class PublicNamePageController {
  name = "";
  proofName: string | null = null;
  claimedName: string | null = null;
  phase: PublicNamePhase = "idle";
  message = "Enter a lowercase public name, then check it.";

  constructor(private readonly commands: PublicNameCommands = PUBLIC_NAME_COMMANDS) {}

  get canCheck(): boolean {
    return !this.busy && isNormalizedOslUsername(this.name);
  }

  get canClaim(): boolean {
    return !this.busy && this.phase === "ready" && this.proofName === this.name;
  }

  get busy(): boolean {
    return this.phase === "checking" || this.phase === "claiming";
  }

  /** Input changes invalidate the renderer state and the native proof. */
  async enterName(value: string): Promise<void> {
    if (value === this.name) return;
    this.name = value;
    this.proofName = null;
    this.claimedName = null;
    this.phase = "idle";
    this.message = isNormalizedOslUsername(value)
      ? "Check this exact name before claiming it."
      : "Use 3–30 lowercase letters, numbers, or underscores; start and end with a letter or number.";
    await this.commands.cancel();
  }

  async checkName(): Promise<boolean> {
    if (!this.canCheck) return false;
    const checkedName = this.name;
    this.proofName = null;
    this.phase = "checking";
    this.message = "Checking name and account proof…";
    const result = await this.commands.check(checkedName);
    if (this.name !== checkedName
      || result?.username !== checkedName
      || result.available !== true
      || result.proofReady !== true) {
      this.phase = "refused";
      this.message = result?.username === checkedName && result.available === false
        ? "That public name is unavailable."
        : "The matching account proof did not succeed. Claim remains unavailable.";
      return false;
    }
    this.proofName = checkedName;
    this.phase = "ready";
    this.message = `${checkedName} is ready to claim with its matching account proof.`;
    return true;
  }

  /**
   * This guard is part of the authority boundary, not just presentation.
   * Calling the method directly before an exact proof sends no command.
   */
  async claimName(): Promise<HubUsernameClaim | null> {
    if (!this.canClaim) return null;
    const provedName = this.proofName;
    if (provedName === null) return null;
    this.phase = "claiming";
    this.message = "Claiming public name…";
    const claimed = await this.commands.claim(provedName);
    this.proofName = null;
    if (!claimed || claimed.username !== provedName) {
      this.phase = "refused";
      this.message = "The claim was refused. Check the name again for a fresh proof.";
      return null;
    }
    this.claimedName = claimed.username;
    this.phase = "claimed";
    this.message = `${claimed.username} is now your public OSL name.`;
    return claimed;
  }

  async cancel(): Promise<void> {
    this.name = "";
    this.proofName = null;
    this.claimedName = null;
    this.phase = "idle";
    this.message = "Public-name check cancelled.";
    await this.commands.cancel();
  }

  render(): string {
    const claimDisabled = this.canClaim ? "" : " disabled";
    const checkDisabled = this.canCheck ? "" : " disabled";
    const value = escape(this.name);
    return `<section class="public-name-page" aria-labelledby="public-name-title" data-public-name-phase="${this.phase}" data-proof-name="${escape(this.proofName ?? "")}"><header><h3 id="public-name-title">Public OSL name</h3><p>People can find this exact name. OSL proves the connected account before it can be claimed.</p></header><label for="public-name-input">Name<input id="public-name-input" value="${value}" maxlength="30" autocomplete="off" autocapitalize="none" spellcheck="false"/></label><p class="form-status" id="public-name-status" role="status">${escape(this.message)}</p><div class="settings-actions"><button class="button" id="public-name-check" type="button"${checkDisabled}>${this.phase === "checking" ? "Checking…" : "Check name"}</button><button class="button primary" id="public-name-claim" type="button"${claimDisabled}>${this.phase === "claiming" ? "Claiming…" : "Claim"}</button><button class="button ghost" id="public-name-cancel" type="button"${this.busy ? " disabled" : ""}>Cancel</button></div></section>`;
  }
}
