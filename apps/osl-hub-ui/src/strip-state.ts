// The OSL Strip — pure state → view derivation for the 44px chip bar that
// rides on top of a carrier app. Canon: design-export README, "Screens /
// Views > 4. Strip" and its "Behaviors that must survive reimplementation".
//
// No DOM in this module, so the honesty rules are unit-testable:
//
//   * ROOM HONESTY. When OSL cannot prove which room the message is going to,
//     the timer / once / eye / whitelist / burn controls grey out, every greyed
//     control's tooltip says why, and the carrier composer placeholder warns.
//     Greyed means "OSL can't", never "you can't".
//   * REVEAL IS A TOGGLE. The eye's `revealed` flag is persisted through the
//     decrypt-display policy. A second activation hides it; a lost room still
//     fails closed (that rule is enforced in strip.ts and asserted in
//     strip.test.ts).
//   * FACTS ARE MONOSPACE. Every chip face here is a fact the system asserts,
//     so the DOM layer renders faces in the status style (Consolas, uppercase).

export const NO_ROOM_REASON =
  "Greyed — OSL can't prove which chat this is. Acting on the wrong chat is worse than not acting.";

export const UNPROVEN_COMPOSER_PLACEHOLDER =
  "OSL can't see this box — don't send protected text here";

export const TIMER_HONESTY_LINE =
  "OSL deletes its own copy and stops decrypting. It cannot stop a screenshot.";

export const WHITELIST_MODIFIED_BUILD_NOTE =
  "A modified build can do anything with a message once it is decrypted. OSL can see that the build is modified; it cannot see what it does.";

/** Chip colour vocabulary; strip.css maps each tone to design-token colours. */
export type StripTone =
  | "quiet"     // resting control: 1.5px border, white text
  | "muted"     // informational grey (e.g. FREE plan)
  | "accent"    // cyan — OSL is acting / open popup
  | "safe"      // green — verified, protection on
  | "timer"     // yellow — timers and view-once
  | "warning"   // amber — attention without danger
  | "danger"    // red — burn, protection off, reveal at rest
  | "pro"       // purple — Pro
  | "disabled"; // greyed; ALWAYS carries a tooltip reason

export interface StripAvailability {
  readonly available: boolean;
  /** Why OSL can't, whenever `available` is false. Disabled ≠ unexplained. */
  readonly reason?: string;
}

export interface StripTimerPreset extends StripAvailability {
  readonly label: string;
  /** null = no timer (OFF). */
  readonly seconds: number | null;
}

export interface StripQuickSettingRow extends StripAvailability {
  readonly id: string;
  readonly name: string;
  /** Mono value face shown on the right; cycles on activation. */
  readonly value?: string;
  readonly kind: "cycle" | "toggle" | "action" | "gear";
  readonly on?: boolean;
}

export interface StripWhitelistPerson {
  readonly id: string;
  readonly name: string;
  readonly build: "verified" | "modified";
  /** e.g. "VERIFIED BUILD 0.9.4" / "MODIFIED BUILD". */
  readonly buildLabel: string;
  readonly allowed: boolean;
  /** false → the only offer is "VERIFY FIRST". */
  readonly matched: boolean;
}

export type StripLockState = "on" | "off" | "unreachable";

export interface OslStripState {
  /** OSL can prove which room the message is going to. */
  readonly roomProven: boolean;
  readonly roomLabel: string;
  readonly plan: "free" | "pro";
  readonly planAction: StripAvailability;
  readonly homeAction: StripAvailability;
  readonly burn: StripAvailability;
  readonly whitelist: StripAvailability & {
    /** null roster = OSL has no roster source here (chip stays honest-grey). */
    readonly roster: readonly StripWhitelistPerson[] | null;
  };
  readonly timer: StripAvailability & {
    /** null = no timer set. */
    readonly seconds: number | null;
    readonly presets: readonly StripTimerPreset[];
    /** Optional host-supplied fact line, e.g. "IF SENT NOW · GONE BY <UTC>". */
    readonly factLine?: string;
    /** Optional host-supplied tooltip override. */
    readonly tooltip?: string;
  };
  readonly once: StripAvailability & {
    readonly armed: boolean;
    /** Seconds per open, when the engine supports a duration; else null. */
    readonly seconds: number | null;
  };
  readonly lock: {
    readonly state: StripLockState;
    readonly toggle: StripAvailability;
  };
  readonly reveal: StripAvailability & {
    /** True while the eye's toggle is on and plaintext is being shown. */
    readonly revealed: boolean;
  };
  readonly quickSettings: readonly StripQuickSettingRow[];
  /** Render carrier window controls (min/max/close)? */
  readonly windowControls: boolean;
}

export interface StripChipView {
  readonly face: string;
  readonly tone: StripTone;
  readonly disabled: boolean;
  readonly title: string;
  readonly pressed?: boolean;
}

export interface OslStripView {
  readonly roomProven: boolean;
  readonly home: StripChipView;
  readonly plan: StripChipView;
  readonly quick: StripChipView;
  readonly burn: StripChipView;
  readonly whitelist: StripChipView;
  readonly timer: StripChipView;
  readonly once: StripChipView;
  readonly lock: StripChipView;
  readonly eye: StripChipView;
  /** Non-null when the carrier composer placeholder must warn. */
  readonly composerWarning: string | null;
}

/** "1h" / "1d" / "3d" / "7d" / "2h30m" — the canonical lowercase timer face. */
export function stripTimerFace(seconds: number | null): string {
  if (seconds === null || seconds <= 0) return "OFF";
  const totalMinutes = Math.floor(seconds / 60);
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor((totalMinutes % 1440) / 60);
  const minutes = totalMinutes % 60;
  const face = `${days ? `${days}d` : ""}${hours ? `${hours}h` : ""}${minutes ? `${minutes}m` : ""}`;
  return face === "" ? `${seconds}s` : face;
}

/** "1 day" / "2 hours 30 minutes" — the timer face in words. */
export function stripTimerWords(seconds: number | null): string {
  if (seconds === null || seconds <= 0) return "no timer";
  const totalMinutes = Math.floor(seconds / 60);
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor((totalMinutes % 1440) / 60);
  const minutes = totalMinutes % 60;
  const parts: string[] = [];
  if (days) parts.push(`${days} ${days === 1 ? "day" : "days"}`);
  if (hours) parts.push(`${hours} ${hours === 1 ? "hour" : "hours"}`);
  if (minutes) parts.push(`${minutes} ${minutes === 1 ? "minute" : "minutes"}`);
  return parts.length ? parts.join(" ") : "under a minute";
}

/** Room honesty: an unproven room overrides every per-room availability. */
function roomGated(state: OslStripState, base: StripAvailability): StripAvailability {
  if (!state.roomProven) return { available: false, reason: NO_ROOM_REASON };
  return base;
}

function disabledTitle(availability: StripAvailability, fallback: string): string {
  return availability.reason ?? fallback;
}

export function deriveStripView(state: OslStripState): OslStripView {
  const burn = roomGated(state, state.burn);
  const whitelist = roomGated(state, state.whitelist);
  const timer = roomGated(state, state.timer);
  const once = roomGated(state, state.once);
  const reveal = roomGated(state, state.reveal);

  const roster = state.whitelist.roster;
  const wlAllowed = roster ? roster.filter((person) => person.allowed).length : null;
  const wlFace = !state.roomProven
    ? "NO ROOM"
    : roster
      ? `${wlAllowed}/${roster.length}`
      : "—";

  const timerSet = state.timer.seconds !== null && state.timer.seconds > 0;
  const timerFace = stripTimerFace(state.timer.seconds);
  const timerTooltip = state.timer.tooltip
    ?? (timerSet
      ? `Disappears ${stripTimerWords(state.timer.seconds)} after opening`
      : "No timer — messages stay until someone deletes them");

  const onceFace = state.once.armed
    ? state.once.seconds !== null ? `ONCE ${state.once.seconds}s` : "ONCE"
    : "ONCE OFF";

  const lockState = state.lock.state;
  const lockTitle = lockState === "unreachable"
    ? "OSL can't reach this composer — protected sends are blocked, not downgraded"
    : lockState === "on"
      ? state.lock.toggle.available
        ? "OSL is on this composer · click to turn it off"
        : `OSL is on this composer. ${disabledTitle(state.lock.toggle, "")}`.trim()
      : state.lock.toggle.available
        ? "Click to put OSL on this composer"
        : `OSL is not on this composer. ${disabledTitle(state.lock.toggle, "")}`.trim();

  // Owner ruling: the eye is a toggle, never a press-and-hold gesture.
  const eyeTitle = reveal.available
    ? state.reveal.revealed
      ? "Showing what OSL actually sent — click to restore the cover text"
      : "Click to see the real message"
    : disabledTitle(reveal, NO_ROOM_REASON);

  return {
    roomProven: state.roomProven,
    home: {
      face: "",
      tone: state.homeAction.available ? "quiet" : "disabled",
      disabled: !state.homeAction.available,
      title: state.homeAction.available
        ? "OSL — back to home"
        : disabledTitle(state.homeAction, "OSL home"),
    },
    plan: {
      face: state.plan === "pro" ? "PRO" : "FREE",
      tone: state.planAction.available
        ? state.plan === "pro" ? "pro" : "muted"
        : "disabled",
      disabled: !state.planAction.available,
      title: state.planAction.available
        ? state.plan === "pro"
          ? "Pro · custom enclave URLs, longer relay allowance"
          : "Free OSL — open settings to see what Pro adds"
        : disabledTitle(state.planAction, "Plan"),
    },
    quick: {
      face: "",
      tone: "quiet",
      disabled: false,
      title: "Quick settings",
    },
    burn: {
      face: "",
      tone: burn.available ? "danger" : "disabled",
      disabled: !burn.available,
      title: burn.available
        ? "Burn — choose what OSL destroys"
        : disabledTitle(burn, NO_ROOM_REASON),
    },
    whitelist: {
      face: wlFace,
      tone: !whitelist.available
        ? "disabled"
        : roster === null
          ? "muted"
          : wlAllowed
            ? "accent"
            : "warning",
      disabled: !whitelist.available,
      title: whitelist.available
        ? "Whitelist for this scope only — who can read what you send here"
        : disabledTitle(whitelist, NO_ROOM_REASON),
    },
    timer: {
      face: timerFace,
      tone: !timer.available ? "disabled" : timerSet ? "timer" : "quiet",
      disabled: !timer.available,
      title: timer.available ? timerTooltip : disabledTitle(timer, NO_ROOM_REASON),
    },
    once: {
      face: onceFace,
      tone: !once.available ? "disabled" : state.once.armed ? "timer" : "quiet",
      disabled: !once.available,
      title: once.available
        ? state.once.armed
          ? state.once.seconds !== null
            ? `View once is on · opens for ${state.once.seconds}s, then closes`
            : "View once is on · one open, then OSL closes it for good"
          : "View once is off"
        : disabledTitle(once, NO_ROOM_REASON),
    },
    lock: {
      face: "",
      tone: lockState === "on" ? "safe" : "danger",
      disabled: !state.lock.toggle.available,
      title: lockTitle,
    },
    eye: {
      face: "",
      tone: !reveal.available ? "disabled" : state.reveal.revealed ? "safe" : "danger",
      disabled: !reveal.available,
      title: eyeTitle,
      pressed: state.reveal.revealed,
    },
    composerWarning: state.roomProven ? null : UNPROVEN_COMPOSER_PLACEHOLDER,
  };
}
