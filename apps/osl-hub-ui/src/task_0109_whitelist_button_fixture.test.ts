import { describe, expect, it } from "vitest";
import {
  discordQaWhitelistButtonFixtureMarkup,
  discordQaWhitelistButtonMarkup,
  type DiscordQaWhitelistButtonState,
} from "./discord-qa-whitelist-button";

interface ButtonFacts {
  id: string;
  state: DiscordQaWhitelistButtonState;
  pressed: string;
  label: string;
  disabled: boolean;
}

function buttonFacts(html: string): ButtonFacts[] {
  return [...html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/gu)]
    .filter((match) => match[1].includes('data-whitelist-button="single-place"'))
    .map((match) => {
      const attributes = match[1];
      return {
        id: attribute(attributes, "id"),
        state: attribute(attributes, "data-whitelist-state") as DiscordQaWhitelistButtonState,
        pressed: attribute(attributes, "aria-pressed"),
        label: /<span class="discord-qa-whitelist-label">([^<]*)<\/span>/u.exec(match[2])?.[1] ?? "",
        disabled: /\sdisabled(?:\s|$)/u.test(attributes),
      };
    });
}

function attribute(attributes: string, name: string): string {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  return new RegExp(`${escaped}="([^"]*)"`, "u").exec(attributes)?.[1] ?? "";
}

describe("TASK 0109 single-place whitelist button fixture", () => {
  it("shows one off-list button and one on-list button", () => {
    const fixture = discordQaWhitelistButtonFixtureMarkup();
    const buttons = buttonFacts(fixture);
    const offList = buttons.filter((button) => button.state === "off-list");
    const onList = buttons.filter((button) => button.state === "on-list");

    console.log(`TASK0109_FIXTURE_TOTAL_WHITELIST_BUTTONS=${buttons.length}`);
    for (const state of ["off-list", "on-list"] as const) {
      const stateButtons = buttons.filter((button) => button.state === state);
      console.log(`TASK0109_FIXTURE_STATE state=${state} button_count=${stateButtons.length} aria_pressed=${stateButtons[0]?.pressed ?? "missing"} label="${stateButtons[0]?.label ?? "missing"}"`);
    }
    console.log(`TASK0109_DONE whitelist_button_states=off-list,on-list buttons_per_state=${offList.length},${onList.length}`);

    expect(fixture).toContain('data-ui-fixture="task-0109-whitelist-button"');
    expect(buttons).toHaveLength(2);
    expect(offList).toHaveLength(1);
    expect(onList).toHaveLength(1);
    expect(offList[0]).toMatchObject({ id: "task-0109-whitelist-off-list", pressed: "false", label: "Off list", disabled: false });
    expect(onList[0]).toMatchObject({ id: "task-0109-whitelist-on-list", pressed: "true", label: "On list", disabled: false });
  });

  it("keeps the production toggle single-place and fail-closed", () => {
    const offList = discordQaWhitelistButtonMarkup({
      scopeApproved: false,
      protectionActive: true,
      verifiedPeer: true,
      busy: false,
    });
    const onList = discordQaWhitelistButtonMarkup({
      scopeApproved: true,
      protectionActive: true,
      verifiedPeer: true,
      busy: false,
    });
    const disabled = discordQaWhitelistButtonMarkup({
      scopeApproved: false,
      protectionActive: false,
      verifiedPeer: true,
      busy: false,
    });

    expect(buttonFacts(offList)).toHaveLength(1);
    expect(buttonFacts(onList)).toHaveLength(1);
    expect(buttonFacts(offList)[0]).toMatchObject({ state: "off-list", pressed: "false", disabled: false });
    expect(buttonFacts(onList)[0]).toMatchObject({ state: "on-list", pressed: "true", disabled: false });
    expect(buttonFacts(disabled)[0]).toMatchObject({ state: "off-list", pressed: "false", disabled: true });
    expect(offList).toContain('data-whitelist-next="allow"');
    expect(onList).toContain('data-whitelist-next="revoke"');
  });
});
