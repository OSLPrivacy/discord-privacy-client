import { describe, expect, it } from "vitest";
import {
  DEADMAN_LIMITS,
  DEADMAN_WIPE_CONFIRMATION,
  renderDeadmanScreen,
  selectDeadmanAction,
} from "./deadman";

describe("USB dead-man switch", () => {
  it("defaults to the recoverable lock action", () => {
    expect(selectDeadmanAction("lock", "")).toEqual({ action: "lock", wipeConfirmed: false });
  });

  it("refuses wipe until its exact typed confirmation is supplied", () => {
    const selection = selectDeadmanAction("wipe", "wipe");

    expect(selection).toEqual({ action: "lock", wipeConfirmed: false });
  });

  it("enables wipe only after its exact typed confirmation", () => {
    expect(selectDeadmanAction("wipe", DEADMAN_WIPE_CONFIRMATION)).toEqual({
      action: "wipe",
      wipeConfirmed: true,
    });
  });

  it("renders every honest limit on the choice screen", () => {
    const screen = renderDeadmanScreen(selectDeadmanAction("lock", ""));

    expect(screen).toMatch(/class="[^"\n]*\bdeadman-settings\b[^"\n]*"/u);
    expect(screen).toMatch(/class="[^"\n]*\bdeadman-limits\b[^"\n]*"/u);
    for (const limit of DEADMAN_LIMITS) expect(screen).toContain(limit);
    expect(screen.indexOf("deadman-limits")).toBeGreaterThan(screen.indexOf("deadman-action"));
    expect(screen).toContain(`Type ${DEADMAN_WIPE_CONFIRMATION} to enable wipe`);
  });
});
