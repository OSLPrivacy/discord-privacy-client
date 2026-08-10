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
  const homeApps = [...selectedHomeApps, ...roadmapHomeApps.filter((app) => !selectedHomeApps.some((selected) => selected.id === app.id))];
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
    return `<article class="app-tile ${available ? "" : "app-unavailable"} ${hidden ? "tile-hidden" : ""} ${pending ? "pending" : ""}" data-tile-id="${app.id}" draggable="${state.homeEditMode}" data-service-kind="${app.serviceId ?? "none"}" data-launch-state="${app.launchState}" data-claim-status="${claim ? claim.supportStatus : "comingSoon"}" aria-disabled="${available ? "false" : "true"}"><button id="home-app-${app.id}" type="button" ${available ? `data-home-app="${app.id}"` : ""} aria-label="${escapeHtml(`${app.displayName}, ${pending ? "Opening" : appState}`)}"${claimTitle} ${disabled ? "disabled" : ""}><span class="app-logo-plate">${renderers.homeAppLogo(app)}</span><span class="app-tile-copy"><strong>${escapeHtml(app.displayName)}</strong>${pending ? "<small>Opening…</small>" : available ? "" : `<small>${escapeHtml(caption)}</small>`}</span></button>${controls}</article>`;
  };
  const socialIds = new Set(homeApps.filter((app) => app.provider === null).map((app) => app.id));
  const emailIds = new Set(homeApps.filter((app) => app.provider !== null).map((app) => app.id));
  const socialTiles = orderedIds.filter((id) => socialIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const emailTiles = orderedIds.filter((id) => emailIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const oslTiles = orderedIds.filter((id) => moduleById.has(id as typeof modules[number]["id"])).map(renderHomeTile).join("");
  const organizeButton = (label: string) => `<button class="home-section-action in-dom-tooltip-anchor" data-edit-home type="button" aria-label="${state.homeEditMode ? "Finish arranging" : `Customize ${label}`}">${renderers.homeCommandIcon("organize")}${inDomTooltipMarkup(state.homeEditMode ? "Done" : `Customize ${label}`)}</button>`;
  const oslSection = oslTiles ? `<section class="home-app-section home-osl-section"><div class="app-grid" aria-label="OSL tools">${oslTiles}</div></section>` : "";
  const activeIdentity = state.hubIdentities.find((identity) => identity.active);
  const profileName = activeIdentity?.label?.trim() || "OSL Profile";
  const profileInitial = profileName.slice(0, 1).toLocaleUpperCase();
  return `<main id="home-navigation" class="content-viewport home-dashboard ${state.homeEditMode ? "editing" : ""}"><section class="home-primary">${state.homeDestinationContent}<section class="home-apps" aria-labelledby="route-heading"><div class="home-app-groups">${oslSection}${socialTiles ? `<section class="home-app-section"><header><h2>Social</h2>${organizeButton("social apps")}</header><div class="app-grid" aria-label="Social apps">${socialTiles}</div></section>` : ""}${emailTiles ? `<section class="home-app-section"><header><h2>Email</h2>${organizeButton("email apps")}</header><div class="app-grid" aria-label="Email apps">${emailTiles}</div></section>` : ""}</div></section></section><button class="home-profile-dock in-dom-tooltip-anchor" data-route="settings" data-profile-settings type="button" aria-label="Open your OSL profile"><span aria-hidden="true">${escapeHtml(profileInitial)}</span><strong>${escapeHtml(profileName)}</strong>${inDomTooltipMarkup(profileName)}</button></main>`;
}
