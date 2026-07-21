import { invoke as tauriInvoke, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";

export const DESIGN_PREVIEW_ROUTES = [
  "welcome", "create", "restore", "unlock", "recovery", "tutorial", "import", "detected", "install",
  "mullvad", "sending", "passwords", "burnpass", "privacy", "scrub", "decoy",
] as const;

export type DesignPreviewRoute = typeof DESIGN_PREVIEW_ROUTES[number];

const realTauriRuntime = typeof window !== "undefined"
  && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

export const isDesignPreview = typeof window !== "undefined" && !realTauriRuntime;

const previewProKey = "osl-design-preview-pro";
let previewIdentityReady = false;
const previewInstalledNativeApps = new Set(["discord", "signal"]);

export function hasTauriBridge(): boolean {
  return realTauriRuntime || isDesignPreview;
}

export function designPreviewRouteFromHash(): DesignPreviewRoute | null {
  if (!isDesignPreview) return null;
  const candidate = window.location.hash.slice(1);
  return DESIGN_PREVIEW_ROUTES.includes(candidate as DesignPreviewRoute) ? candidate as DesignPreviewRoute : null;
}

export function designPreviewProEnabled(): boolean {
  return isDesignPreview && localStorage.getItem(previewProKey) === "true";
}

export function setDesignPreviewProEnabled(enabled: boolean): void {
  if (isDesignPreview) localStorage.setItem(previewProKey, String(enabled));
}

export function designPreviewNavMarkup(activeRoute: DesignPreviewRoute): string {
  if (!isDesignPreview) return "";
  const index = DESIGN_PREVIEW_ROUTES.indexOf(activeRoute);
  const options = DESIGN_PREVIEW_ROUTES.map((route) => `<option value="${route}" ${route === activeRoute ? "selected" : ""}>${route}</option>`).join("");
  return `<aside class="design-preview-nav" aria-label="Design preview navigation"><strong>Design preview</strong><label><span>Screen</span><select id="design-preview-route">${options}</select></label><div><button id="design-preview-back" type="button" ${index === 0 ? "disabled" : ""}>Back</button><button id="design-preview-next" type="button" ${index === DESIGN_PREVIEW_ROUTES.length - 1 ? "disabled" : ""}>Next</button></div><label class="design-preview-pro"><input id="design-preview-pro" type="checkbox" ${designPreviewProEnabled() ? "checked" : ""}/><span>Pro preview</span></label></aside>`;
}

export function bindDesignPreviewNav(
  activeRoute: DesignPreviewRoute,
  apply: (route: DesignPreviewRoute, proEnabled: boolean) => void,
): void {
  if (!isDesignPreview) return;
  const jump = (next: DesignPreviewRoute): void => {
    if (window.location.hash !== `#${next}`) window.location.hash = next;
    else apply(next, designPreviewProEnabled());
  };
  document.querySelector<HTMLSelectElement>("#design-preview-route")?.addEventListener("change", (event) => {
    jump((event.currentTarget as HTMLSelectElement).value as DesignPreviewRoute);
  });
  const index = DESIGN_PREVIEW_ROUTES.indexOf(activeRoute);
  document.querySelector("#design-preview-back")?.addEventListener("click", () => {
    const previous = DESIGN_PREVIEW_ROUTES[index - 1];
    if (previous) jump(previous);
  });
  document.querySelector("#design-preview-next")?.addEventListener("click", () => {
    const next = DESIGN_PREVIEW_ROUTES[index + 1];
    if (next) jump(next);
  });
  document.querySelector<HTMLInputElement>("#design-preview-pro")?.addEventListener("change", (event) => {
    setDesignPreviewProEnabled((event.currentTarget as HTMLInputElement).checked);
    apply(activeRoute, designPreviewProEnabled());
  });
}

const previewServices = [
  service("discord", "Discord", "DC", 0, [
    { id: "personal", label: "Personal", displayHandle: "designer.demo", state: "demoLinked", provider: null },
    { id: "studio", label: "Studio", displayHandle: "osl.studio", state: "demoLinked", provider: null },
  ]),
  service("telegram", "Telegram", "TG", 1, [
    { id: "private", label: "Private", displayHandle: "+1 ••• ••• 0142", state: "demoLinked", provider: null },
    { id: "work", label: "Work", displayHandle: "@osl_work", state: "demoLinked", provider: null },
  ]),
  service("instagram", "Instagram", "IG", 2), service("snapchat", "Snapchat", "SC", 3),
  service("email", "Email", "EM", 4, [
    { id: "inbox", label: "Main inbox", displayHandle: "design@example.com", state: "demoLinked", provider: "gmail" },
    { id: "private-mail", label: "Private mail", displayHandle: "private@example.com", state: "demoLinked", provider: "proton" },
  ]),
  service("x", "X", "X", 5), service("messenger", "Facebook Messenger", "MS", 6),
  service("signal", "Signal", "SG", 7, [
    { id: "primary", label: "Primary", displayHandle: "+1 ••• ••• 6670", state: "demoLinked", provider: null },
    { id: "family", label: "Family", displayHandle: "+1 ••• ••• 4408", state: "demoLinked", provider: null },
  ]), service("whatsapp", "WhatsApp", "WA", 8),
  service("slack", "Slack", "SL", 9, [], "enterprise", "comingSoon"),
  service("linkedin", "LinkedIn messaging", "LI", 10, [], "enterprise", "comingSoon"),
  service("teams", "Microsoft Teams", "TM", 11, [], "enterprise", "comingSoon"),
];

function service(
  id: string,
  displayName: string,
  sidebarGlyph: string,
  sidebarOrder: number,
  accounts: unknown[] = [],
  category: "consumer" | "enterprise" = "consumer",
  launchState: "available" | "comingSoon" = "available",
): Record<string, unknown> {
  return { id, displayName, sidebarGlyph, sidebarOrder, category, launchState, supportsNativePreview: true, supportsProtectedPreview: true, accounts };
}

function previewReadiness(ready = previewIdentityReady): Record<string, unknown> {
  return {
    originalCoreLinked: true,
    identityLoaded: ready,
    keyserverInitialised: ready,
    cloudRegistrationState: ready ? "registered" : "notAttempted",
    groupSenderKeysEnabled: ready,
    remoteServiceHasNativeAccess: ready,
    bootstrapAttempted: true,
    passwordGateRequired: false,
    unlocked: ready,
    activeOslUserId: ready ? "osl_preview_designer" : null,
    bootstrapStatus: ready ? "ready" : "setupRequired",
  };
}

function passwordRoleStatus(): Record<string, boolean> {
  return { mainPasswordSet: true, stealthPasswordSet: false, burnPasswordSet: false, unlocked: true, stealthActionWired: true, burnActionWired: true };
}

function previewLicense(): Record<string, unknown> {
  return designPreviewProEnabled()
    ? { access: "pro", status: "ACTIVE", currentPeriodEnd: 2_000_000_000, lastValidatedAt: 1_900_000_000 }
    : { access: "free", status: "UNCONFIGURED", currentPeriodEnd: null, lastValidatedAt: null };
}

async function designPreviewInvoke(command: string, args: InvokeArgs = {}): Promise<unknown> {
  const data = typeof args === "object" && args !== null && !Array.isArray(args)
    ? args as Record<string, unknown>
    : {};
  switch (command) {
    case "get_core_readiness": return previewReadiness();
    case "get_onboarding_preferences": return { onboardingComplete: false, sendMode: "clipboard", placementMode: "atomic", showPlaintextPreview: true, acknowledgeExperimentalSendRisk: false };
    case "save_onboarding_preferences": return data.preferences;
    case "create_hub_osl_identity":
    case "import_hub_osl_identity_phrase":
      return { userId: "osl_preview_designer", identityRecoveryPhrase: command.startsWith("create") ? "amber birch canyon drift ember fern harbor ivory juniper kindle lunar meadow" : null, storageMethod: "design-preview", passwordSetupRequired: true };
    case "setup_hub_main_password":
      previewIdentityReady = true;
      return { passwordRecoveryPhrase: "canvas copper ember harbor iris maple orbit pebble quiet river silver willow", encryptedStateReloadComplete: true, encryptedStateReloadIssueCount: 0, readiness: { accessState: "ready", identityLoaded: true, mainPasswordSet: true, unlocked: true, serviceNeutralIdentitySupported: true, canCreateIdentity: false, canImportIdentityPhrase: false, passwordAttemptsUsed: 0, passwordLockoutSecondsRemaining: 0 } };
    case "unlock_hub_password_gate":
      previewIdentityReady = true;
      return { outcome: "unlocked", lockoutSecondsRemaining: 0, attemptsUsed: 0, readiness: previewReadiness(true), burn: null };
    case "get_hub_password_role_status":
    case "set_hub_stealth_password":
    case "set_hub_burn_password":
    case "remove_hub_stealth_password":
    case "remove_hub_burn_password": return passwordRoleStatus();
    case "get_hub_license_state":
    case "validate_hub_activation_code": return previewLicense();
    case "clear_hub_activation_code": return { access: "free", status: "UNCONFIGURED", currentPeriodEnd: null, lastValidatedAt: null };
    case "list_linked_services": return structuredClone(previewServices);
    case "list_native_apps": return [
      { id: "discord", displayName: "Discord", availability: previewInstalledNativeApps.has("discord") ? "installed" : "installable", isolatedProfileAvailable: true, supportsOverlay: true },
      { id: "telegram", displayName: "Telegram", availability: previewInstalledNativeApps.has("telegram") ? "installed" : "installable", isolatedProfileAvailable: true, supportsOverlay: false },
      { id: "signal", displayName: "Signal", availability: previewInstalledNativeApps.has("signal") ? "installed" : "installable", isolatedProfileAvailable: true, supportsOverlay: true },
      { id: "whatsapp", displayName: "WhatsApp", availability: previewInstalledNativeApps.has("whatsapp") ? "installed" : "installable", isolatedProfileAvailable: false, supportsOverlay: false },
    ];
    case "install_native_app":
      previewInstalledNativeApps.add(String(data.appId));
      return { id: data.appId, started: true, packageId: `Preview.${String(data.appId)}` };
    case "get_mullvad_status": return { availability: "installed" };
    case "get_vpn_connection_status": return { connected: false };
    case "install_mullvad":
    case "open_mullvad": return { started: true };
    case "list_browser_imports": return [
      { id: "chrome", displayName: "Google Chrome", installed: true },
      { id: "edge", displayName: "Microsoft Edge", installed: true },
      { id: "firefox", displayName: "Firefox", installed: true },
    ];
    case "open_browser_import": return { id: data.browserId, opened: true };
    case "begin_browser_account_import": return { preferredSource: "chrome", detectedSources: ["chrome", "edge", "firefox"], opened: true, mode: "firefoxMigrationWizard", manualExportRequired: false };
    case "get_firefox_status": return { availability: "installed" };
    case "install_firefox": return { started: true, packageId: "Mozilla.Firefox" };
    case "launch_firefox_service": return { serviceId: data.serviceId, started: true };
    case "create_service_account": return { id: "preview-account", label: data.label ?? "Personal", displayHandle: "preview@example.com", state: "demoLinked", provider: data.provider ?? null };
    case "open_service_host": return { serviceId: data.serviceId, accountId: data.accountId, generation: 1 };
    case "remove_service_account": return { serviceId: data.serviceId, accountId: data.accountId, profileExisted: true, cleanupPending: false, registryRemoved: true };
    case "close_service_host": return null;
    case "host_native_app_window": return { id: data.appId, status: "hosted", reason: "none", mode: "ownedBorderless" };
    case "resize_native_app_window": return { id: "discord", status: "resized", reason: "none", mode: "ownedBorderless" };
    case "focus_native_app_window": return { id: "discord", status: "focused", reason: "none", mode: "ownedBorderless" };
    case "detach_native_app_window": return { id: "discord", status: "detached", reason: "none", mode: "ownedBorderless" };
    case "set_hub_screenshot_protection":
    case "set_hub_notifications_enabled": return true;
    case "export_hub_friend_code": return { friendCode: "OSLFR1.PREVIEWFRIENDCODE1234", oslUserId: "osl_preview_designer", safetyNumber: "1234 5678 9012" };
    case "list_hub_people": return [];
    case "list_hub_app_notifications": return [];
    case "list_hub_identities": return [{ slotId: "preview01", label: "Design preview", oslUserId: "osl_preview_designer", safetyNumber: "1234 5678 9012", active: true }];
    case "get_mass_cleanup_capabilities": return { version: 1, services: [] };
    case "check_hub_for_updates": return { status: "up_to_date", current: "0.1.0" };
    default:
      console.info(`[design preview] harmless mock for unknown invoke: ${command}`);
      return null;
  }
}

export async function invoke<T>(command: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  if (!isDesignPreview) {
    if (options !== undefined) return tauriInvoke<T>(command, args, options);
    if (args !== undefined) return tauriInvoke<T>(command, args);
    return tauriInvoke<T>(command);
  }
  return await designPreviewInvoke(command, args) as T;
}
