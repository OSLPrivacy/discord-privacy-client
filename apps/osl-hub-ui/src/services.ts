import { invoke } from "./dev-preview";
import { isTauriRuntime } from "./preferences";

export type ServiceId = "discord" | "telegram" | "instagram" | "snapchat" | "email" | "x" | "slack" | "linkedin" | "teams" | "messenger" | "signal" | "whatsapp";
export type ConnectionState = "demoLinked" | "notLinked";
export type EmailProvider = "gmail" | "outlook" | "proton" | "tuta" | "fastmail" | "yahoo" | "zoho" | "aol" | "gmx" | "maildotcom";
export type ServiceCategory = "consumer" | "enterprise";
export type LaunchState = "available" | "comingSoon";
export type OfferedEmailProvider = "gmail" | "outlook" | "proton" | "yahoo" | "aol" | "gmx" | "maildotcom";
export type HomeAppId = Exclude<ServiceId, "email" | "slack" | "linkedin" | "teams"> | OfferedEmailProvider | "slack" | "linkedin";
export type HomeAppVisibility = "launch" | "later";
export type HomeAppSection = "social" | "email" | "later";
export type NativeAppId = "discord" | "telegram" | "signal" | "whatsapp";
export type BrowserImportId = "chrome" | "edge" | "firefox" | "brave" | "opera" | "vivaldi";

export interface BrowserImportStatus {
  id: BrowserImportId;
  displayName: string;
  installed: boolean;
}

export interface BrowserAccountImportAction {
  preferredSource: BrowserImportId;
  detectedSources: BrowserImportId[];
  opened: true;
  mode: "firefoxMigrationWizard";
  manualExportRequired: boolean;
}

export interface NativeApp {
  id: NativeAppId;
  displayName: string;
  availability: "installed" | "installable" | "unavailable";
  isolatedProfileAvailable: boolean;
  supportsOverlay: boolean;
}

export interface NativeAppAction {
  id: NativeAppId;
  started: true;
  packageId?: string;
}

export interface MullvadStatus {
  availability: "installed" | "installable" | "unavailable";
}

export interface MullvadAction {
  started: true;
}

export interface NativeWindowHostAction {
  id: NativeAppId;
  status: "hosted" | "resized" | "focused" | "detached" | "unsupported" | "failed";
  reason: "none" | "platformUnsupported" | "secondaryInstanceUnverified" | "appNotInstalled" | "profileUnavailable" | "launchFailed" | "windowNotFound" | "windowIdentityChanged" | "ownerWindowUnavailable" | "hostWindowUnavailable" | "windowOperationRejected" | "notHosted";
  mode: "none" | "ownedBorderless";
}

export interface FirefoxStatus {
  availability: "installed" | "installable" | "unavailable";
}

export interface LinkedAccount {
  id: string;
  label: string;
  displayHandle: string;
  state: ConnectionState;
  provider: EmailProvider | null;
}

export interface LinkedService {
  id: ServiceId;
  displayName: string;
  sidebarGlyph: string;
  sidebarOrder: number;
  category: ServiceCategory;
  launchState: LaunchState;
  supportsNativePreview: boolean;
  supportsProtectedPreview: boolean;
  accounts: LinkedAccount[];
}

/** Non-sensitive catalog data for the fixed OSL Privacy home grid. */
export interface HomeAppCatalogEntry {
  id: HomeAppId;
  displayName: string;
  serviceId: ServiceId | null;
  provider: OfferedEmailProvider | null;
  visibility: HomeAppVisibility;
  section: HomeAppSection;
  launchState: LaunchState;
  linked: boolean;
  accountCount: number;
  setupEligible: boolean;
}

export interface EmbeddedServiceHost {
  serviceId: ServiceId;
  accountId: string;
  generation: number;
}

export interface EmbeddedServiceSetup {
  account: LinkedAccount;
  host: EmbeddedServiceHost;
}

export interface EmbeddedServiceAccountRemoval {
  serviceId: ServiceId;
  accountId: string;
  profileExisted: boolean;
  cleanupPending: boolean;
  registryRemoved: boolean;
}

export interface NotificationIntegrationEligibility {
  configuredAppCount: number;
  eligible: boolean;
}

const serviceIds: readonly ServiceId[] = ["discord", "telegram", "instagram", "snapchat", "email", "x", "slack", "linkedin", "teams", "messenger", "signal", "whatsapp"];
const connectionStates: readonly ConnectionState[] = ["demoLinked", "notLinked"];
const emailProviders: readonly EmailProvider[] = ["gmail", "outlook", "proton", "tuta", "fastmail", "yahoo", "zoho", "aol", "gmx", "maildotcom"];
const maxAccountsPerService = 10;
const nativeAppIds: readonly NativeAppId[] = ["discord", "telegram", "signal", "whatsapp"];
const browserImportIds: readonly BrowserImportId[] = ["chrome", "edge", "firefox", "brave", "opera", "vivaldi"];
const firefoxServiceIds: readonly HomeAppId[] = [
  "instagram", "snapchat", "x", "messenger", "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom",
];
const nativePreviewApps: readonly NativeApp[] = [
  { id: "discord", displayName: "Discord", availability: "installable", isolatedProfileAvailable: false, supportsOverlay: false },
  { id: "telegram", displayName: "Telegram", availability: "installable", isolatedProfileAvailable: true, supportsOverlay: false },
  { id: "signal", displayName: "Signal", availability: "installable", isolatedProfileAvailable: true, supportsOverlay: false },
  { id: "whatsapp", displayName: "WhatsApp", availability: "installable", isolatedProfileAvailable: false, supportsOverlay: false },
];

interface HomeAppDefinition {
  id: HomeAppId;
  displayName: string;
  serviceId: ServiceId | null;
  provider: OfferedEmailProvider | null;
  visibility: HomeAppVisibility;
  section: HomeAppSection;
  defaultLaunchState: LaunchState;
}

const homeAppDefinitions: readonly HomeAppDefinition[] = [
  homeApp("discord", "Discord", "discord"),
  homeApp("instagram", "Instagram", "instagram"),
  homeApp("snapchat", "Snapchat", "snapchat"),
  homeApp("x", "X", "x"),
  homeApp("telegram", "Telegram", "telegram"),
  homeApp("signal", "Signal", "signal"),
  homeApp("whatsapp", "WhatsApp", "whatsapp"),
  homeApp("messenger", "Messenger", "messenger"),
  homeApp("gmail", "Gmail", "email", "gmail"),
  homeApp("outlook", "Outlook", "email", "outlook"),
  homeApp("proton", "Proton Mail", "email", "proton"),
  homeApp("yahoo", "Yahoo Mail", "email", "yahoo"),
  homeApp("aol", "AOL Mail", "email", "aol"),
  homeApp("gmx", "GMX", "email", "gmx"),
  homeApp("maildotcom", "Mail.com", "email", "maildotcom"),
  homeApp("slack", "Slack", "slack", null, "later", "comingSoon"),
  homeApp("linkedin", "LinkedIn messaging", "linkedin", null, "later", "comingSoon"),
];

const previewRegistry: unknown = [
  service("discord", "Discord", "DC", 0, "consumer", "available"),
  service("telegram", "Telegram", "TG", 1, "consumer", "available"),
  service("instagram", "Instagram", "IG", 2, "consumer", "available"),
  service("snapchat", "Snapchat", "SC", 3, "consumer", "available"),
  service("email", "Email", "EM", 4, "consumer", "available"),
  service("x", "X", "X", 5, "consumer", "available"),
  service("messenger", "Facebook Messenger", "MS", 6, "consumer", "available"),
  service("signal", "Signal", "SG", 7, "consumer", "available"),
  service("whatsapp", "WhatsApp", "WA", 8, "consumer", "available"),
  service("slack", "Slack", "SL", 9, "enterprise", "comingSoon"),
  service("linkedin", "LinkedIn messaging", "LI", 10, "enterprise", "comingSoon"),
  service("teams", "Microsoft Teams", "TM", 11, "enterprise", "comingSoon"),
];

export async function loadLinkedServices(): Promise<LinkedService[]> {
  if (isTauriRuntime()) {
    const parsed = parseLinkedServices(await invoke<unknown>("list_linked_services"));
    return parsed ?? [];
  }
  return parseLinkedServices(previewRegistry) ?? [];
}

export async function loadHomeAppCatalog(): Promise<HomeAppCatalogEntry[]> {
  return homeAppsFromServices(await loadLinkedServices());
}

/** Optional Windows-native launch catalog. Embedded profiles remain the default OSL surface. */
export async function loadNativeApps(): Promise<NativeApp[]> {
  if (!isTauriRuntime()) return nativePreviewApps.map((app) => ({ ...app }));
  return parseNativeApps(await invoke<unknown>("list_native_apps"));
}

export async function installNativeApp(appId: NativeAppId): Promise<NativeAppAction> {
  if (!isTauriRuntime() || !nativeAppIds.includes(appId)) throw new Error("native install unavailable");
  return parseNativeAppAction(await invoke<unknown>("install_native_app", { appId }), true);
}

export async function loadMullvadStatus(): Promise<MullvadStatus> {
  if (!isTauriRuntime()) return { availability: "unavailable" };
  return parseMullvadStatus(await invoke<unknown>("get_mullvad_status"));
}

/** Optional preview/future native signal. Unknown runtimes fail closed to showing the choice. */
export async function loadVpnConnectionDetected(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const raw = await invoke<unknown>("get_vpn_connection_status");
  return typeof raw === "object" && raw !== null && (raw as { connected?: unknown }).connected === true;
}

export async function installMullvad(): Promise<MullvadAction> {
  if (!isTauriRuntime()) throw new Error("Mullvad installation unavailable");
  return parseMullvadAction(await invoke<unknown>("install_mullvad"));
}

export async function openMullvad(): Promise<MullvadAction> {
  if (!isTauriRuntime()) throw new Error("Mullvad launch unavailable");
  return parseMullvadAction(await invoke<unknown>("open_mullvad"));
}

export function parseMullvadStatus(raw: unknown): MullvadStatus {
  if (!isExactRecord(raw, ["availability"])
    || !["installed", "installable", "unavailable"].includes(String(raw.availability))) {
    throw new Error("invalid Mullvad status");
  }
  return raw as unknown as MullvadStatus;
}

function parseMullvadAction(raw: unknown): MullvadAction {
  if (!isExactRecord(raw, ["started"]) || raw.started !== true) throw new Error("invalid Mullvad action");
  return raw as unknown as MullvadAction;
}

export async function loadBrowserImports(): Promise<BrowserImportStatus[]> {
  if (!isTauriRuntime()) return browserImportIds.map((id) => ({ id, displayName: id[0].toUpperCase() + id.slice(1), installed: false }));
  return parseBrowserImports(await invoke<unknown>("list_browser_imports"));
}

export async function openBrowserImport(browserId: BrowserImportId): Promise<void> {
  if (!isTauriRuntime() || !browserImportIds.includes(browserId)) throw new Error("browser import unavailable");
  const raw = await invoke<unknown>("open_browser_import", { browserId });
  if (!isExactRecord(raw, ["id", "opened"]) || raw.id !== browserId || raw.opened !== true) {
    throw new Error("invalid browser import response");
  }
}

export async function beginBrowserAccountImport(): Promise<BrowserAccountImportAction> {
  if (!isTauriRuntime()) throw new Error("browser account import unavailable");
  const raw = await invoke<unknown>("begin_browser_account_import");
  if (!isExactRecord(raw, ["preferredSource", "detectedSources", "opened", "mode", "manualExportRequired"])
    || !browserImportIds.includes(raw.preferredSource as BrowserImportId)
    || !Array.isArray(raw.detectedSources)
    || raw.detectedSources.length < 1
    || raw.detectedSources.length > browserImportIds.length
    || new Set(raw.detectedSources).size !== raw.detectedSources.length
    || !raw.detectedSources.every((id) => browserImportIds.includes(id as BrowserImportId))
    || !raw.detectedSources.includes(raw.preferredSource)
    || raw.opened !== true
    || raw.mode !== "firefoxMigrationWizard"
    || typeof raw.manualExportRequired !== "boolean") {
    throw new Error("invalid browser account import response");
  }
  return raw as unknown as BrowserAccountImportAction;
}

export function parseBrowserImports(raw: unknown): BrowserImportStatus[] {
  if (!Array.isArray(raw) || raw.length > browserImportIds.length) throw new Error("invalid browser import catalog");
  const seen = new Set<BrowserImportId>();
  return raw.map((candidate) => {
    if (!isExactRecord(candidate, ["id", "displayName", "installed"])) throw new Error("invalid browser import catalog");
    const id = candidate.id as BrowserImportId;
    if (!browserImportIds.includes(id) || seen.has(id) || !isDisplayString(candidate.displayName, 40) || typeof candidate.installed !== "boolean") {
      throw new Error("invalid browser import catalog");
    }
    seen.add(id);
    return candidate as unknown as BrowserImportStatus;
  });
}

function parseNativeWindowHostAction(raw: unknown, expectedId?: NativeAppId): NativeWindowHostAction {
  const statuses = ["hosted", "resized", "focused", "detached", "unsupported", "failed"];
  const reasons = ["none", "platformUnsupported", "secondaryInstanceUnverified", "appNotInstalled", "profileUnavailable", "launchFailed", "windowNotFound", "windowIdentityChanged", "ownerWindowUnavailable", "hostWindowUnavailable", "windowOperationRejected", "notHosted"];
  if (!isExactRecord(raw, ["id", "status", "reason", "mode"])
    || !nativeAppIds.includes(raw.id as NativeAppId)
    || (expectedId !== undefined && raw.id !== expectedId)
    || !statuses.includes(String(raw.status))
    || !reasons.includes(String(raw.reason))
    || !["none", "ownedBorderless"].includes(String(raw.mode))) {
    throw new Error("invalid native window host response");
  }
  const success = ["hosted", "resized", "focused", "detached"].includes(String(raw.status));
  if ((success && (raw.reason !== "none" || raw.mode !== "ownedBorderless"))
    || (!success && raw.mode !== "none")) {
    throw new Error("invalid native window host response");
  }
  return raw as unknown as NativeWindowHostAction;
}

export async function hostNativeAppWindow(appId: NativeAppId): Promise<NativeWindowHostAction> {
  if (!isTauriRuntime() || !nativeAppIds.includes(appId)) throw new Error("native host unavailable");
  return parseNativeWindowHostAction(await invoke<unknown>("host_native_app_window", { appId }), appId);
}

export async function resizeNativeAppWindow(): Promise<NativeWindowHostAction> {
  if (!isTauriRuntime()) throw new Error("native host unavailable");
  return parseNativeWindowHostAction(await invoke<unknown>("resize_native_app_window"));
}

export async function focusNativeAppWindow(): Promise<NativeWindowHostAction> {
  if (!isTauriRuntime()) throw new Error("native host unavailable");
  return parseNativeWindowHostAction(await invoke<unknown>("focus_native_app_window"));
}

export async function detachNativeAppWindow(): Promise<NativeWindowHostAction> {
  if (!isTauriRuntime()) throw new Error("native host unavailable");
  return parseNativeWindowHostAction(await invoke<unknown>("detach_native_app_window"));
}

export function parseNativeApps(raw: unknown): NativeApp[] {
  if (!Array.isArray(raw) || raw.length > nativeAppIds.length) throw new Error("invalid native app catalog");
  const seen = new Set<NativeAppId>();
  return raw.map((candidate) => {
    if (!isExactRecord(candidate, ["id", "displayName", "availability", "isolatedProfileAvailable", "supportsOverlay"])) throw new Error("invalid native app catalog");
    const id = candidate.id as NativeAppId;
    if (!nativeAppIds.includes(id) || seen.has(id) || !isDisplayString(candidate.displayName, 80)
      || !["installed", "installable", "unavailable"].includes(String(candidate.availability))
      || typeof candidate.isolatedProfileAvailable !== "boolean" || typeof candidate.supportsOverlay !== "boolean") {
      throw new Error("invalid native app catalog");
    }
    seen.add(id);
    return { id, displayName: candidate.displayName as string, availability: candidate.availability as NativeApp["availability"], isolatedProfileAvailable: candidate.isolatedProfileAvailable, supportsOverlay: candidate.supportsOverlay };
  });
}

export function parseNativeAppAction(raw: unknown, installation: boolean): NativeAppAction {
  const keys = installation ? ["id", "started", "packageId"] as const : ["id", "started"] as const;
  if (!isExactRecord(raw, keys) || !nativeAppIds.includes(raw.id as NativeAppId) || raw.started !== true) {
    throw new Error("invalid native app action");
  }
  if (installation && !isDisplayString(raw.packageId, 160)) throw new Error("invalid native app action");
  return raw as unknown as NativeAppAction;
}

export async function loadFirefoxStatus(): Promise<FirefoxStatus> {
  if (!isTauriRuntime()) return { availability: "installable" };
  return parseFirefoxStatus(await invoke<unknown>("get_firefox_status"));
}

export async function launchFirefoxService(serviceId: HomeAppId): Promise<void> {
  if (!isTauriRuntime() || !firefoxServiceIds.includes(serviceId)) throw new Error("Firefox launch unavailable");
  const raw = await invoke<unknown>("launch_firefox_service", { serviceId });
  if (!isExactRecord(raw, ["serviceId", "started"]) || raw.serviceId !== serviceId || raw.started !== true) throw new Error("invalid Firefox launch response");
}

export async function installFirefox(): Promise<void> {
  if (!isTauriRuntime()) throw new Error("Firefox installation unavailable");
  const raw = await invoke<unknown>("install_firefox");
  if (!isExactRecord(raw, ["started", "packageId"]) || raw.started !== true || raw.packageId !== "Mozilla.Firefox") throw new Error("invalid Firefox install response");
}

/**
 * Create one isolated local profile. `provider` selects only a compiled-in
 * email manifest; callers can never supply a URL, executable, or profile path.
 */
export async function createEmbeddedServiceAccount(
  serviceId: ServiceId,
  label: string,
  provider: OfferedEmailProvider | null = null,
): Promise<LinkedAccount> {
  if (!isTauriRuntime() || !serviceIds.includes(serviceId) || !isAccountLabel(label)) {
    throw new Error("embedded service setup unavailable");
  }
  if ((serviceId === "email") !== (provider !== null)) {
    throw new Error("embedded service setup unavailable");
  }
  return parseLinkedAccount(await invoke<unknown>("create_service_account", {
    serviceId,
    label: label.trim(),
    provider,
  }));
}

/** Open one already-owned profile inside OSL's allowlisted child webview. */
export async function openEmbeddedServiceAccount(
  serviceId: ServiceId,
  accountId: string,
): Promise<EmbeddedServiceHost> {
  if (!isTauriRuntime() || !serviceIds.includes(serviceId) || !isOpaqueAccountId(accountId)) {
    throw new Error("embedded service open unavailable");
  }
  return parseEmbeddedServiceHost(await invoke<unknown>("open_service_host", {
    serviceId,
    accountId,
  }));
}

/** Close only OSL's currently hosted child surface before leaving an app. */
export async function closeEmbeddedServiceHost(): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke("close_service_host");
}

/** Remove one exact OSL-owned profile; no caller-supplied path is accepted. */
export async function removeEmbeddedServiceAccount(
  serviceId: ServiceId,
  accountId: string,
): Promise<EmbeddedServiceAccountRemoval> {
  if (!isTauriRuntime() || !serviceIds.includes(serviceId) || !isOpaqueAccountId(accountId)) {
    throw new Error("embedded service removal unavailable");
  }
  return parseEmbeddedServiceAccountRemoval(await invoke<unknown>("remove_service_account", {
    serviceId,
    accountId,
  }));
}

/**
 * First-click setup path: create an isolated profile and immediately show the
 * real service login page inside OSL. A failed page open keeps the profile so
 * the user can retry without creating duplicate accounts.
 */
export async function setupEmbeddedHomeApp(
  app: HomeAppCatalogEntry,
  label = "Personal",
): Promise<EmbeddedServiceSetup> {
  if (!app.serviceId || app.launchState !== "available" || !app.setupEligible) {
    throw new Error("this app is not available for setup");
  }
  const account = await createEmbeddedServiceAccount(app.serviceId, label, app.provider);
  const host = await openEmbeddedServiceAccount(app.serviceId, account.id);
  return { account, host };
}

/** Return only the profiles represented by one Home tile (including email provider). */
export function embeddedAccountsForHomeApp(
  app: HomeAppCatalogEntry,
  services: readonly LinkedService[],
): LinkedAccount[] {
  if (!app.serviceId) return [];
  const service = services.find((candidate) => candidate.id === app.serviceId);
  return service ? serviceAccountsForProvider(service, app.provider) : [];
}

/**
 * Default click behavior for a configured tile. A native application is used
 * only by an explicit separate native-launch action; ordinary tile clicks
 * always resume the selected isolated embedded profile.
 */
export async function openEmbeddedHomeApp(
  app: HomeAppCatalogEntry,
  services: readonly LinkedService[],
  accountId?: string,
): Promise<EmbeddedServiceHost> {
  if (!app.serviceId || app.launchState !== "available") {
    throw new Error("this app is not available");
  }
  const accounts = embeddedAccountsForHomeApp(app, services);
  const account = accountId
    ? accounts.find((candidate) => candidate.id === accountId)
    : accounts[0];
  if (!account) throw new Error("set up this app first");
  return openEmbeddedServiceAccount(app.serviceId, account.id);
}

export function parseEmbeddedServiceHost(raw: unknown): EmbeddedServiceHost {
  if (!isExactRecord(raw, ["serviceId", "accountId", "generation"])
    || !serviceIds.includes(raw.serviceId as ServiceId)
    || !isOpaqueAccountId(raw.accountId)
    || !Number.isSafeInteger(raw.generation)
    || Number(raw.generation) < 1) {
    throw new Error("invalid embedded service host");
  }
  return raw as unknown as EmbeddedServiceHost;
}

export function parseEmbeddedServiceAccountRemoval(raw: unknown): EmbeddedServiceAccountRemoval {
  if (!isExactRecord(raw, ["serviceId", "accountId", "profileExisted", "cleanupPending", "registryRemoved"])
    || !serviceIds.includes(raw.serviceId as ServiceId)
    || !isOpaqueAccountId(raw.accountId)
    || typeof raw.profileExisted !== "boolean"
    || typeof raw.cleanupPending !== "boolean"
    || raw.registryRemoved !== true) {
    throw new Error("invalid embedded service removal");
  }
  return raw as unknown as EmbeddedServiceAccountRemoval;
}

export function parseFirefoxStatus(raw: unknown): FirefoxStatus {
  if (!isExactRecord(raw, ["availability"]) || !["installed", "installable", "unavailable"].includes(String(raw.availability))) {
    throw new Error("invalid Firefox status");
  }
  return { availability: raw.availability as FirefoxStatus["availability"] };
}

/**
 * Expands the generic Email service into the fixed provider tiles promised by
 * the home grid. An entry remains present with zero accounts so an unlinked app
 * is a setup action, not something that disappears. `linked` means an owned
 * isolated profile exists; it does not claim that a remote login was detected.
 */
export function homeAppsFromServices(services: readonly LinkedService[]): HomeAppCatalogEntry[] {
  const byId = new Map(services.map((service) => [service.id, service]));
  return homeAppDefinitions.map((definition) => {
    const service = definition.serviceId ? byId.get(definition.serviceId) : undefined;
    const accounts = service?.accounts.filter((account) => definition.provider === null || account.provider === definition.provider) ?? [];
    const accountCount = accounts.length;
    const setupEligible = definition.visibility === "launch"
      && service?.launchState === "available"
      && service.accounts.length < maxAccountsPerService;
    return {
      id: definition.id,
      displayName: definition.displayName,
      serviceId: definition.serviceId,
      provider: definition.provider,
      visibility: definition.visibility,
      section: definition.section,
      launchState: service?.launchState ?? definition.defaultLaunchState,
      linked: accountCount > 0,
      accountCount,
      setupEligible,
    };
  });
}

/**
 * Every configured app appears exactly once. A preference may reorder apps,
 * but it cannot silently hide a newly configured service from the top strip.
 */
export function configuredTopStripApps(
  catalog: readonly HomeAppCatalogEntry[],
  preferredOrder: readonly string[] = [],
): HomeAppCatalogEntry[] {
  const configured = catalog.filter((app) => app.visibility === "launch" && app.accountCount > 0);
  const byId = new Map(configured.map((app) => [app.id, app]));
  const ordered: HomeAppCatalogEntry[] = [];
  for (const id of preferredOrder) {
    const app = byId.get(id as HomeAppId);
    if (!app) continue;
    ordered.push(app);
    byId.delete(app.id);
  }
  for (const app of configured) {
    if (!byId.delete(app.id)) continue;
    ordered.push(app);
  }
  return ordered;
}

/** Notifications become useful only after two distinct app tiles are configured. */
export function notificationIntegrationEligibility(
  catalog: readonly HomeAppCatalogEntry[],
): NotificationIntegrationEligibility {
  const configuredAppCount = configuredTopStripApps(catalog).length;
  return { configuredAppCount, eligible: configuredAppCount >= 2 };
}

/** Keep a provider-specific Home tile scoped to only that provider's profiles. */
export function serviceAccountsForProvider(
  service: LinkedService,
  provider: EmailProvider | null,
): LinkedAccount[] {
  if (service.id !== "email" || provider === null) return service.accounts;
  return service.accounts.filter((account) => account.provider === provider);
}

export function parseLinkedAccount(raw: unknown): LinkedAccount {
  if (!isExactRecord(raw, ["id", "label", "displayHandle", "state", "provider"])) throw new Error("invalid service account response");
  if (
    typeof raw.id !== "string"
    || !/^[a-z0-9][a-z0-9_-]{0,79}$/.test(raw.id)
    || !isDisplayString(raw.label, 40)
    || !isDisplayString(raw.displayHandle, 120)
    || !connectionStates.includes(raw.state as ConnectionState)
    || !(raw.provider === null || emailProviders.includes(raw.provider as EmailProvider))
  ) throw new Error("invalid service account response");
  return raw as unknown as LinkedAccount;
}

export function parseLinkedServices(raw: unknown): LinkedService[] | null {
  if (!Array.isArray(raw) || raw.length !== serviceIds.length) return null;
  const parsed: LinkedService[] = [];
  const seenServices = new Set<ServiceId>();
  for (const candidate of raw) {
    if (!isExactRecord(candidate, ["id", "displayName", "sidebarGlyph", "sidebarOrder", "category", "launchState", "supportsNativePreview", "supportsProtectedPreview", "accounts"])) return null;
    if (!serviceIds.includes(candidate.id as ServiceId) || seenServices.has(candidate.id as ServiceId)) return null;
    if (!isDisplayString(candidate.displayName, 80)) return null;
    if (!isDisplayString(candidate.sidebarGlyph, 8) || typeof candidate.sidebarOrder !== "number" || !Number.isSafeInteger(candidate.sidebarOrder) || candidate.sidebarOrder < 0 || candidate.sidebarOrder > 255) return null;
    if (candidate.category !== "consumer" && candidate.category !== "enterprise") return null;
    if (candidate.launchState !== "available" && candidate.launchState !== "comingSoon") return null;
    if (typeof candidate.supportsNativePreview !== "boolean" || typeof candidate.supportsProtectedPreview !== "boolean") return null;
    if (!Array.isArray(candidate.accounts) || candidate.accounts.length > 10) return null;

    const seenAccounts = new Set<string>();
    const accounts: LinkedAccount[] = [];
    for (const rawAccount of candidate.accounts) {
      try {
        const account = parseLinkedAccount(rawAccount);
        if (seenAccounts.has(account.id)) continue;
        if (candidate.id === "email" ? account.provider === null : account.provider !== null) continue;
        seenAccounts.add(account.id);
        accounts.push(account);
      } catch {
        // One corrupt local account row must not hide every other service.
        // The invalid row is never rendered or passed back to native code.
      }
    }

    const id = candidate.id as ServiceId;
    seenServices.add(id);
    parsed.push({
      id,
      displayName: candidate.displayName,
      sidebarGlyph: candidate.sidebarGlyph,
      sidebarOrder: candidate.sidebarOrder,
      category: candidate.category,
      launchState: candidate.launchState,
      supportsNativePreview: candidate.supportsNativePreview,
      supportsProtectedPreview: candidate.supportsProtectedPreview,
      accounts,
    });
  }
  if (seenServices.size !== serviceIds.length || new Set(parsed.map((service) => service.sidebarOrder)).size !== serviceIds.length) return null;
  return parsed.sort((a, b) => a.sidebarOrder - b.sidebarOrder);
}

export function escapeHtml(value: string): string {
  return value.replace(/[&<>'"]/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "'": "&#39;",
    '"': "&quot;",
  })[character] ?? character);
}

function service(id: ServiceId, displayName: string, sidebarGlyph: string, sidebarOrder: number, category: ServiceCategory, launchState: LaunchState): LinkedService {
  return { id, displayName, sidebarGlyph, sidebarOrder, category, launchState, supportsNativePreview: launchState === "available", supportsProtectedPreview: launchState === "available", accounts: [] };
}

function homeApp(
  id: HomeAppId,
  displayName: string,
  serviceId: ServiceId | null,
  provider: OfferedEmailProvider | null = null,
  visibility: HomeAppVisibility = "launch",
  defaultLaunchState: LaunchState = "available",
): HomeAppDefinition {
  const section: HomeAppSection = visibility === "later" ? "later" : serviceId === "email" ? "email" : "social";
  return { id, displayName, serviceId, provider, visibility, section, defaultLaunchState };
}

function isOpaqueAccountId(value: unknown): value is string {
  return typeof value === "string" && /^[a-z0-9](?:[a-z0-9_-]{0,78}[a-z0-9])?$/.test(value);
}

function isAccountLabel(value: unknown): value is string {
  return typeof value === "string"
    && value.trim().length > 0
    && value.trim().length <= 40
    && !/[\u0000-\u001f\u007f]/.test(value);
}

function isDisplayString(value: unknown, maxLength: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= maxLength && !/[\u0000-\u001f\u007f]/.test(value);
}

function isExactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value);
  return actual.length === keys.length && actual.every((key) => keys.includes(key));
}
