import type { HubIdentitySlot } from "./adapters";
import { inDomTooltipMarkup } from "./in-dom-tooltip";
import {
  escapeHtml,
  homeAppsFromServices,
  type HomeAppCatalogEntry,
  type HomeAppId,
  type LinkedService,
  type NativeApp,
  type NativeAppId,
} from "./services";

type HomeModuleId = "osl-chats" | "osl-mail" | "osl-notes" | "scrub";

export interface HomeLauncherState {
  services: readonly LinkedService[];
  nativeApps: readonly NativeApp[];
  savedNativeApps: ReadonlySet<NativeAppId>;
  selectedOnboardingApps: ReadonlySet<HomeAppId>;
  hasExplicitOnboardingAppSelection: boolean;
  homeTileOrder: readonly string[];
  hiddenHomeTiles: ReadonlySet<string>;
  homeEditMode: boolean;
  appLaunchPendingId: HomeAppId | null;
  hubIdentities: readonly HubIdentitySlot[];
  homeDestinationContent: string;
}

export interface HomeLauncherRenderers {
  homeModuleIcon: (id: HomeModuleId | "activity" | "osl-servers") => string;
  homeAppLogo: (app: HomeAppCatalogEntry) => string;
  homeCommandIcon: (id: "friends" | "notifications" | "settings" | "organize") => string;
  nativeClaimLabel: (status: NativeApp["supportStatus"]) => string;
}

export function homeLauncherBody(state: HomeLauncherState, renderers: HomeLauncherRenderers): string {
  const launchableHomeApps = homeAppsFromServices(state.services).filter((app) => app.visibility === "launch");
  const roadmapHomeApps = launchableHomeApps.filter((app) => app.launchState !== "available");
  const rememberedHomeApps = new Set<HomeAppId>(state.hasExplicitOnboardingAppSelection
    ? state.selectedOnboardingApps
    : [
        ...state.selectedOnboardingApps,
        ...launchableHomeApps.filter((app) => app.linked || state.savedNativeApps.has(app.id as NativeAppId)).map((app) => app.id),
      ]);
  const selectedHomeApps = state.hasExplicitOnboardingAppSelection || rememberedHomeApps.size
    ? launchableHomeApps.filter((app) => app.launchState === "available" && rememberedHomeApps.has(app.id))
    : launchableHomeApps.filter((app) => app.launchState === "available");
  const designSocialOrder = new Map([
    ["Discord", 0],
    ["Telegram", 1],
    ["Signal", 2],
    ["WhatsApp", 3],
    ["Messenger", 4],
    ["X", 5],
  ]);
  const homeApps = [...selectedHomeApps, ...roadmapHomeApps.filter((app) => !selectedHomeApps.some((selected) => selected.id === app.id))]
    .filter((app) => app.displayName !== "Instagram" && app.displayName !== "Tuta")
    .sort((left, right) => (designSocialOrder.get(left.displayName) ?? Number.MAX_SAFE_INTEGER)
      - (designSocialOrder.get(right.displayName) ?? Number.MAX_SAFE_INTEGER));
  const modules = [
    { id: "osl-chats", name: "OSL Chat", available: true },
    { id: "osl-mail", name: "OSL Mail", available: true },
    { id: "osl-notes", name: "OSL Notes", available: false },
    { id: "scrub", name: "Scrub", available: true },
  ] as const;
  const byId = new Map(homeApps.map((app) => [app.id, app]));
  const moduleById = new Map(modules.map((module) => [module.id, module]));
  const defaultIds = [...homeApps.map((app) => app.id), ...modules.map((module) => module.id)];
  const orderedIds = [...state.homeTileOrder.filter((id) => defaultIds.includes(id as HomeAppId)), ...defaultIds.filter((id) => !state.homeTileOrder.includes(id))];
  const renderHomeTile = (id: string, index: number): string => {
    const hidden = state.hiddenHomeTiles.has(id);
    if (hidden && !state.homeEditMode) return "";
    const controls = state.homeEditMode ? `<span class="tile-edit-controls"><button class="tile-remove" type="button" data-tile-toggle="${escapeHtml(id)}" aria-label="${hidden ? "Show" : "Remove"} ${escapeHtml(id)}">${hidden ? "+" : "−"}</button><span class="tile-keyboard-controls"><button type="button" data-tile-move="${escapeHtml(id)}:-1" ${index === 0 ? "disabled" : ""} aria-label="Move before">←</button><button type="button" data-tile-move="${escapeHtml(id)}:1" ${index === orderedIds.length - 1 ? "disabled" : ""} aria-label="Move after">→</button></span></span>` : "";
    const module = moduleById.get(id as typeof modules[number]["id"]);
    if (module) return `<article class="app-tile home-module ${module.available ? "" : "module-unavailable"} ${hidden ? "tile-hidden" : ""}" data-tile-id="${module.id}" draggable="${state.homeEditMode}" data-module-kind="${module.id}"><button class="in-dom-tooltip-anchor" type="button" data-home-module="${module.id}" ${module.available ? "" : "disabled"} aria-label="${escapeHtml(module.available ? module.name : `${module.name}, coming later`)}"><span class="app-logo-plate osl-module-logo" aria-hidden="true">${renderers.homeModuleIcon(module.id)}</span><span class="app-tile-copy"><strong>${module.name}</strong></span>${inDomTooltipMarkup(module.available ? module.name : `${module.name} · Coming later`)}</button>${controls}</article>`;
    const app = byId.get(id as HomeAppId);
    if (!app) return "";
    const appState = app.linked ? "OSL profile ready" : app.launchState === "available" ? "Set up" : app.id === "messenger" ? "Cannot send yet" : "Coming later";
    const pending = state.appLaunchPendingId === app.id;
    const available = app.launchState === "available";
    const disabled = !available || Boolean(state.appLaunchPendingId);
    const claim = state.nativeApps.find((candidate) => candidate.id === app.id as NativeAppId);
    const caption = claim ? renderers.nativeClaimLabel(claim.supportStatus) : app.id === "messenger" ? "Cannot send yet" : "Coming soon";
    const claimTitle = claim ? ` title="${escapeHtml(claim.claimNote)}"` : "";
    const displayName = ({ proton: "Proton", yahoo: "Yahoo", aol: "AOL", icloud: "iCloud" } as Partial<Record<HomeAppId, string>>)[app.id] ?? app.displayName;
    const logo = app.id === "messenger"
      ? `<svg class="company-logo" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="10" fill="#7b61ff"/><path fill="#fff" d="m6.7 14.4 3.8-4.1 2.7 2.2 4.2-4.5-3.8 5.8-2.8-2.2-4.1 2.8Z"/></svg>`
      : renderers.homeAppLogo(app);
    return `<article class="app-tile ${available ? "" : "app-unavailable"} ${hidden ? "tile-hidden" : ""} ${pending ? "pending" : ""}" data-tile-id="${app.id}" draggable="${state.homeEditMode}" data-service-kind="${app.serviceId ?? "none"}" data-launch-state="${app.launchState}" data-claim-status="${claim ? claim.supportStatus : "comingSoon"}" aria-disabled="${available ? "false" : "true"}"><button id="home-app-${app.id}" type="button" ${available ? `data-home-app="${app.id}"` : ""} aria-label="${escapeHtml(`${displayName}, ${pending ? "Opening" : appState}`)}"${claimTitle} ${disabled ? "disabled" : ""}><span class="app-logo-plate">${logo}</span><span class="app-tile-copy"><strong>${escapeHtml(displayName)}</strong>${pending ? "<small>Opening…</small>" : available ? "" : `<small>${escapeHtml(caption)}</small>`}</span></button>${controls}</article>`;
  };
  const socialIds = new Set(homeApps.filter((app) => app.provider === null).map((app) => app.id));
  const emailIds = new Set(homeApps.filter((app) => app.provider !== null).map((app) => app.id));
  const renderedSocialTiles = orderedIds.filter((id) => socialIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const xTile = homeApps.some((app) => app.displayName === "X") ? "" : `<article class="app-tile app-unavailable" data-tile-id="x" data-service-kind="x" data-launch-state="comingSoon" aria-disabled="true"><button type="button" aria-label="X, coming later" disabled><span class="app-logo-plate"><svg class="company-logo" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M18.9 2H22l-6.8 7.8L23.2 22H17l-4.8-6.3L6.7 22H3.6l7.1-8.1L1.1 2h6.3l4.4 5.8L18.9 2Zm-1.1 17.9h1.7L6.5 4H4.7l13.1 15.9Z"/></svg></span><span class="app-tile-copy"><strong>X</strong></span></button></article>`;
  const socialTiles = `${renderedSocialTiles}${xTile}`;
  const emailTiles = orderedIds.filter((id) => emailIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const oslTiles = orderedIds.filter((id) => moduleById.has(id as typeof modules[number]["id"])).map(renderHomeTile).join("");
  const oslSection = oslTiles ? `<section class="home-app-section home-osl-section"><div class="app-grid" aria-label="OSL tools">${oslTiles}</div></section>` : "";
  const activeIdentity = state.hubIdentities.find((identity) => identity.active);
  const profileName = activeIdentity?.label?.trim() || "OSL Profile";
  const profileInitial = profileName.slice(0, 1).toLocaleUpperCase();
  const collapsedControls = `<nav class="home-collapsed-actions" aria-label="Home view controls"><button class="home-section-action in-dom-tooltip-anchor" data-edit-home type="button" aria-label="${state.homeEditMode ? "Finish arranging" : "Customize apps"}">${renderers.homeCommandIcon("organize")}${inDomTooltipMarkup(state.homeEditMode ? "Done" : "Customize apps")}</button><button class="home-command-icon in-dom-tooltip-anchor" data-open-friends type="button" aria-label="Friends">${renderers.homeCommandIcon("friends")}${inDomTooltipMarkup("Friends")}</button></nav>`;
  return `<main id="home-navigation" class="content-viewport home-dashboard ${state.homeEditMode ? "editing" : ""}" aria-label="Home">${collapsedControls}<section class="home-primary"><section class="home-apps"><div class="home-app-groups">${oslSection}${socialTiles ? `<section class="home-app-section"><header><h2>Social</h2></header><div class="app-grid" aria-label="Social apps">${socialTiles}</div></section>` : ""}${emailTiles ? `<section class="home-app-section"><header><h2>Email</h2></header><div class="app-grid" aria-label="Email apps">${emailTiles}</div></section>` : ""}</div></section></section><button class="home-profile-dock in-dom-tooltip-anchor" data-route="settings" data-profile-settings type="button" aria-label="Open your OSL profile"><span aria-hidden="true">${escapeHtml(profileInitial)}</span><strong>${escapeHtml(profileName)}</strong>${inDomTooltipMarkup(profileName)}</button></main>`;
}
