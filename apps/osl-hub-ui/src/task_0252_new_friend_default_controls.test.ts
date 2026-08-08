import { describe, expect, it } from "vitest";
import {
  newFriendDefaultControlsFixtureMarkup,
  newFriendDefaultControlsMarkup,
} from "./new-friend-default-controls";

interface ControlFacts {
  control: string;
  legend: string;
  choices: { value: string; checked: boolean; disabled: boolean }[];
}

function controlFacts(html: string): ControlFacts[] {
  return [...html.matchAll(/<fieldset\b([^>]*)>([\s\S]*?)<\/fieldset>/gu)]
    .filter((match) => match[1].includes("data-new-friend-control="))
    .map((match) => ({
      control: /data-new-friend-control="([^"]*)"/u.exec(match[1])?.[1] ?? "",
      legend: /<legend>([^<]*)<\/legend>/u.exec(match[2])?.[1] ?? "",
      choices: [...match[2].matchAll(/<input\b([^>]*)\/>/gu)].map((input) => ({
        value: /data-new-friend-choice="([^"]*)"/u.exec(input[1])?.[1] ?? "",
        checked: /\schecked\s/u.test(input[1]),
        disabled: /\sdisabled\s/u.test(input[1]),
      })),
    }));
}

function saveFacts(html: string): { label: string; disabled: boolean }[] {
  return [...html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/gu)]
    .filter((match) => match[1].includes('data-new-friend-action="save-defaults"'))
    .map((match) => ({
      label: match[2],
      disabled: /\sdisabled\s/u.test(match[1]),
    }));
}

describe("TASK 0252 new-friend default controls fixture", () => {
  it("renders all three controls and a Save default action", () => {
    const fixture = newFriendDefaultControlsFixtureMarkup();
    const controls = controlFacts(fixture);
    const saves = saveFacts(fixture);

    for (const control of controls) {
      console.log(`TASK0252_CONTROL control=${control.control} legend="${control.legend}" choices=${control.choices.map((choice) => choice.value).join(",")} checked=${control.choices.find((choice) => choice.checked)?.value ?? "none"}`);
    }
    console.log(`TASK0252_SAVE buttons=${saves.length} label="${saves[0]?.label ?? "missing"}" disabled=${saves[0]?.disabled ?? "missing"}`);
    console.log(`TASK0252_DONE fixture_controls=${controls.map((control) => control.control).join(",")} control_count=${controls.length} save_action=${saves.length}`);

    expect(fixture).toContain('data-ui-fixture="task-0252-new-friend-default-controls"');
    expect(controls).toHaveLength(3);
    expect(controls.map((control) => control.control)).toEqual([
      "account-reach",
      "auto-whitelist",
      "verification-warnings",
    ]);

    const [reach, autoWhitelist, warnings] = controls;
    expect(reach.choices.map((choice) => choice.value)).toEqual(["approved_chats_only", "all_shared_chats"]);
    expect(autoWhitelist.choices.map((choice) => choice.value)).toEqual(["never", "ask_me", "always", "only_if_a_friend"]);
    expect(warnings.choices.map((choice) => choice.value)).toEqual(["always", "never"]);

    for (const control of controls) {
      expect(control.choices.filter((choice) => choice.checked)).toHaveLength(1);
      expect(control.choices.every((choice) => !choice.disabled)).toBe(true);
    }
    expect(reach.choices.find((choice) => choice.checked)?.value).toBe("approved_chats_only");
    expect(autoWhitelist.choices.find((choice) => choice.checked)?.value).toBe("never");
    expect(warnings.choices.find((choice) => choice.checked)?.value).toBe("always");

    expect(saves).toHaveLength(1);
    expect(saves[0]).toMatchObject({ label: "Save default", disabled: false });
  });

  it("reflects the model selection and busy state in the production markup", () => {
    const markup = newFriendDefaultControlsMarkup({
      accountReach: "all_shared_chats",
      autoWhitelist: "only_if_a_friend",
      verificationWarnings: "never",
      busy: false,
    });
    const controls = controlFacts(markup);
    expect(controls.find((control) => control.control === "account-reach")?.choices.find((choice) => choice.checked)?.value).toBe("all_shared_chats");
    expect(controls.find((control) => control.control === "auto-whitelist")?.choices.find((choice) => choice.checked)?.value).toBe("only_if_a_friend");
    expect(controls.find((control) => control.control === "verification-warnings")?.choices.find((choice) => choice.checked)?.value).toBe("never");
    expect(saveFacts(markup)[0]).toMatchObject({ disabled: false });

    const busy = newFriendDefaultControlsMarkup({
      accountReach: "approved_chats_only",
      autoWhitelist: "never",
      verificationWarnings: "always",
      busy: true,
    });
    expect(controlFacts(busy).every((control) => control.choices.every((choice) => choice.disabled))).toBe(true);
    expect(saveFacts(busy)[0]).toMatchObject({ disabled: true });
  });
});
