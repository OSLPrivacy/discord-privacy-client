/** The four controls on the left of the carrier Strip (PRODUCT §5). */
export type StripPlan = "free" | "pro";
export type StripQuickSetting = "warnings" | "coverText" | "sendWith" | "clipboardClearsAfter" | "findable" | "whitelist" | "logs" | "allSettings";

export const stripQuickSettingValues: Readonly<Record<Exclude<StripQuickSetting, "findable" | "allSettings">, readonly string[]>> = {
  warnings: ["warn me", "block send", "mark only"],
  coverText: ["wordbank", "AI [PRO]"],
  sendWith: ["Enter", "Enter x2", "Clipboard"],
  clipboardClearsAfter: ["30 sec", "never", "manual"],
  whitelist: ["ask", "auto", "strict"],
  logs: ["off", "on"],
};

export type StripQuickSettingsState = Record<Exclude<StripQuickSetting, "findable" | "allSettings">, string>;

export const defaultStripQuickSettings = (): StripQuickSettingsState => ({
  warnings: "warn me",
  coverText: "wordbank",
  sendWith: "Enter",
  clipboardClearsAfter: "30 sec",
  whitelist: "ask",
  logs: "off",
});

const labels: Readonly<Record<StripQuickSetting, string>> = {
  warnings: "Warnings",
  coverText: "Cover text",
  sendWith: "Send with",
  clipboardClearsAfter: "Clipboard clears after",
  findable: "Findable by strangers",
  whitelist: "Whitelist",
  logs: "Logs",
  allSettings: "All settings",
};

export function cycleStripQuickSetting(state: StripQuickSettingsState, setting: Exclude<StripQuickSetting, "findable" | "allSettings">): string {
  const values = stripQuickSettingValues[setting];
  const index = values.indexOf(state[setting]);
  const next = values[(index + 1) % values.length];
  state[setting] = next;
  return next;
}

export function stripLeftClusterMarkup(plan: StripPlan, state: StripQuickSettingsState): string {
  const quick = (Object.keys(stripQuickSettingValues) as Array<Exclude<StripQuickSetting, "findable" | "allSettings">>)
    .map((setting) => `<button type="button" class="strip-quick-setting" data-strip-quick-setting="${setting}"><span>${labels[setting]}</span><strong>${state[setting]}</strong></button>`)
    .join("");
  // These are deliberately the only direct children: the carrier's right
  // cluster is built and owned separately.
  return `<div class="strip-left-cluster" data-strip-left-cluster><button type="button" class="strip-logo" data-strip-left-element="logo" data-strip-home aria-label="OSL Home">OSL</button><span class="strip-plan-chip" data-strip-left-element="plan" data-strip-plan="${plan}">${plan === "pro" ? "PRO" : "FREE"}</span><details class="strip-quick-settings" data-strip-left-element="quick-settings"><summary>Quick settings</summary><div class="strip-quick-settings-menu">${quick}<button type="button" class="strip-quick-setting readonly" data-strip-findable><span>Findable by strangers</span><strong>Settings</strong></button><button type="button" class="strip-quick-setting readonly" data-strip-all-settings><span>All settings</span><strong>Open</strong></button></div></details><button type="button" class="strip-burn" data-strip-left-element="burn" data-strip-burn>Burn</button></div>`;
}

type StripControl = { readonly dataset: { readonly stripQuickSetting?: string }; addEventListener(type: "click", listener: () => void): void };
export interface StripLeftClusterRoot { querySelectorAll(selector: string): Iterable<StripControl> }

export function bindStripLeftCluster(root: StripLeftClusterRoot, state: StripQuickSettingsState, actions: { home(): void; settings(): void; burn(): void; changed(): void }): number {
  let bindings = 0;
  for (const control of root.querySelectorAll("[data-strip-home]")) { bindings += 1; control.addEventListener("click", actions.home); }
  for (const control of root.querySelectorAll("[data-strip-burn]")) { bindings += 1; control.addEventListener("click", actions.burn); }
  // Findability is intentionally read-only in the Strip (rule 4761): it has
  // zero values here and always takes the operator to Settings.
  for (const control of root.querySelectorAll("[data-strip-findable], [data-strip-all-settings]")) { bindings += 1; control.addEventListener("click", actions.settings); }
  for (const control of root.querySelectorAll("[data-strip-quick-setting]")) {
    const setting = control.dataset.stripQuickSetting as Exclude<StripQuickSetting, "findable" | "allSettings"> | undefined;
    if (!setting || !(setting in stripQuickSettingValues)) continue;
    bindings += 1;
    control.addEventListener("click", () => { cycleStripQuickSetting(state, setting); actions.changed(); });
  }
  return bindings;
}
