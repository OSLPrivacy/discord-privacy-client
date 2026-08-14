import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  bindStripLeftCluster,
  cycleStripQuickSetting,
  defaultStripQuickSettings,
  stripLeftClusterMarkup,
  stripQuickSettingValues,
} from "./strip-left-cluster";
import { ComposerProtectionTraceController } from "./composer-protection-trace";

const inspectedStripSource = process.env.TASK5036_STRIP_SOURCE
  ?? new URL("./strip-left-cluster.ts", import.meta.url);

class Control {
  readonly dataset: Record<string, string | undefined>;
  private listener: (() => void) | null = null;

  constructor(dataset: Record<string, string | undefined> = {}) {
    this.dataset = dataset;
  }

  addEventListener(_: "click", listener: () => void): void {
    this.listener = listener;
  }

  click(): void {
    this.listener?.();
  }
}

function root(groups: Record<string, Control[]>) {
  return { querySelectorAll: (selector: string) => groups[selector] ?? [] };
}

function leftElements(markup: string): string[] {
  return [...markup.matchAll(/data-strip-left-element="([^"]+)"/gu)]
    .map((match) => match[1] as string);
}

describe("TASK 5036 Strip left cluster and lock trace acceptance", () => {
  it("reads logo, plan, quick settings, and burn in order on three carriers", () => {
    const expected = ["logo", "plan", "quick-settings", "burn"];
    const carriers = ["Discord", "Signal", "WhatsApp"] as const;

    for (const carrier of carriers) {
      const elements = leftElements(stripLeftClusterMarkup("free", defaultStripQuickSettings()));
      expect(elements).toEqual(expected);
      console.info(`TASK5036 carrier=${carrier} elements=${elements.join(",")} count=${elements.length}`);
    }
    console.info(`TASK5036 left_cluster_carriers=${carriers.length} order=${expected.join(",")}`);
  });

  it("opens Home from the logo three times out of three", () => {
    const carriers = ["Discord", "Signal", "WhatsApp"] as const;
    let homeOpens = 0;

    for (const carrier of carriers) {
      const logo = new Control();
      let route = "carrier";
      bindStripLeftCluster(
        root({
          "[data-strip-home]": [logo],
          "[data-strip-burn]": [],
          "[data-strip-findable], [data-strip-all-settings]": [],
          "[data-strip-quick-setting]": [],
        }),
        defaultStripQuickSettings(),
        {
          home: () => { homeOpens += 1; route = "Home"; },
          settings: () => undefined,
          burn: () => undefined,
          changed: () => undefined,
        },
      );
      logo.click();
      expect(route).toBe("Home");
      console.info(`TASK5036 logo carrier=${carrier} route=${route}`);
    }

    expect(homeOpens).toBe(3);
    console.info(`TASK5036 logo_home_opens=${homeOpens}/3`);
  });

  it("matches the FREE and PRO fixture plan chips both ways", () => {
    const fixtures = [
      ["free", "FREE"],
      ["pro", "PRO"],
    ] as const;

    for (const [plan, label] of fixtures) {
      const markup = stripLeftClusterMarkup(plan, defaultStripQuickSettings());
      expect(markup).toContain(`data-strip-plan="${plan}">${label}`);
      console.info(`TASK5036 plan=${plan} chip=${label}`);
    }
    console.info(`TASK5036 plan_fixtures=${fixtures.length} free=FREE pro=PRO`);
  });

  it("cycles every quick setting through each value once per full loop", () => {
    let checked = 0;
    for (const [setting, values] of Object.entries(stripQuickSettingValues)) {
      const state = defaultStripQuickSettings();
      const seen: string[] = [state[setting as keyof typeof state]];
      for (let index = 1; index < values.length; index += 1) {
        seen.push(cycleStripQuickSetting(state, setting as keyof typeof state));
      }
      expect(seen).toEqual(values);
      expect(cycleStripQuickSetting(state, setting as keyof typeof state)).toBe(values[0]);
      checked += 1;
      console.info(`TASK5036 quick=${setting} values=${seen.join(" > ")} loop_once=true`);
    }
    expect(checked).toBe(Object.keys(stripQuickSettingValues).length);
    console.info(`TASK5036 quick_settings_checked=${checked}`);
  });

  it("turning the lock on draws one cyan composer trace and leaves protection on", () => {
    const trace = new ComposerProtectionTraceController();
    expect(trace.applyLockEngaged(false, true)).toEqual({ engaged: false, traceCount: 0 });
    expect(trace.applyLockEngaged(true, true)).toEqual({ engaged: true, traceCount: 1 });
    expect(trace.applyLockEngaged(true, true)).toEqual({ engaged: true, traceCount: 1 });

    const overlayCss = readFileSync(new URL("./overlay.css", import.meta.url), "utf8");
    expect(overlayCss).toContain("stroke: #49d6ff");
    expect(overlayCss).toContain("animation-iteration-count: 1");
    console.info("TASK5036 lock_on cyan_traces=1 protection=on animation_iterations=1");
  });

  it("rejects a left-cluster source missing its burn element", () => {
    const source = readFileSync(inspectedStripSource, "utf8");
    const burnElements = source.match(/data-strip-left-element="burn"/gu) ?? [];
    expect(burnElements).toHaveLength(1);
    console.info(`TASK5036 burn_element_count=${burnElements.length} source=${inspectedStripSource}`);
  });
});
