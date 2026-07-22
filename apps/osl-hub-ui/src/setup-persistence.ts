export const activeSetupRoutes = [
  "browser",
  "detected",
  "mullvad",
  "sending",
  "passwords",
  "burnpass",
  "privacy",
  "scrub",
] as const;

export type ActiveSetupRoute = typeof activeSetupRoutes[number];
export const scrubSetupSteps = ["intro", "accounts", "options"] as const;
export type ScrubSetupStep = typeof scrubSetupSteps[number];

export interface SetupResumeCheckpoint {
  route: ActiveSetupRoute;
  scrubStep: ScrubSetupStep;
}

const activeRouteSet = new Set<string>(activeSetupRoutes);
const scrubStepSet = new Set<string>(scrubSetupSteps);

export function isActiveSetupRoute(value: string): value is ActiveSetupRoute {
  return activeRouteSet.has(value);
}

export function parseSetupResumeCheckpoint(raw: string | null): SetupResumeCheckpoint | null {
  try {
    const value = JSON.parse(raw ?? "null") as unknown;
    if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
    const candidate = value as Partial<SetupResumeCheckpoint>;
    if (typeof candidate.route !== "string" || !isActiveSetupRoute(candidate.route)) return null;
    const scrubStep = typeof candidate.scrubStep === "string" && scrubStepSet.has(candidate.scrubStep)
      ? candidate.scrubStep as ScrubSetupStep
      : "intro";
    return { route: candidate.route, scrubStep: candidate.route === "scrub" ? scrubStep : "intro" };
  } catch {
    return null;
  }
}

export const setupPrivacyChoiceIds = [
  "hide-notifications",
  "auto-lock",
  "disable-previews",
  "ip-grabber-protection",
  "external-default-browser",
  "clear-clipboard",
] as const;

export type SetupPrivacyChoiceId = typeof setupPrivacyChoiceIds[number];

export function parseSetupPrivacyChoices(raw: string | null): Set<SetupPrivacyChoiceId> {
  // Only notification redaction has a complete runtime implementation today.
  // Unsupported future choices must never become active merely because setup
  // storage is missing, malformed, or from a newer build.
  const defaults = new Set<SetupPrivacyChoiceId>(["hide-notifications"]);
  try {
    const value = JSON.parse(raw ?? "null") as unknown;
    if (!Array.isArray(value) || value.length > setupPrivacyChoiceIds.length) return defaults;
    const allowed = new Set<string>(setupPrivacyChoiceIds);
    if (!value.every((choice) => typeof choice === "string" && allowed.has(choice))) return defaults;
    return new Set(value as SetupPrivacyChoiceId[]);
  } catch {
    return defaults;
  }
}
