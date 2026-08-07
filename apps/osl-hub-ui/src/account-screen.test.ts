/**
 * TASK 0792 - the seven Account controls, and the two things the screen is
 * measured on.
 *
 * The screenshot check next door proves the seven are drawn and legible on a
 * Linux capture, and that the capture carries none of the fixture secrets.
 * This one proves the same two claims where they are decided: that no secret
 * survives the trip from `accountSecrets` into markup, whatever the secret is,
 * and that Reset exists on exactly the four controls that can be put back and
 * refuses on the three that cannot.
 */
import { describe, expect, it } from "vitest";

import {
  ACCOUNT_CONTROL_IDS,
  ACCOUNT_CONTROL_LABELS,
  ACCOUNT_MASK,
  ACCOUNT_MASK_LENGTH,
  ACCOUNT_NO_PASSWORD_ERROR,
  ACCOUNT_NO_PRO_CODE_ERROR,
  ACCOUNT_NO_RECOVERY_ERROR,
  ACCOUNT_RESET_RULES,
  ACCOUNT_SAFE_RESET_IDS,
  DEFAULT_LOCK_MINUTES,
  type AccountControlId,
  type AccountSecrets,
  accountControls,
  accountSavePayload,
  accountScreenState,
  accountSecretFacts,
  accountStatusLine,
  changedAccountControls,
  lockLabel,
  renderAccountScreen,
  resetAccountControl,
  resetSafeAccountControls,
  saveAccount,
  setDisplayName,
  setLockMinutes,
  unsafeResetError,
} from "./account-screen";
import {
  ACCOUNT_SCREEN_NAMED_SECRETS,
  ACCOUNT_SCREEN_SECRETS,
  ACCOUNT_SCREEN_SETTINGS,
  accountSecretProbes,
} from "./account-screen-data";

/** The seven names TASK 0792 is measured against, in screen order. */
const NAMED_CONTROLS = [
  "identity",
  "password",
  "recovery",
  "lock",
  "stealth",
  "burn password",
  "Pro code",
];

const state = () => accountScreenState(ACCOUNT_SCREEN_SECRETS, ACCOUNT_SCREEN_SETTINGS);

describe("the seven Account controls", () => {
  it("draws the seven the finish line names, in that order", () => {
    expect(ACCOUNT_CONTROL_IDS.map((id) => ACCOUNT_CONTROL_LABELS[id].toLowerCase())).toEqual(
      NAMED_CONTROLS.map((name) => name.toLowerCase()),
    );
    expect(accountControls(state()).map((control) => control.label)).toEqual(
      NAMED_CONTROLS.map((name) => (name === "Pro code" ? name : name[0].toUpperCase() + name.slice(1))),
    );
  });

  it("puts every name in the markup", () => {
    const html = renderAccountScreen(state());
    for (const name of NAMED_CONTROLS) {
      expect(html.toLowerCase()).toContain(name.toLowerCase());
    }
  });

  it("makes identity and lock directly editable and the rest hand their job on", () => {
    const controls = accountControls(state());
    const byId = new Map(controls.map((control) => [control.id, control]));
    expect(byId.get("identity")?.action).toBeNull();
    expect(byId.get("lock")?.action).toBeNull();
    for (const id of ["password", "recovery", "stealth", "burn-password", "pro-code"] as const) {
      expect(byId.get(id)?.action).toBeTruthy();
    }
    const named = setDisplayName(state(), "Ada Vance");
    expect(accountControls(named)[0].value).toBe("Ada Vance");
    expect(changedAccountControls(named)).toEqual(["identity"]);
    const locked = setLockMinutes(state(), 1);
    expect(accountControls(locked)[3].value).toBe("After 1 minute");
    expect(accountStatusLine(locked)).toBe("1 control changed - not saved yet.");
    expect(accountStatusLine(state())).toBe("All 7 account controls saved.");
  });
});

describe("no secret reaches the screen", () => {
  it("refuses to build a view with no password, phrase or code", () => {
    const blanks: [Partial<AccountSecrets>, string][] = [
      [{ password: "  " }, ACCOUNT_NO_PASSWORD_ERROR],
      [{ recoveryPhrase: "" }, ACCOUNT_NO_RECOVERY_ERROR],
      [{ proCode: "" }, ACCOUNT_NO_PRO_CODE_ERROR],
    ];
    for (const [override, message] of blanks) {
      expect(() => accountSecretFacts({ ...ACCOUNT_SCREEN_SECRETS, ...override })).toThrow(message);
    }
  });

  it("keeps only set/not-set and a word count", () => {
    expect(accountSecretFacts(ACCOUNT_SCREEN_SECRETS)).toEqual({
      passwordSet: true,
      recoveryWordCount: 12,
      proCodeSet: true,
      stealthSet: true,
      burnSet: true,
    });
    expect(
      accountSecretFacts({ ...ACCOUNT_SCREEN_SECRETS, stealthPassword: "", burnPassword: "" }),
    ).toMatchObject({ stealthSet: false, burnSet: false });
  });

  it("carries no secret into the state, the markup or the save payload", () => {
    const current = state();
    const surfaces = {
      state: JSON.stringify(current),
      markup: renderAccountScreen(current),
      payload: JSON.stringify(accountSavePayload(current.draft)),
      controls: JSON.stringify(accountControls(current)),
      saved: JSON.stringify(saveAccount(current).saved),
    };
    for (const [name, text] of Object.entries(surfaces)) {
      for (const [key, secret] of Object.entries(ACCOUNT_SCREEN_SECRETS)) {
        for (const probe of accountSecretProbes(secret)) {
          expect(text.toLowerCase(), `${name} carried "${probe}" from ${key}`).not.toContain(
            probe.toLowerCase(),
          );
        }
      }
    }
    expect(ACCOUNT_SCREEN_NAMED_SECRETS).toEqual(["password", "recoveryPhrase", "proCode"]);
  });

  it("masks to the same length whatever the secret is", () => {
    const short = renderAccountScreen(
      accountScreenState({ ...ACCOUNT_SCREEN_SECRETS, password: "a" }, ACCOUNT_SCREEN_SETTINGS),
    );
    const long = renderAccountScreen(
      accountScreenState(
        { ...ACCOUNT_SCREEN_SECRETS, password: "a".repeat(64) },
        ACCOUNT_SCREEN_SETTINGS,
      ),
    );
    expect(short).toBe(long);
    expect(ACCOUNT_MASK).toHaveLength(ACCOUNT_MASK_LENGTH);
    expect([...ACCOUNT_MASK]).toEqual(Array.from({ length: ACCOUNT_MASK_LENGTH }, () => "•"));
    // Five slots stand for a secret: password, recovery, stealth, burn, Pro.
    const masked = accountControls(state()).filter((control) => control.secret);
    expect(masked.map((control) => control.id)).toEqual([
      "password",
      "recovery",
      "stealth",
      "burn-password",
      "pro-code",
    ]);
    expect(masked.every((control) => control.value === ACCOUNT_MASK)).toBe(true);
  });
});

describe("reset only where it is safe", () => {
  it("offers Reset on four controls and a reason on three", () => {
    expect(ACCOUNT_SAFE_RESET_IDS).toEqual(["identity", "lock", "stealth", "burn-password"]);
    const unsafe = ACCOUNT_CONTROL_IDS.filter((id) => !ACCOUNT_RESET_RULES[id].safe);
    expect(unsafe).toEqual(["password", "recovery", "pro-code"]);
    for (const id of unsafe) {
      const rule = ACCOUNT_RESET_RULES[id];
      expect(rule.safe).toBe(false);
      if (!rule.safe) expect(rule.reason.length).toBeGreaterThan(40);
    }
    const html = renderAccountScreen(state());
    for (const id of ACCOUNT_SAFE_RESET_IDS) {
      expect(html).toContain(`data-account-action="reset" data-account-control="${id}"`);
    }
    for (const id of unsafe) {
      expect(html).not.toContain(`data-account-action="reset" data-account-control="${id}"`);
    }
    expect(html.match(/data-account-reset="unsafe"/gu)).toHaveLength(3);
    expect(html.match(/data-account-reset="safe"/gu)).toHaveLength(4);
  });

  it("throws rather than reset a password, a recovery kit or a Pro code", () => {
    for (const id of ["password", "recovery", "pro-code"] as AccountControlId[]) {
      expect(() => resetAccountControl(state(), id)).toThrow(unsafeResetError(id));
      expect(unsafeResetError(id)).toContain(ACCOUNT_CONTROL_LABELS[id]);
    }
    expect(unsafeResetError("password")).toContain("cannot read the password");
  });

  it("puts each safe control back to its default", () => {
    const identity = resetAccountControl(state(), "identity");
    expect(identity.draft.settings.displayName).toBe(ACCOUNT_SCREEN_SETTINGS.handle);
    const lock = resetAccountControl(state(), "lock");
    expect(lock.draft.settings.lockMinutes).toBe(DEFAULT_LOCK_MINUTES);
    expect(lockLabel(lock.draft.settings.lockMinutes)).toBe("After 5 minutes");
    expect(resetAccountControl(state(), "stealth").draft.facts.stealthSet).toBe(false);
    expect(resetAccountControl(state(), "burn-password").draft.facts.burnSet).toBe(false);
  });

  it("resets the four and leaves the three alone", () => {
    const reset = resetSafeAccountControls(state());
    expect(changedAccountControls(reset)).toEqual(ACCOUNT_SAFE_RESET_IDS);
    expect(reset.draft.facts.passwordSet).toBe(true);
    expect(reset.draft.facts.proCodeSet).toBe(true);
    expect(reset.draft.facts.recoveryWordCount).toBe(12);
    expect(accountStatusLine(reset)).toBe("4 controls changed - not saved yet.");
    const saved = saveAccount(reset);
    expect(accountStatusLine(saved.state)).toBe("All 7 account controls saved.");
    expect(saved.saved.map((setting) => `${setting.id}=${setting.reading}`)).toEqual([
      `identity=${ACCOUNT_SCREEN_SETTINGS.handle}`,
      "password=set",
      "recovery=12 words",
      "lock=After 5 minutes",
      "stealth=off",
      "burn-password=off",
      "pro-code=set",
    ]);
  });
});
