// TASK 1408 - the consent page gate.
//
// The finish line is about one control: Continue must be unavailable before
// the risk tick is set, a direct call must be refused, it must become
// available once the exact risk tick is set, and nothing else on the page may
// move it. The `invoke` used here is a stub that re-implements the native rule
// from `continue_discord_scrub_after_risk_agreement` (task 1407), so a direct
// call is checked against both layers: the page's own gate, and what the hub
// would say if the page ever let a call through.
import { describe, expect, it } from "vitest";
import {
  DISCORD_TERMS_ADDRESS,
  SCRUB_CONSENT_SENTENCES,
  continueFromScrubConsentPage,
  pressScrubConsentTermsButton,
  riskTickLabel,
  scrubConsentAccountsMissingRiskTick,
  scrubConsentContinueAvailable,
  scrubConsentPageMarkup,
  scrubConsentPageState,
  scrubConsentRefusalMessage,
  setScrubConsentRiskTick,
  type ScrubConsentPageState,
} from "./scrub-consent-page";

const ACCOUNTS = [
  { accountId: "discord-account-alpha-1408", accountLabel: "Ada on Discord" },
  { accountId: "discord-account-beta-1408", accountLabel: "Ada's spare Discord" },
];

/** Counts calls and enforces the hub's own rule from task 1407. */
function nativeStub() {
  const calls: string[] = [];
  const agreed = new Set<string>();
  const selected = ACCOUNTS.map((account) => account.accountId);
  const invoke = async (command: string, payload?: Record<string, unknown>) => {
    calls.push(command);
    if (command === "save_discord_scrub_consent_facts") {
      const input = (payload as { input: { accountId: string } }).input;
      agreed.add(input.accountId);
      return { accountId: input.accountId };
    }
    if (command === "continue_discord_scrub_after_risk_agreement") {
      const missing = selected.filter((accountId) => !agreed.has(accountId));
      if (missing.length > 0) {
        throw new Error(
          `Risk agreement is required for every selected Discord account before continuing: missing=${missing.join(",")}`,
        );
      }
      return { accountIds: selected, agreedAccountIds: [...agreed], mayContinue: true };
    }
    throw new Error(`unexpected command ${command}`);
  };
  return { calls, invoke };
}

const continueIsDisabled = (state: ScrubConsentPageState) =>
  scrubConsentPageMarkup(state).includes('id="scrub-consent-continue" type="button" disabled');

describe("TASK 1408 Scrub consent page", () => {
  it("shows separate accounts, terms buttons, a risk tick, Back and Continue", () => {
    const state = scrubConsentPageState(ACCOUNTS);
    const markup = scrubConsentPageMarkup(state);
    for (const account of ACCOUNTS) {
      expect(markup).toContain(`data-account-id="${account.accountId}"`);
      expect(markup).toContain(`data-terms-button="${account.accountId}"`);
      expect(markup).toContain(`data-risk-tick="${account.accountId}"`);
      expect(markup).toContain(riskTickLabel(account.accountLabel));
    }
    expect(markup).toContain(`data-terms-address="${DISCORD_TERMS_ADDRESS}"`);
    expect(markup).toContain('id="scrub-consent-back"');
    expect(markup).toContain('id="scrub-consent-continue"');
    console.log(
      `TASK1408_PAGE accounts=${state.accounts.length} terms_buttons=${(markup.match(/data-terms-button=/gu) ?? []).length}`
      + ` risk_ticks=${(markup.match(/data-risk-tick=/gu) ?? []).length} back=1 continue=1 terms_address=${DISCORD_TERMS_ADDRESS}`,
    );
  });

  it("keeps Continue unavailable before the risk tick and refuses a direct call without invoking", async () => {
    const state = scrubConsentPageState(ACCOUNTS);
    const native = nativeStub();

    expect(scrubConsentContinueAvailable(state)).toBe(false);
    expect(continueIsDisabled(state)).toBe(true);

    const result = await continueFromScrubConsentPage(state, native.invoke);
    expect(result.ok).toBe(false);
    expect(result.ok === false && result.refusal).toBe(
      "Risk agreement is required for every selected Discord account before continuing:"
      + " missing=discord-account-alpha-1408,discord-account-beta-1408",
    );
    expect(native.calls).toEqual([]);
    console.log(
      `TASK1408_BEFORE_TICK continue_available=${scrubConsentContinueAvailable(state)} continue_disabled_attribute=${continueIsDisabled(state)}`
      + ` direct_invoke_result=REFUSED refusal="${result.ok === false ? result.refusal : ""}" invoke_calls=${native.calls.length}`,
    );
  });

  it("makes Continue available once the exact risk tick is set on every chosen account", async () => {
    let state = scrubConsentPageState(ACCOUNTS);
    state = setScrubConsentRiskTick(state, ACCOUNTS[0].accountId, true);
    expect(scrubConsentContinueAvailable(state)).toBe(false);
    expect(scrubConsentAccountsMissingRiskTick(state)).toEqual([ACCOUNTS[1].accountId]);
    const partial = scrubConsentContinueAvailable(state);

    state = setScrubConsentRiskTick(state, ACCOUNTS[1].accountId, true);
    expect(scrubConsentContinueAvailable(state)).toBe(true);
    expect(continueIsDisabled(state)).toBe(false);

    const native = nativeStub();
    const result = await continueFromScrubConsentPage(state, native.invoke);
    expect(result).toEqual({ ok: true, agreedAccountIds: ACCOUNTS.map((account) => account.accountId) });
    expect(native.calls).toEqual([
      "save_discord_scrub_consent_facts",
      "save_discord_scrub_consent_facts",
      "continue_discord_scrub_after_risk_agreement",
    ]);
    console.log(
      `TASK1408_AFTER_TICK one_tick_continue_available=${partial} both_ticks_continue_available=${scrubConsentContinueAvailable(state)}`
      + ` continue_disabled_attribute=${continueIsDisabled(state)} direct_invoke_result=OK invoke_calls=${native.calls.length}`
      + ` agreed_ids=${result.ok === true ? result.agreedAccountIds.join(",") : ""}`,
    );
  });

  it("changes Continue for the risk tick only", () => {
    const base = scrubConsentPageState(ACCOUNTS);
    const ticked = ACCOUNTS.reduce(
      (state, account) => setScrubConsentRiskTick(state, account.accountId, true),
      base,
    );

    const others: Array<[string, ScrubConsentPageState]> = [
      ["press_terms_alpha", pressScrubConsentTermsButton(base, ACCOUNTS[0].accountId)],
      ["press_terms_beta", pressScrubConsentTermsButton(base, ACCOUNTS[1].accountId)],
      ["press_terms_both", pressScrubConsentTermsButton(pressScrubConsentTermsButton(base, ACCOUNTS[0].accountId), ACCOUNTS[1].accountId)],
      ["show_refusal_notice", { ...base, notice: scrubConsentRefusalMessage(base) }],
      ["rename_account_label", { ...base, accounts: base.accounts.map((account) => ({ ...account, accountLabel: "Renamed" })) }],
      ["tick_unknown_account", setScrubConsentRiskTick(base, "discord-account-not-chosen-1408", true)],
      ["risk_tick_off_again", setScrubConsentRiskTick(ticked, ACCOUNTS[0].accountId, false)],
    ];

    const report: string[] = [];
    for (const [name, state] of others) {
      expect(scrubConsentContinueAvailable(state)).toBe(false);
      expect(continueIsDisabled(state)).toBe(true);
      report.push(`${name}=${scrubConsentContinueAvailable(state)}`);
    }
    // The same page, with only the risk ticks flipped on, does open Continue.
    expect(scrubConsentContinueAvailable(ticked)).toBe(true);
    report.push(`risk_tick_on_both=${scrubConsentContinueAvailable(ticked)}`);
    // ...and with the terms pressed as well, it is still the ticks that decide.
    const tickedAndTermsPressed = ACCOUNTS.reduce(
      (state, account) => pressScrubConsentTermsButton(state, account.accountId),
      ticked,
    );
    expect(scrubConsentContinueAvailable(tickedAndTermsPressed)).toBe(true);
    report.push(`risk_tick_on_both_plus_terms=${scrubConsentContinueAvailable(tickedAndTermsPressed)}`);
    console.log(`TASK1408_ONLY_RISK_TICK ${report.join(" ")}`);
  });

  it("stores exactly the sentences it shows, within the hub's consent-fact limits", () => {
    const state = scrubConsentPageState(ACCOUNTS);
    const markup = scrubConsentPageMarkup(state);
    const sizes: string[] = [];
    for (const [key, sentence] of Object.entries(SCRUB_CONSENT_SENTENCES)) {
      // `validate_consent_fact` in apps/osl-hub/src/preferences.rs: non-empty,
      // <= 256 bytes, ASCII graphic or space only.
      expect(sentence.length).toBeLessThanOrEqual(256);
      expect(/^[\x20-\x7e]+$/u.test(sentence)).toBe(true);
      expect(markup).toContain(sentence);
      sizes.push(`${key}=${sentence.length}`);
    }
    console.log(`TASK1408_FACTS ${sizes.join(" ")}`);
  });

  it("does not open the terms address by itself", () => {
    const state = pressScrubConsentTermsButton(scrubConsentPageState(ACCOUNTS), ACCOUNTS[0].accountId);
    const markup = scrubConsentPageMarkup(state);
    expect(markup).toContain(DISCORD_TERMS_ADDRESS);
    expect(markup).not.toContain("<a ");
    expect(markup).not.toContain("href=");
    expect(markup).not.toContain("window.open");
    console.log(`TASK1408_TERMS_NO_AUTO_OPEN anchors=0 hrefs=0 address_shown_as_text=true address=${DISCORD_TERMS_ADDRESS}`);
  });
});
