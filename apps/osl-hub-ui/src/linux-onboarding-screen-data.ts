export const LINUX_ONBOARDING_SCREEN_WINDOW = {
  width: 1280,
  height: 800,
} as const;

export const LINUX_ONBOARDING_SCREEN_FIXTURES = {
  accounts: [
    {
      id: "osl-linux-screen-alma",
      service: "Signal",
      handle: "+15550130324",
      ownerName: "Alma Reed",
    },
    {
      id: "osl-linux-screen-miles",
      service: "Discord",
      handle: "miles.fixed.0324",
      ownerName: "Miles Chen",
    },
  ],
  names: ["Alma Reed", "Miles Chen", "Nora Vale"],
  phrases: [
    "amber cabin delta frost harbor ivory juniper lantern meadow nickel orbit quartz",
    "atlas broom cedar dusk ember flint grove honest iris kettle lunar mint",
  ],
} as const;

export const LINUX_ONBOARDING_REQUIRED_CONTROLS = [
  "Continue",
  "Back",
  "Create account",
  "Sign in",
] as const;

export type LinuxOnboardingRequiredControl = typeof LINUX_ONBOARDING_REQUIRED_CONTROLS[number];

export type LinuxOnboardingScreenName =
  | "create-account"
  | "sign-in"
  | "continue"
  | "back";

export type LinuxOnboardingScreenData = {
  name: LinuxOnboardingScreenName;
  controls: string[];
  textLength: number;
};

export type LinuxOnboardingScreenRun = {
  run: string;
  window: typeof LINUX_ONBOARDING_SCREEN_WINDOW;
  fixtures: typeof LINUX_ONBOARDING_SCREEN_FIXTURES;
  screens: LinuxOnboardingScreenData[];
  controls: Record<LinuxOnboardingRequiredControl, boolean>;
};

type LinuxOnboardingScreenMarkup = Record<LinuxOnboardingScreenName, string>;

function decodeHtmlText(value: string): string {
  return value
    .replace(/&nbsp;/gu, " ")
    .replace(/&amp;/gu, "&")
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&quot;/gu, "\"")
    .replace(/&#39;/gu, "'");
}

export function visibleText(markup: string): string {
  return decodeHtmlText(markup)
    .replace(/<svg\b[\s\S]*?<\/svg>/gu, " ")
    .replace(/<[^>]*>/gu, " ")
    .replace(/[←→]/gu, " ")
    .replace(/\s+/gu, " ")
    .trim();
}

export function buttonControls(markup: string): string[] {
  const controls: string[] = [];
  for (const match of markup.matchAll(/<button\b[^>]*>([\s\S]*?)<\/button>/gu)) {
    const text = visibleText(match[1]);
    if (text) controls.push(text);
  }
  return controls;
}

export function prepareFixedLinuxOnboardingScreenData(
  run: string,
  markup: LinuxOnboardingScreenMarkup,
): LinuxOnboardingScreenRun {
  const screens = (Object.keys(markup) as LinuxOnboardingScreenName[]).map((name) => ({
    name,
    controls: buttonControls(markup[name]),
    textLength: visibleText(markup[name]).length,
  }));
  const allControls = new Set(screens.flatMap((screen) => screen.controls));

  return {
    run,
    window: LINUX_ONBOARDING_SCREEN_WINDOW,
    fixtures: LINUX_ONBOARDING_SCREEN_FIXTURES,
    screens,
    controls: Object.fromEntries(
      LINUX_ONBOARDING_REQUIRED_CONTROLS.map((control) => [control, allControls.has(control)]),
    ) as Record<LinuxOnboardingRequiredControl, boolean>,
  };
}
