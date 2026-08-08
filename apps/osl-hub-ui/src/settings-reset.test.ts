import { describe, expect, it, vi } from "vitest";
import { resetSettingsScreen, type SettingsResetGroup } from "./settings-reset";

describe("per-screen Settings Reset", () => {
  it("resets Look only and preserves changed Notices, Friends, and Behaviour choices", async () => {
    const state = { Look: { theme: "changed" }, Notices: { alerts: "changed" }, Friends: { reach: "changed" }, Behaviour: { sound: "changed" } };
    const invoke = vi.fn(async (_command: string, { group }: { group: SettingsResetGroup }) => {
      state[group] = {} as never;
      return { action: "reset" as const, group, settingsDefaulted: 1 };
    });
    await resetSettingsScreen("Look", invoke);
    expect(state).toEqual({ Look: {}, Notices: { alerts: "changed" }, Friends: { reach: "changed" }, Behaviour: { sound: "changed" } });
    expect(invoke).toHaveBeenCalledExactlyOnceWith("reset_hub_setting_group", { group: "Look" });
  });
});
