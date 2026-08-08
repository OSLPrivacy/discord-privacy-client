export const SETTINGS_RESET_GROUPS = ["Account", "Whitelisting", "Privacy", "Notifications", "Apps and sending", "Look", "Behaviour"] as const;
export type SettingsResetGroup = typeof SETTINGS_RESET_GROUPS[number];

export interface SettingsResetReceipt {
  readonly action: "reset";
  readonly group: SettingsResetGroup;
  readonly settingsDefaulted: number;
}

/** Invoke the reset command for precisely the screen whose control was pressed. */
export async function resetSettingsScreen(
  group: SettingsResetGroup,
  invoke: (command: string, args: { group: SettingsResetGroup }) => Promise<SettingsResetReceipt>,
): Promise<SettingsResetReceipt> {
  const receipt = await invoke("reset_hub_setting_group", { group });
  if (receipt.action !== "reset" || receipt.group !== group) throw new Error("OSL Settings reset receipt did not match this screen");
  return receipt;
}
