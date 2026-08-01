import type { SendMode } from "./state";

/**
 * Product classification for Windows desktop surfaces.
 *
 * This file intentionally does not launch anything. A service is promoted to
 * `verified` only after the Rust boundary has a fixed executable/package
 * identity and an exact publisher allowlist. `candidate` services must fail
 * closed instead of silently opening their website.
 */
export type DesktopServiceId =
  | "discord"
  | "outlook"
  | "proton"
  | "tuta"
  | "fastmail"
  | "zoho"
  | "slack"
  | "teams"
  | "instagram"
  | "messenger"
  | "x"
  | "snapchat"
  | "gmail"
  | "yahoo"
  | "aol"
  | "gmx"
  | "maildotcom"
  | "icloud";

export type WindowsDesktopSurface =
  | "verified"
  | "candidate"
  | "packagedWeb"
  | "browserOnly";

export interface DesktopServicePolicy {
  id: DesktopServiceId;
  surface: WindowsDesktopSurface;
  /** A desktop candidate never falls back to a normal browser implicitly. */
  nativeOnly: boolean;
  /** No reviewed secondary-profile launch contract exists for these apps. */
  separateProfileAvailable: false;
  /**
   * Send-mode account-risk facts for this service.
   *
   * Terms status and enforcement likelihood are independent. If reliable,
   * current enforcement evidence is absent, likelihood stays `Unknown`; terms
   * wording alone must not become a ban percentage or prediction.
   */
  sendModeRisk?: readonly {
    mode: Exclude<SendMode, "manual">;
    termsStatus: "No known restriction" | "May conflict with terms" | "Explicitly restricted" | "Unknown";
    enforcementLikelihood: "Low" | "Medium" | "High" | "Unknown";
    enforcementEvidence: { source: string; reviewedAt: string } | null;
    explanation: string;
  }[];
}

export type OslMailStageId = "stageA" | "stageB" | "stageC";

export interface OslMailStage {
  id: OslMailStageId;
  label: string;
  availability: "available" | "comingLater";
  boundary: string;
  includes: readonly string[];
  excludes: readonly string[];
  externalEmailScope: "ordinaryExternalEmailUnlessSeparatelySupported";
}

export const oslMailStages: readonly OslMailStage[] = [
  {
    id: "stageA",
    label: "Stage A - private email client",
    availability: "comingLater",
    boundary: "Client-side protection is unavailable until the OSL Mail desktop bridge exists.",
    includes: [
      "connect existing mailbox after authorization",
      "warn before send",
      "sanitize links and attachments",
      "organize retention",
      "label protection scope honestly",
    ],
    excludes: [
      "OSL-operated mailbox",
      "custom domain hosting",
      "universal encrypted delivery",
      "silent mailbox import",
      "server-side mail operations",
    ],
    externalEmailScope: "ordinaryExternalEmailUnlessSeparatelySupported",
  },
  {
    id: "stageB",
    label: "Stage B - OSL aliases and relay",
    availability: "comingLater",
    boundary: "Aliases and relay only after abuse, deliverability, reply routing, recovery and support gates pass.",
    includes: ["disposable aliases", "reply relay", "breach isolation"],
    excludes: ["complete OSL mailbox", "custom domain hosting", "universal encrypted delivery"],
    externalEmailScope: "ordinaryExternalEmailUnlessSeparatelySupported",
  },
  {
    id: "stageC",
    label: "Stage C - OSL mailbox",
    availability: "comingLater",
    boundary: "Full mailbox only after a separate mail operations review.",
    includes: ["optional OSL address", "custom domains", "calendar", "encrypted storage"],
    excludes: ["available by default", "bundled by default", "beta by default"],
    externalEmailScope: "ordinaryExternalEmailUnlessSeparatelySupported",
  },
] as const;

export function oslMailStage(id: OslMailStageId): OslMailStage {
  const stage = oslMailStages.find((entry) => entry.id === id);
  if (!stage) throw new Error("unknown OSL Mail stage");
  return stage;
}

const policy = (
  id: DesktopServiceId,
  surface: WindowsDesktopSurface,
  sendModeRisk?: DesktopServicePolicy["sendModeRisk"],
): DesktopServicePolicy => ({
  id,
  surface,
  nativeOnly: surface === "verified" || surface === "candidate",
  separateProfileAvailable: false,
  ...(sendModeRisk ? { sendModeRisk } : {}),
});

const discordSendModeRisk: NonNullable<DesktopServicePolicy["sendModeRisk"]> = [
  {
    mode: "clipboard",
    termsStatus: "No known restriction",
    enforcementLikelihood: "Unknown",
    enforcementEvidence: null,
    explanation: "OSL encrypts and copies; the user chooses where and when to paste and send.",
  },
  {
    mode: "double",
    termsStatus: "May conflict with terms",
    enforcementLikelihood: "Unknown",
    enforcementEvidence: null,
    explanation: "OSL places the encrypted payload after one Enter and sends only after a second distinct Enter, which may put the account at risk if the service treats assisted placement or sending as automation.",
  },
  {
    mode: "single",
    termsStatus: "May conflict with terms",
    enforcementLikelihood: "Unknown",
    enforcementEvidence: null,
    explanation: "OSL places and sends after one explicit action and fresh checks, which may put the account at risk if the service treats assisted sending as automation.",
  },
] as const;

/**
 * Current first-party Windows surface inventory.
 *
 * `verified` describes the integration boundary, not merely vendor support.
 * Outlook covers both the signed classic executable and the exact reviewed
 * New Outlook Store package. Neither route permits browser fallback.
 */
export const desktopServicePolicies: readonly DesktopServicePolicy[] = [
  policy("discord", "verified", discordSendModeRisk),
  policy("outlook", "verified"),
  policy("proton", "candidate"),
  policy("tuta", "candidate"),
  policy("fastmail", "candidate"),
  policy("zoho", "candidate"),
  policy("slack", "candidate"),
  policy("teams", "candidate"),
  policy("instagram", "packagedWeb"),
  policy("messenger", "browserOnly"),
  policy("x", "packagedWeb"),
  policy("snapchat", "browserOnly"),
  policy("gmail", "browserOnly"),
  policy("yahoo", "browserOnly"),
  policy("aol", "browserOnly"),
  policy("gmx", "browserOnly"),
  policy("maildotcom", "browserOnly"),
  policy("icloud", "browserOnly"),
] as const;

const desktopServicePolicyById = new Map(
  desktopServicePolicies.map((entry) => [entry.id, entry] as const),
);

export function desktopServicePolicy(id: DesktopServiceId): DesktopServicePolicy {
  const entry = desktopServicePolicyById.get(id);
  if (!entry) throw new Error("unknown desktop service");
  return entry;
}

export function requiresNativeDesktopSurface(id: DesktopServiceId): boolean {
  return desktopServicePolicy(id).nativeOnly;
}
