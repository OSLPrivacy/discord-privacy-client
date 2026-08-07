import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { futureAccountSwitchMarkup } from "./future-account-switch";
import {
  GET_FUTURE_ACCOUNT_COMMAND,
  SET_FUTURE_ACCOUNT_COMMAND,
  loadFutureAccountSwitchStates,
  saveFutureAccountSwitch,
  type FutureAccountCommandDependencies,
} from "./future-account-switch-connect";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

/** The 4 friends whose switches the fixture draws. */
const FRIENDS = ["hub-person-ada", "hub-person-bel", "hub-person-cai", "hub-person-dot"] as const;

type RenderedSwitch = {
  personId: string;
  state: string;
  checked: boolean;
};

type FakeHub = {
  saved: Map<string, boolean>;
  commands: string[];
  dependencies: FutureAccountCommandDependencies;
};

/** A Hub double that stores per-friend state behind the real command names. */
function fakeHub(initial: Readonly<Record<string, boolean>>): FakeHub {
  const saved = new Map(Object.entries(initial));
  const commands: string[] = [];
  const dependencies: FutureAccountCommandDependencies = {
    isTauriRuntime: () => true,
    invoke: (command, args) => {
      commands.push(command);
      const enabled = saved.get(args.personId);
      if (enabled === undefined) return Promise.reject(new Error("OSL friend is unknown"));
      if (command === GET_FUTURE_ACCOUNT_COMMAND) {
        return Promise.resolve({ personId: args.personId, enabled, result: enabled ? "on" : "off" });
      }
      if (command === SET_FUTURE_ACCOUNT_COMMAND && typeof args.enabled === "boolean") {
        saved.set(args.personId, args.enabled);
        return Promise.resolve({ personId: args.personId, enabled: args.enabled, result: args.enabled ? "on" : "off" });
      }
      return Promise.reject(new Error(`unexpected command ${command}`));
    },
    recordBackendFailure: () => undefined,
  };
  return { saved, commands, dependencies };
}

/** Open the friend-page fixture: load every saved state, then draw 4 rows. */
async function openFixture(hub: FakeHub): Promise<RenderedSwitch[]> {
  const states = await loadFutureAccountSwitchStates(FRIENDS, hub.dependencies);
  const markup = FRIENDS.map((personId) =>
    `<article class="person-row person-profile"><details class="friend-management"><summary>Manage</summary><div>${
      futureAccountSwitchMarkup({ personId, enabled: states.get(personId) ?? false })
    }</div></details></article>`).join("");
  return renderedSwitches(markup);
}

function renderedSwitches(markup: string): RenderedSwitch[] {
  const found: RenderedSwitch[] = [];
  const pattern = /<label class="setting-line interactive future-account-switch"([^>]*)>([\s\S]*?)<\/label>/gu;
  for (const match of markup.matchAll(pattern)) {
    const attrs = match[1] ?? "";
    found.push({
      personId: /data-future-account-switch="([^"]*)"/u.exec(attrs)?.[1] ?? "",
      state: /data-future-account-switch-state="([^"]*)"/u.exec(attrs)?.[1] ?? "",
      checked: /<input\b[^>]*\bchecked\b/u.test(match[2] ?? ""),
    });
  }
  return found;
}

/** How many reopened switches disagree with the Hub's saved state. */
function mismatchCount(switches: RenderedSwitch[], saved: Map<string, boolean>): number {
  return FRIENDS.filter((personId) => {
    const drawn = switches.find((entry) => entry.personId === personId);
    return drawn === undefined || drawn.checked !== saved.get(personId);
  }).length;
}

describe("TASK0268 future-account switch connection", () => {
  it("wires the switch to the future-account command in main.ts", () => {
    expect(mainSource).toContain('import { loadFutureAccountSwitchStates, saveFutureAccountSwitch } from "./future-account-switch-connect";');
    expect(mainSource).toContain('document.querySelectorAll<HTMLInputElement>("[data-future-account-toggle]")');
    expect(mainSource).toContain("input.addEventListener(\"change\", (event) => void changeFriendFutureAccountSwitch(event.currentTarget as HTMLInputElement))");
    expect(mainSource).toContain("const saved = await saveFutureAccountSwitch(personId, requested, { isTauriRuntime, invoke, recordBackendFailure });");
    expect(mainSource).toContain("await loadFutureAccountSwitchStates(personIds, { isTauriRuntime, invoke, recordBackendFailure })");
    expect(mainSource).toContain("if (saved) friendFutureAccountAutoWhitelist.set(saved.personId, saved.enabled);");
  });

  it("reopening shows the last saved state for all 4 switches", async () => {
    const hub = fakeHub({
      "hub-person-ada": true,
      "hub-person-bel": false,
      "hub-person-cai": true,
      "hub-person-dot": false,
    });

    const opened = await openFixture(hub);
    expect(opened).toHaveLength(4);
    expect(mismatchCount(opened, hub.saved)).toBe(0);

    // Flip two switches through the connected save path.
    expect(await saveFutureAccountSwitch("hub-person-ada", false, hub.dependencies)).toEqual({ personId: "hub-person-ada", enabled: false });
    expect(await saveFutureAccountSwitch("hub-person-dot", true, hub.dependencies)).toEqual({ personId: "hub-person-dot", enabled: true });

    // The Hub stored the flips, through the real command name.
    expect(hub.commands.filter((command) => command === SET_FUTURE_ACCOUNT_COMMAND)).toHaveLength(2);
    expect(hub.saved.get("hub-person-ada")).toBe(false);
    expect(hub.saved.get("hub-person-dot")).toBe(true);

    const reopened = await openFixture(hub);
    const mismatches = mismatchCount(reopened, hub.saved);

    for (const entry of reopened) {
      console.log(`TASK0268 reopened ${entry.personId} saved=${hub.saved.get(entry.personId) ? "on" : "off"} shown=${entry.state}`);
    }
    console.log(`TASK0268 reopened switches=${reopened.length} mismatches=${mismatches}`);

    expect(reopened).toHaveLength(4);
    expect(reopened.map((entry) => entry.state)).toEqual(["off", "off", "on", "on"]);
    expect(mismatches).toBe(0);
    expect(hub.commands.filter((command) => command === GET_FUTURE_ACCOUNT_COMMAND)).toHaveLength(8);
  });

  it("flipping 1 switch without saving leaves the reopened count at 0", async () => {
    const hub = fakeHub({
      "hub-person-ada": true,
      "hub-person-bel": false,
      "hub-person-cai": true,
      "hub-person-dot": false,
    });

    const opened = await openFixture(hub);
    // The unsaved flip: the checkbox changes on screen, but the change never
    // reaches the set command (the user closed the page mid-toggle).
    const flipped = opened.map((entry) => entry.personId === "hub-person-bel" ? { ...entry, checked: !entry.checked } : entry);
    const flippedInView = mismatchCount(flipped, hub.saved);
    expect(flippedInView).toBe(1);
    expect(hub.commands.filter((command) => command === SET_FUTURE_ACCOUNT_COMMAND)).toHaveLength(0);

    const reopened = await openFixture(hub);
    const mismatches = mismatchCount(reopened, hub.saved);
    console.log(`TASK0268 unsaved flips=${flippedInView} reopened switches=${reopened.length} mismatches=${mismatches}`);

    expect(reopened).toHaveLength(4);
    expect(mismatches).toBe(0);
  });
});
