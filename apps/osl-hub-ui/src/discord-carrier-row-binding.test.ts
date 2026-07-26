import { describe, expect, it } from "vitest";
import {
  applyCarrierRowGeometry,
  clearCarrierRowGeometry,
  parseNativeDiscordCarrierRowBindings,
} from "./discord-carrier-row-binding";

const binding = {
  messageId: "msg-0123456789abcdef",
  nativeLocatorSha256: "1".repeat(64),
  carrierSha256: "2".repeat(64),
  leftPx: 184,
  topPx: 412,
  widthPx: 528,
  heightPx: 38,
  backgroundColor: "rgb(49 51 56)",
  foregroundColor: "rgb(219 222 225)",
  fontFamily: "gg sans",
  fontSizePx: 16,
  fontWeight: 400,
  lineHeightPx: 20,
  letterSpacingPx: 0,
  zoom: 1,
  density: 1.25,
};

describe("native Discord carrier row binding", () => {
  it("accepts only unique, bounded, hash-bound native rows", () => {
    expect(parseNativeDiscordCarrierRowBindings([binding])).toEqual([binding]);
    expect(parseNativeDiscordCarrierRowBindings([{ ...binding, plaintext: "secret" }])).toBeNull();
    expect(parseNativeDiscordCarrierRowBindings([{ ...binding, heightPx: 11 }])).toBeNull();
    expect(parseNativeDiscordCarrierRowBindings([{ ...binding, backgroundColor: "red" }])).toBeNull();
    expect(parseNativeDiscordCarrierRowBindings([{ ...binding, fontFamily: "gg sans;display:none" }])).toBeNull();
    expect(parseNativeDiscordCarrierRowBindings([{ ...binding, lineHeightPx: 0 }])).toBeNull();
    expect(parseNativeDiscordCarrierRowBindings([{ ...binding, leftPx: -1 }])).toBeNull();
    expect(parseNativeDiscordCarrierRowBindings([
      binding,
      { ...binding, messageId: "msg-other" },
    ])).toBeNull();
    expect(parseNativeDiscordCarrierRowBindings([
      binding,
      { ...binding, nativeLocatorSha256: "3".repeat(64) },
    ])).toBeNull();
  });

  it("reveals only an exactly positioned bound row and clears it fail closed", () => {
    const classes = new Set<string>();
    const properties = new Map<string, string>();
    const row = {
      hidden: true,
      dataset: {} as Record<string, string>,
      classList: {
        add(value: string) { classes.add(value); },
        remove(value: string) { classes.delete(value); },
        contains(value: string) { return classes.has(value); },
      },
      style: {
        setProperty(name: string, value: string) { properties.set(name, value); },
        removeProperty(name: string) {
          const previous = properties.get(name) ?? "";
          properties.delete(name);
          return previous;
        },
        getPropertyValue(name: string) { return properties.get(name) ?? ""; },
      },
    } as unknown as HTMLElement;
    applyCarrierRowGeometry(row, binding);
    expect(row.hidden).toBe(false);
    expect(row.classList.contains("osl-discord-transcript__row--carrier-bound")).toBe(true);
    expect(row.dataset.nativeLocatorSha256).toBe(binding.nativeLocatorSha256);
    expect(row.style.getPropertyValue("--osl-carrier-left")).toBe("184px");
    expect(row.style.getPropertyValue("--osl-carrier-top")).toBe("412px");
    expect(row.style.getPropertyValue("--osl-carrier-width")).toBe("528px");
    expect(row.style.getPropertyValue("--osl-carrier-height")).toBe("38px");
    expect(row.style.getPropertyValue("--osl-carrier-background")).toBe("rgb(49 51 56)");
    expect(row.style.getPropertyValue("--osl-carrier-foreground")).toBe("rgb(219 222 225)");
    expect(row.style.getPropertyValue("--osl-carrier-font-family")).toBe('"gg sans"');
    expect(row.style.getPropertyValue("--osl-carrier-line-height")).toBe("20px");

    clearCarrierRowGeometry(row);
    expect(row.hidden).toBe(true);
    expect(row.classList.contains("osl-discord-transcript__row--carrier-bound")).toBe(false);
    expect(row.dataset.nativeLocatorSha256).toBeUndefined();
    expect(row.style.getPropertyValue("--osl-carrier-left")).toBe("");
    expect(row.style.getPropertyValue("--osl-carrier-background")).toBe("");
  });

  it("writes every measured metric verbatim and re-writes it on a fresh measurement", () => {
    // These rows paint decrypted text directly onto individual Discord message
    // rows, so any rounding, flooring or substitution between the measurement
    // and the CSS variable is a visible seam. Fractional and small values are
    // the cases that catch a clamp, so they are the ones asserted here: the
    // 10.5px size is below the 11px floor typography.test.ts holds OSL's own
    // chrome to, and must still land unaltered.
    const properties = new Map<string, string>();
    const classes = new Set<string>();
    const row = {
      hidden: true,
      dataset: {} as Record<string, string>,
      classList: {
        add(value: string) { classes.add(value); },
        remove(value: string) { classes.delete(value); },
        contains(value: string) { return classes.has(value); },
      },
      style: {
        setProperty(name: string, value: string) { properties.set(name, value); },
        removeProperty(name: string) { properties.delete(name); return ""; },
        getPropertyValue(name: string) { return properties.get(name) ?? ""; },
      },
    } as unknown as HTMLElement;

    const zoomedOut = {
      ...binding,
      fontSizePx: 10.5,
      fontWeight: 350,
      lineHeightPx: 13.125,
      letterSpacingPx: -0.25,
    };
    applyCarrierRowGeometry(row, zoomedOut);
    expect(row.style.getPropertyValue("--osl-carrier-font-size")).toBe("10.5px");
    expect(row.style.getPropertyValue("--osl-carrier-font-weight")).toBe("350");
    expect(row.style.getPropertyValue("--osl-carrier-line-height")).toBe("13.125px");
    expect(row.style.getPropertyValue("--osl-carrier-letter-spacing")).toBe("-0.25px");

    // Self-heal: Discord is zoomed back in and the backend re-measures. Every
    // variable has to converge on the new measurement, with nothing stale left
    // over from the previous one -- applyCarrierRowGeometry clears first.
    applyCarrierRowGeometry(row, { ...binding, fontSizePx: 17, lineHeightPx: 22.5 });
    expect(row.style.getPropertyValue("--osl-carrier-font-size")).toBe("17px");
    expect(row.style.getPropertyValue("--osl-carrier-font-weight")).toBe("400");
    expect(row.style.getPropertyValue("--osl-carrier-line-height")).toBe("22.5px");
    expect(row.style.getPropertyValue("--osl-carrier-letter-spacing")).toBe("0px");
    expect(row.hidden).toBe(false);
  });
});
