/**
 * Product classification for Windows desktop surfaces.
 *
 * This file intentionally does not launch anything. A service is promoted to
 * `verified` only after the Rust boundary has a fixed executable/package
 * identity and an exact publisher allowlist. `candidate` services must fail
 * closed instead of silently opening their website.
 */
export type DesktopServiceId =
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
}

const policy = (
  id: DesktopServiceId,
  surface: WindowsDesktopSurface,
): DesktopServicePolicy => ({
  id,
  surface,
  nativeOnly: surface === "verified" || surface === "candidate",
  separateProfileAvailable: false,
});

/**
 * Current first-party Windows surface inventory.
 *
 * `verified` describes the integration boundary, not merely vendor support.
 * Outlook covers both the signed classic executable and the exact reviewed
 * New Outlook Store package. Neither route permits browser fallback.
 */
export const desktopServicePolicies: readonly DesktopServicePolicy[] = [
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
