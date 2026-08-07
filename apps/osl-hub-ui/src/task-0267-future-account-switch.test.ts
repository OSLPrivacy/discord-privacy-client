import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { futureAccountSwitchMarkup, type FutureAccountSwitchModel } from "./future-account-switch";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

type RenderedSwitch = {
  personId: string;
  label: string;
  state: string;
  checked: boolean;
};

/** A friend page fixture: the manage block the People list draws per friend. */
function friendPageFixture(person: FutureAccountSwitchModel): string {
  return `<article class="person-row person-profile"><header><div><strong>Ada Lovelace</strong><small>Verified</small></div></header><details class="friend-management"><summary>Manage</summary><div><div class="friend-approvals"><span>Approved chats</span><div><span class="friend-none">No chats approved</span></div></div>${
    futureAccountSwitchMarkup(person)
  }</div></details></article>`;
}

function renderedSwitches(markup: string): RenderedSwitch[] {
  const found: RenderedSwitch[] = [];
  const pattern = /<label class="setting-line interactive future-account-switch"([^>]*)>([\s\S]*?)<\/label>/gu;
  for (const match of markup.matchAll(pattern)) {
    const attrs = match[1] ?? "";
    const body = match[2] ?? "";
    found.push({
      personId: /data-future-account-switch="([^"]*)"/u.exec(attrs)?.[1] ?? "",
      label: /<strong>([^<]+)<\/strong>/u.exec(body)?.[1] ?? "",
      state: /data-future-account-switch-state="([^"]*)"/u.exec(attrs)?.[1] ?? "",
      checked: /<input\b[^>]*\bchecked\b/u.test(body),
    });
  }
  return found;
}

describe("TASK0267 future-account switch", () => {
  it("draws the switch on the friend page", () => {
    expect(mainSource).toContain('import { futureAccountSwitchMarkup } from "./future-account-switch";');
    expect(mainSource).toContain("const futureAccountSwitch = mode === \"manage\"");
    expect(mainSource).toContain("futureAccountSwitchMarkup({");
    expect(mainSource).toContain("${truncated}</div>${futureAccountSwitch}<details class=\"friend-security\"");
  });

  it("renders the switch in both on and off states", () => {
    const on = renderedSwitches(friendPageFixture({ personId: "hub-person-ada", enabled: true }));
    const off = renderedSwitches(friendPageFixture({ personId: "hub-person-ada", enabled: false }));

    console.log(`TASK0267 on switches=${on.length} label=${on[0]?.label} state=${on[0]?.state} checked=${on[0]?.checked}`);
    console.log(`TASK0267 off switches=${off.length} label=${off[0]?.label} state=${off[0]?.state} checked=${off[0]?.checked}`);
    console.log(`TASK0267 states=${[on[0]?.state, off[0]?.state].join(", ")}`);

    expect(on).toHaveLength(1);
    expect(off).toHaveLength(1);
    expect(on[0]?.label).toBe("Auto-whitelist new accounts");
    expect(off[0]?.label).toBe("Auto-whitelist new accounts");
    expect(on[0]?.personId).toBe("hub-person-ada");
    expect(off[0]?.personId).toBe("hub-person-ada");
    expect(on[0]?.state).toBe("on");
    expect(off[0]?.state).toBe("off");
    expect(on[0]?.checked).toBe(true);
    expect(off[0]?.checked).toBe(false);
  });

  it("explains what each state does", () => {
    const on = friendPageFixture({ personId: "hub-person-ada", enabled: true });
    const off = friendPageFixture({ personId: "hub-person-ada", enabled: false });
    expect(on).toContain("approved for the chats you already share");
    expect(off).toContain("stays unapproved until you approve it");
  });
});
