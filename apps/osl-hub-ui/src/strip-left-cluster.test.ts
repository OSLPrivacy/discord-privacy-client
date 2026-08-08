import { describe, expect, it } from "vitest";
import { bindStripLeftCluster, cycleStripQuickSetting, defaultStripQuickSettings, stripLeftClusterMarkup, stripQuickSettingValues } from "./strip-left-cluster";

class Control {
  readonly dataset: Record<string, string | undefined>;
  private listener: (() => void) | null = null;
  constructor(dataset: Record<string, string | undefined> = {}) { this.dataset = dataset; }
  addEventListener(_: "click", listener: () => void): void { this.listener = listener; }
  click(): void { this.listener?.(); }
}

function root(groups: Record<string, Control[]>) { return { querySelectorAll: (selector: string) => groups[selector] ?? [] }; }

describe("TASK5021 Strip left cluster", () => {
  it("renders exactly logo, plan, quick settings, and burn; the plan follows the fixture", () => {
    const free = stripLeftClusterMarkup("free", defaultStripQuickSettings());
    const pro = stripLeftClusterMarkup("pro", defaultStripQuickSettings());
    expect([...free.matchAll(/data-strip-left-element="([^"]+)"/gu)].map((match) => match[1])).toEqual(["logo", "plan", "quick-settings", "burn"]);
    expect(free).toContain('data-strip-plan="free">FREE');
    expect(pro).toContain('data-strip-plan="pro">PRO');
    console.info("TASK5021 elements=4 order=logo,plan,quick-settings,burn free=FREE pro=PRO");
  });

  it("cycles every named quick-setting value in order and sends findability to Settings", () => {
    const state = defaultStripQuickSettings();
    const loops: string[] = [];
    for (const [setting, values] of Object.entries(stripQuickSettingValues)) {
      const seen = [state[setting as keyof typeof state]];
      for (let index = 0; index < values.length; index += 1) seen.push(cycleStripQuickSetting(state, setting as keyof typeof state));
      expect(seen).toEqual([...values, values[0]]);
      loops.push(`${setting}=${seen.join(" > ")}`);
    }
    const findable = new Control(); const allSettings = new Control(); const changes = { count: 0, settings: 0, home: 0, burn: 0 };
    bindStripLeftCluster(root({ "[data-strip-home]": [], "[data-strip-burn]": [], "[data-strip-findable], [data-strip-all-settings]": [findable, allSettings], "[data-strip-quick-setting]": [] }), state, { home: () => { changes.home += 1; }, burn: () => { changes.burn += 1; }, settings: () => { changes.settings += 1; }, changed: () => { changes.count += 1; } });
    findable.click(); allSettings.click();
    expect(changes).toEqual({ count: 0, settings: 2, home: 0, burn: 0 });
    console.info(`TASK5021 quick_loops=${loops.join("; ")}`);
    console.info(`TASK5021 findable_cycles=0 settings_opens=${changes.settings}`);
  });

  it("opens Home from three carrier fixtures", () => {
    let homes = 0;
    for (const carrier of ["Discord", "Signal", "WhatsApp"]) {
      const logo = new Control();
      bindStripLeftCluster(root({ "[data-strip-home]": [logo], "[data-strip-burn]": [], "[data-strip-findable], [data-strip-all-settings]": [], "[data-strip-quick-setting]": [] }), defaultStripQuickSettings(), { home: () => { homes += 1; }, burn: () => undefined, settings: () => undefined, changed: () => undefined });
      logo.click();
      console.info(`TASK5021 carrier=${carrier} route=Home`);
    }
    expect(homes).toBe(3);
  });
});
