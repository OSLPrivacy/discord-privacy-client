/**
 * TASK 6872 retained redesign contract.
 *
 * This renderer deliberately accepts only subsystem-origin observations.  A
 * missing observation is rendered as unavailable; it can never become a
 * synthetic success merely because a visual capture needs populated content.
 */

export type RetainedDisposition =
  | "retained"
  | "deleted-d1"
  | "deleted-d4"
  | "chats-strip-follow-up";

export type RetainedTheme = "dark" | "high-contrast";

export interface RetainedPage {
  readonly page_id: string;
  readonly source_image: {
    readonly path: string;
    readonly commit: string;
    readonly blob: string;
    readonly width: number;
    readonly height: number;
  };
  readonly route: string;
  readonly state: string;
  readonly disposition: RetainedDisposition;
  readonly required_windows_widths: readonly number[];
  readonly capability_feed: string | null;
}

export interface CapabilityObservation {
  readonly origin: "subsystem";
  readonly status: "available" | "busy" | "failed" | "unavailable";
  readonly detail: string;
}

export interface CapabilityContract {
  readonly source: string | null;
  readonly symbol: string | null;
  readonly unavailableCopy: string;
}

export const retainedCapabilityContracts = {
  "core-readiness": {
    source: "src/core.ts",
    symbol: "loadCoreIntegration",
    unavailableCopy: "OSL core status is unavailable on this device.",
  },
  "native-app-detection": {
    source: "src/services.ts",
    symbol: "loadNativeApps",
    unavailableCopy: "Installed-app detection is unavailable.",
  },
  "native-app-installer": {
    source: "src/services.ts",
    symbol: "installNativeApp",
    unavailableCopy: "Windows installation is unavailable; no install was claimed.",
  },
  "mullvad-host": {
    source: "src/services.ts",
    symbol: "loadMullvadStatus",
    unavailableCopy: "Mullvad status is unavailable; OSL does not claim a connection.",
  },
  "friend-authority": {
    source: "src/adapters.ts",
    symbol: "listHubPeople",
    unavailableCopy: "Friend state could not be read from the local authority.",
  },
  notifications: {
    source: "src/adapters.ts",
    symbol: "loadAppNotifications",
    unavailableCopy: "Notification state is unavailable.",
  },
  "recovery-kit": {
    source: "src/adapters.ts",
    symbol: "viewHubRecoveryPhrase",
    unavailableCopy: "Recovery material is unavailable and no words are displayed.",
  },
  "scrub-engine": {
    source: "src/mass-cleanup.ts",
    symbol: "loadMassCleanupCapabilities",
    unavailableCopy: "Cleanup capability is unavailable; nothing was reported deleted.",
  },
  "media-store": {
    source: "src/adapters.ts",
    symbol: "prepareHubAttachment",
    unavailableCopy: "Media storage is unavailable; nothing was uploaded.",
  },
  entitlement: {
    source: "src/core.ts",
    symbol: "loadHubLicenseState",
    unavailableCopy: "OSL Pro validation is unavailable.",
  },
  "update-service": {
    source: "src/updates.ts",
    symbol: "checkHubForUpdates",
    unavailableCopy: "Update status is unavailable.",
  },
  "signal-qa": {
    source: "src/signal-qa-ipc.ts",
    symbol: "getSignalProtectedSendReadiness",
    unavailableCopy: "Signal inspection is unavailable.",
  },
  "call-engine": {
    source: null,
    symbol: null,
    unavailableCopy: "Calls are unavailable in this build; no connection is implied.",
  },
  "profile-posts": {
    source: null,
    symbol: null,
    unavailableCopy: "Post publishing is unavailable in this build.",
  },
  "profile-stories": {
    source: null,
    symbol: null,
    unavailableCopy: "Stories are unavailable in this build; no views are counted.",
  },
} as const satisfies Readonly<Record<string, CapabilityContract>>;

export type RetainedCapabilityFeed = keyof typeof retainedCapabilityContracts;

export const retainedSurfaceFamilies = [
  "shell",
  "home",
  "friends",
  "media",
  "calls",
  "settings",
  "privacy",
  "onboarding",
  "installer",
  "recovery",
  "story",
  "post",
  "modal",
  "empty",
  "error",
] as const;

export interface RetainedSurfaceModel {
  readonly pageId: string;
  readonly routeKey: string;
  readonly title: string;
  readonly family: typeof retainedSurfaceFamilies[number];
  readonly state: string;
  readonly feed: RetainedCapabilityFeed;
  readonly observation: CapabilityObservation;
  readonly actionEnabled: boolean;
  readonly statusRole: "status" | "alert";
}

export interface AccessibilityNode {
  readonly role: string;
  readonly name: string;
  readonly disabled?: boolean;
  readonly live?: "polite" | "assertive";
}

export interface RetainedCapture {
  readonly capture_id: string;
  readonly page_id: string;
  readonly route_state: string;
  readonly width: number;
  readonly height: number;
  readonly theme: RetainedTheme;
  readonly source_blob: string;
  readonly visual: {
    readonly renderer: "retained-redesign-6872";
    readonly tokens: readonly ["--bg", "--panel", "--text", "--brand", "--success", "--danger"];
    readonly paint_commands: readonly string[];
    readonly html: string;
  };
  readonly accessibility: {
    readonly format: "deterministic-ax-v1";
    readonly nodes: readonly AccessibilityNode[];
  };
}

const unavailableObservation = (feed: RetainedCapabilityFeed): CapabilityObservation => ({
  origin: "subsystem",
  status: "unavailable",
  detail: retainedCapabilityContracts[feed].unavailableCopy,
});

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function titleFromState(state: string): string {
  return state
    .split(/[-_/]+/u)
    .filter(Boolean)
    .map((word) => word[0]?.toUpperCase() + word.slice(1))
    .join(" ");
}

function familyFor(page: RetainedPage): RetainedSurfaceModel["family"] {
  if (/empty/u.test(page.state)) return "empty";
  if (/error|failed|unavailable|problem|refus/u.test(page.state)) return "error";
  if (page.route === "/home" || page.route.startsWith("/home/")) return "home";
  if (page.route === "/friends") return "friends";
  if (page.route.startsWith("/media")) return "media";
  if (page.route === "/calls") return "calls";
  if (page.route.startsWith("/settings")) return "settings";
  if (page.route.startsWith("/privacy")) return "privacy";
  if (page.route === "/onboarding") return /install/u.test(page.state) ? "installer" : "onboarding";
  if (page.route === "/installer") return "installer";
  if (page.route === "/recovery") return "recovery";
  if (page.route === "/story") return "story";
  if (page.route === "/post") return "post";
  if (page.route === "/modal") return "modal";
  return "shell";
}

export function routeStateKey(page: Pick<RetainedPage, "route" | "state">): string {
  return `${page.route}#${page.state}`;
}

export function retainedRouteRegistry(pages: readonly RetainedPage[]): ReadonlySet<string> {
  return new Set(pages.filter((page) => page.disposition === "retained").map(routeStateKey));
}

export function retainedSurfaceModel(
  page: RetainedPage,
  observations: Readonly<Partial<Record<RetainedCapabilityFeed, CapabilityObservation>>> = {},
): RetainedSurfaceModel {
  if (page.disposition !== "retained") throw new Error(`page is not retained: ${page.page_id}`);
  const feed = page.capability_feed as RetainedCapabilityFeed;
  if (!(feed in retainedCapabilityContracts)) throw new Error(`unknown capability feed for ${page.page_id}: ${feed}`);
  const supplied = observations[feed];
  const observation = supplied?.origin === "subsystem" ? supplied : unavailableObservation(feed);
  const isError = /error|failed|unavailable|problem|refus/u.test(page.state) || observation.status === "failed";
  return {
    pageId: page.page_id,
    routeKey: routeStateKey(page),
    title: titleFromState(page.state),
    family: familyFor(page),
    state: page.state,
    feed,
    observation,
    actionEnabled: observation.status === "available",
    statusRole: isError ? "alert" : "status",
  };
}

export function retainedAccessibilityTree(model: RetainedSurfaceModel): readonly AccessibilityNode[] {
  const modal = model.family === "modal";
  return [
    { role: "navigation", name: "OSL navigation" },
    { role: modal ? "dialog" : "main", name: model.title },
    { role: "heading", name: model.title },
    {
      role: model.statusRole,
      name: model.observation.detail,
      live: model.statusRole === "alert" ? "assertive" : "polite",
    },
    { role: "button", name: `Continue from ${model.title}`, disabled: !model.actionEnabled },
  ];
}

function retainedFamilyMarkup(model: RetainedSurfaceModel): string {
  const disabled = 'disabled aria-disabled="true"';
  switch (model.family) {
    case "home":
      return '<section aria-labelledby="retained-overview"><h2 id="retained-overview">Protection overview</h2><ul class="retained-grid"><li>Connected services</li><li>Recent activity</li><li>Privacy status</li></ul></section>';
    case "friends":
      return '<section aria-labelledby="retained-friends"><h2 id="retained-friends">Friends</h2><label for="retained-friend-search">Find a friend</label><input id="retained-friend-search" autocomplete="off"><ul aria-label="Friend results"><li>No friend records are inferred while the authority is unavailable.</li></ul></section>';
    case "media":
      return `<section aria-labelledby="retained-media"><h2 id="retained-media">Media</h2><button type="button" ${disabled}>Choose media</button><p>No local file has been selected or uploaded.</p></section>`;
    case "calls":
      return `<section aria-labelledby="retained-calls"><h2 id="retained-calls">Calls</h2><button type="button" ${disabled}>Start a call</button><p>OSL does not claim a voice or video connection.</p></section>`;
    case "settings":
      return `<form><fieldset ${disabled}><legend>Settings</legend><label><input type="checkbox" disabled> Start with Windows</label><label><input type="checkbox" disabled> Show notifications</label></fieldset></form>`;
    case "privacy":
      return `<form><fieldset ${disabled}><legend>Privacy</legend><label><input type="checkbox" disabled> Findable by username</label><label><input type="checkbox" disabled> Allow friend requests</label><label><input type="checkbox" disabled> Profile viewable</label></fieldset></form>`;
    case "onboarding":
      return '<section aria-labelledby="retained-setup"><p role="status">Set up your apps</p><h2 id="retained-setup">Device setup</h2><progress value="1" max="4">Step 1 of 4</progress></section>';
    case "installer":
      return `<section aria-labelledby="retained-installer"><h2 id="retained-installer">Set up your apps</h2><ul class="retained-apps">${["Signal", "Discord", "Telegram", "WhatsApp"].map((name) => `<li><strong>${name}</strong><span>NOT DETECTED</span><button type="button" ${disabled}>Install ${name}</button></li>`).join("")}</ul></section>`;
    case "recovery":
      return `<section aria-labelledby="retained-recovery"><h2 id="retained-recovery">Recovery</h2><fieldset ${disabled}><legend>Recovery phrase</legend>${Array.from({ length: 12 }, (_, index) => `<label>${index + 1}<input type="password" disabled autocomplete="off"></label>`).join("")}</fieldset></section>`;
    case "story":
      return `<section aria-labelledby="retained-story"><h2 id="retained-story">Stories</h2><progress value="0" max="1">No story loaded</progress><button type="button" ${disabled}>Create story</button><p>No view or like count is recorded.</p></section>`;
    case "post":
      return `<section aria-labelledby="retained-post"><h2 id="retained-post">Posts</h2><article><h3>No post loaded</h3><p>Post state is not synthesized for this capture.</p></article><button type="button" ${disabled}>Create post</button></section>`;
    case "modal":
      return `<section role="dialog" aria-modal="true" aria-labelledby="retained-dialog"><h2 id="retained-dialog">Confirmation</h2><p>No action has been completed.</p><button type="button" ${disabled}>Confirm</button></section>`;
    case "empty":
      return '<section aria-labelledby="retained-empty"><h2 id="retained-empty">Nothing here yet</h2><p>The source returned no records.</p></section>';
    case "error":
      return '<section aria-labelledby="retained-error"><h2 id="retained-error">Could not load this page</h2><p role="alert">No successful result is being inferred.</p></section>';
    case "shell":
      return '<section aria-labelledby="retained-shell-heading"><h2 id="retained-shell-heading">OSL</h2><p>The retained Windows shell is ready to show verified subsystem state.</p></section>';
  }
}

export function retainedSurfaceHtml(model: RetainedSurfaceModel, theme: RetainedTheme): string {
  const statusLive = model.statusRole === "alert" ? "assertive" : "polite";
  const disabled = model.actionEnabled ? "" : " disabled aria-disabled=\"true\"";
  return `<div class="retained-shell" data-retained-theme="${theme}" data-page-id="${escapeHtml(model.pageId)}"><a class="retained-skip" href="#retained-main">Skip to content</a><nav aria-label="OSL navigation"><button type="button" data-retained-route="/home">OSL</button><span aria-current="page">${escapeHtml(model.family)}</span></nav><main id="retained-main" role="main" tabindex="-1"><p class="retained-kicker">${escapeHtml(model.family)}</p><h1>${escapeHtml(model.title)}</h1>${retainedFamilyMarkup(model)}<section class="retained-state" data-route-state="${escapeHtml(model.routeKey)}"><p role="${model.statusRole}" aria-live="${statusLive}" data-capability-feed="${model.feed}" data-capability-origin="${model.observation.origin}" data-capability-status="${model.observation.status}">${escapeHtml(model.observation.detail)}</p><button type="button" data-retained-action="continue"${disabled}>Continue from ${escapeHtml(model.title)}</button></section></main></div>`;
}

export function captureRetainedSurface(
  page: RetainedPage,
  width: number,
  theme: RetainedTheme,
  observations: Readonly<Partial<Record<RetainedCapabilityFeed, CapabilityObservation>>> = {},
): RetainedCapture {
  const model = retainedSurfaceModel(page, observations);
  return {
    capture_id: `${page.page_id}@${width}:${theme}`,
    page_id: page.page_id,
    route_state: model.routeKey,
    width,
    height: page.source_image.height,
    theme,
    source_blob: page.source_image.blob,
    visual: {
      renderer: "retained-redesign-6872",
      tokens: ["--bg", "--panel", "--text", "--brand", "--success", "--danger"],
      paint_commands: [
        `fill:var(--bg):0,0,${width},${page.source_image.height}`,
        `panel:var(--panel):${Math.min(240, Math.floor(width / 4))}`,
        `focus:var(--brand):${model.routeKey}`,
        `status:${model.observation.status}:${model.feed}`,
      ],
      html: retainedSurfaceHtml(model, theme),
    },
    accessibility: {
      format: "deterministic-ax-v1",
      nodes: retainedAccessibilityTree(model),
    },
  };
}
