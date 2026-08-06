import "./onboarding-stealth.css";
import { BURN_PASSWORD_CONFIRMATION } from "./onboarding-password-role";

export type OnboardingPasswordRole = "stealth" | "burn";

type PasswordRoleContentOptions = {
  role: OnboardingPasswordRole;
  configured: boolean | undefined;
  passwordEyeIcon: () => string;
  statusTag: (label: string) => string;
};

/**
 * 2026-08-06 restyle. Both screens are the same screen. They differ in six
 * things and nothing else: the title, the line under the picture, the badge's
 * colour and glyph, whether there is a typed confirmation, and whether the
 * button's hover is the usual cyan or the destructive red.
 *
 * Writing them as one function is not tidiness. They ask for the same three
 * passwords in the same three boxes, and when they were separate they had
 * already drifted -- different button shapes, different escape-hatch wording --
 * on the one pair of screens where a person has to be certain which of the two
 * they are looking at.
 */
interface RoleShape {
  readonly title: string;
  readonly quiet: string;
  /** The picture's ending: what the badge says happened. */
  readonly badge: string;
  readonly badgeLabel: string;
  readonly newLabel: string;
  readonly next: string;
  /** Only burn has one, and typing it is what makes the button pressable. */
  readonly confirmation: boolean;
  readonly destructive: boolean;
}

const SHAPES: Record<OnboardingPasswordRole, RoleShape> = {
  stealth: {
    title: "Stealth password",
    quiet: "Opens an empty workspace",
    // A struck-through eye: the workspace is not hidden, it is simply empty.
    badge: `<circle class="st-badge-disc" cx="86" cy="8" r="7"/><path class="st-badge-mark" d="M82.4 8 c1.6 -2 5.6 -2 7.2 0 c-1.6 2 -5.6 2 -7.2 0 Z"/><path class="st-badge-mark" d="M83 5 L89 11"/>`,
    badgeLabel: "a workspace empties of its messages when stealth mode turns on",
    newLabel: "New stealth password",
    next: "burnpass",
    confirmation: false,
    destructive: false,
  },
  burn: {
    title: "Burn password",
    quiet: "Entered at sign in, it permanently erases OSL data from this device. No recovery",
    // The same window empties, but this time nothing comes back.
    badge: `<circle class="st-badge-disc st-badge-danger" cx="86" cy="8" r="7"/><path class="st-badge-mark st-badge-cross" d="M83.2 5.2 L88.8 10.8 M88.8 5.2 L83.2 10.8"/>`,
    badgeLabel: "a workspace is erased when the burn password is entered",
    newLabel: "New burn password",
    next: "mullvad",
    confirmation: true,
    destructive: true,
  },
};

/**
 * The workspace empties. What the badge is decides whether that is temporary or
 * permanent -- which is the whole difference between these two passwords, and
 * the reason they get one picture with two endings.
 */
function roleArt(shape: RoleShape): string {
  return `<svg class="stealth-art" width="150" height="66" viewBox="0 0 100 44" fill="none" role="img" aria-label="${shape.badgeLabel}">
    <rect class="st-ink" x="14" y="6" width="72" height="34" rx="3.5"/>
    <path class="st-bar" d="M14 14 H86"/>
    <circle class="st-dot" cx="20" cy="10" r="1.4"/>
    <g class="st-content">
      <rect class="st-bubble" x="20" y="19" width="26" height="7" rx="2.5"/>
      <rect class="st-bubble" x="54" y="29" width="26" height="7" rx="2.5"/>
    </g>
    <g class="st-badge">${shape.badge}</g>
  </svg>`;
}

export function onboardingPasswordRoleContent({ role, configured, passwordEyeIcon, statusTag }: PasswordRoleContentOptions): string {
  const shape = SHAPES[role];
  const detail = role === "stealth"
    ? "Opens an empty workspace without loading your private data. It does not hide that OSL is installed. OSL only marks its folder hidden, and anyone viewing hidden files can still find it."
    : "Erases OSL data from this device when entered at sign in.";

  if (configured) {
    return `<h1 id="route-heading" tabindex="-1">${shape.title}</h1><div class="password-role-ready">${statusTag("Set")}<p>${detail}</p></div><div class="setup-footer onboarding-actions"><button class="button primary" data-password-role-next="${shape.next}" type="button">Continue</button></div>`;
  }

  const formId = `setup-${role}-form`;
  const field = (suffix: string, label: string, autocomplete: string, ariaLabel: string) =>
    `<label for="setup-${role}-${suffix}">${label}</label><div class="password-input-row"><input id="setup-${role}-${suffix}" name="${suffix}" type="password" minlength="6" maxlength="128" autocomplete="${autocomplete}" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-${suffix}" aria-label="${ariaLabel}">${passwordEyeIcon()}</button></div>`;

  // t15-b4 gates the burn password behind a typed confirmation, and
  // canSetOnboardingPasswordRole() REQUIRES it. Without this input the burn
  // password could never be set at all -- the validator would refuse a value
  // the form gave no way to enter.
  const confirmation = shape.confirmation
    ? `<label for="setup-burn-confirmation">Type ${BURN_PASSWORD_CONFIRMATION} to enable it</label><input class="stealth-confirm-input" id="setup-burn-confirmation" name="burnConfirmation" type="text" autocomplete="off" autocapitalize="none" spellcheck="false" placeholder="${BURN_PASSWORD_CONFIRMATION}" required/>`
    : "";

  // The one thing the picture cannot say, and only stealth has one: OSL is
  // still visibly installed. It sits with the control that arms the mode rather
  // than in a block above the fold that gets scrolled past.
  const caveat = role === "stealth"
    ? `<small class="stealth-caveat">This does not hide that OSL is installed. OSL only marks its folder hidden, and anyone viewing hidden files can still find it.</small>`
    : "";

  return `<section class="stealth-screen${shape.destructive ? " stealth-destructive" : ""}" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="stealth-title">${shape.title}</h1>
    ${roleArt(shape)}
    <p class="stealth-quiet">${shape.quiet}</p>
    <form id="${formId}" class="password-form onboarding-role-form stealth-form" data-onboarding-password-role="${role}" data-onboarding-password-next="${shape.next}" novalidate>
      ${field("current", "Current password", "current-password", "Show current password")}
      ${field("alternate", shape.newLabel, "new-password", "Show new password")}
      ${field("confirm", "Confirm", "new-password", "Show password confirmation")}
      ${confirmation}
      <p class="unlock-error" data-onboarding-role-error role="alert"></p>
      ${caveat}
      <button class="stealth-submit" type="submit" form="${formId}" data-onboarding-role-submit disabled><span>Set password</span></button>
    </form>
    <div class="setup-footer onboarding-actions stealth-links"><button class="text-button onboarding-role-skip" type="button" data-skip-onboarding-password-role="${shape.next}">Skip</button></div>
  </section>`;
}
