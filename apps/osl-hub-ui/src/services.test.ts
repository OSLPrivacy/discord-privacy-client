import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { configuredTopStripApps, embeddedAccountsForHomeApp, escapeHtml, homeAppsFromServices, loadLinkedServices, loadNativeApps, notificationIntegrationEligibility, parseEmbeddedServiceHost, parseFirefoxStatus, parseLinkedAccount, parseLinkedServices, parseMullvadStatus, parseNativeAppAction, parseNativeApps, serviceAccountsForProvider } from "./services";

const originalAppRoster = [
  "discord", "telegram", "instagram", "snapchat", "x", "messenger", "signal", "whatsapp",
  "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud",
] as const;
const unsupportedOriginalApps = originalAppRoster.filter((id) => id !== "discord");

function validRegistry(): unknown[] {
  const ids = ["discord", "telegram", "instagram", "snapchat", "email", "x", "messenger", "signal", "whatsapp", "slack", "linkedin", "teams"];
  return ids.map((id, sidebarOrder) => ({
    id,
    displayName: id,
    sidebarGlyph: id.slice(0, 2).toUpperCase(),
    sidebarOrder,
    category: id === "slack" || id === "linkedin" || id === "teams" ? "enterprise" : "consumer",
    launchState: id === "signal" || id === "slack" || id === "linkedin" || id === "teams" ? "comingSoon" : "available",
    supportsNativePreview: id !== "signal" && id !== "slack" && id !== "linkedin" && id !== "teams",
    supportsProtectedPreview: id !== "signal" && id !== "slack" && id !== "linkedin" && id !== "teams",
    accounts: id === "signal" || id === "slack" || id === "linkedin" || id === "teams" ? [] : [{ id: `${id}-preview`, label: "Personal", displayHandle: "@preview", state: "demoLinked", provider: id === "email" ? "gmail" : null }],
  }));
}

describe("linked-service contract", () => {
  it("accepts and orders the exact twelve-service Rust payload", () => {
    expect(parseLinkedServices(validRegistry())).toHaveLength(12);
  });

  it("keeps WhatsApp in the service registry without making it a launch tile", async () => {
    const whatsapp = (await loadLinkedServices()).find((service) => service.id === "whatsapp");
    expect(whatsapp).toMatchObject({ displayName: "WhatsApp", category: "consumer", launchState: "available" });
    expect(homeAppsFromServices(await loadLinkedServices()).find((app) => app.id === "whatsapp"))
      .toMatchObject({ visibility: "launch", launchState: "comingSoon", setupEligible: false });
  });

  it("fails closed on unknown service fields or duplicate services", () => {
    const unknown = validRegistry();
    (unknown[0] as Record<string, unknown>).credential = "must-not-exist";
    expect(parseLinkedServices(unknown)).toBeNull();

    const duplicate = validRegistry();
    (duplicate[1] as Record<string, unknown>).id = "discord";
    expect(parseLinkedServices(duplicate)).toBeNull();

  });

  it("quarantines one malformed account without hiding the service catalog", () => {
    const malformed = validRegistry();
    ((malformed[2] as Record<string, unknown>).accounts as Array<Record<string, unknown>>)[0].id = "../cookie";
    const parsed = parseLinkedServices(malformed);
    expect(parsed).toHaveLength(12);
    expect(parsed?.find((service) => service.id === "instagram")?.accounts).toEqual([]);
    expect(parsed?.find((service) => service.id === "discord")?.accounts).toHaveLength(1);
  });

  it("escapes backend labels before innerHTML rendering", () => {
    expect(escapeHtml('<img src=x onerror="boom">')).toBe("&lt;img src=x onerror=&quot;boom&quot;&gt;");
  });

  it("strictly validates native launcher state and action receipts", () => {
    expect(parseNativeApps([{ id: "discord", displayName: "Discord", availability: "installed", supportStatus: "beta", protectedMode: "assistOnly", isolatedProfileAvailable: false, supportsOverlay: false }]))
      .toEqual([{ id: "discord", displayName: "Discord", availability: "installed", supportStatus: "beta", protectedMode: "assistOnly", isolatedProfileAvailable: false, supportsOverlay: false }]);
    expect(parseNativeApps([{ id: "telegram", displayName: "Telegram", availability: "installed", supportStatus: "comingSoon", protectedMode: "unavailable", isolatedProfileAvailable: true, supportsOverlay: false }]))
      .toEqual([{ id: "telegram", displayName: "Telegram", availability: "installed", supportStatus: "comingSoon", protectedMode: "unavailable", isolatedProfileAvailable: true, supportsOverlay: false }]);
    expect(parseNativeAppAction({ id: "discord", started: true }, false)).toEqual({ id: "discord", started: true });
    expect(parseNativeAppAction({ id: "signal", started: true, packageId: "OpenWhisperSystems.Signal" }, true).packageId)
      .toBe("OpenWhisperSystems.Signal");
    expect(() => parseNativeApps([{ id: "discord", displayName: "Discord", availability: "web", supportStatus: "beta", protectedMode: "assistOnly", isolatedProfileAvailable: false, supportsOverlay: true }])).toThrow();
    expect(() => parseNativeApps([{ id: "discord", displayName: "Discord", availability: "installed", supportStatus: "beta", protectedMode: "assistOnly", supportsOverlay: false }])).toThrow();
    expect(() => parseNativeApps([{ id: "telegram", displayName: "Telegram", availability: "installed", supportStatus: "comingSoon", protectedMode: "assistOnly", isolatedProfileAvailable: true, supportsOverlay: false }])).toThrow();
    expect(() => parseNativeApps([{ id: "signal", displayName: "Signal", availability: "installed", supportStatus: "comingSoon", protectedMode: "unavailable", isolatedProfileAvailable: true, supportsOverlay: true }])).toThrow();
    expect(() => parseNativeAppAction({ id: "instagram", started: true }, false)).toThrow();
  });

  it("strictly validates Firefox workspace availability", () => {
    expect(parseFirefoxStatus({ availability: "installed" })).toEqual({ availability: "installed" });
    expect(() => parseFirefoxStatus({ availability: "installed", profile: "secret" })).toThrow();
    expect(() => parseFirefoxStatus({ availability: "embedded" })).toThrow();
  });

  it("strictly validates the narrow Mullvad availability receipt", () => {
    expect(parseMullvadStatus({ availability: "installed" })).toEqual({
      availability: "installed",
      integrationState: "availableToOpen",
      privacyScope: "networkOnly",
      connectionState: "notObserved",
    });
    expect(parseMullvadStatus({ availability: "installable" })).toEqual({
      availability: "installable",
      integrationState: "installable",
      privacyScope: "networkOnly",
      connectionState: "notObserved",
    });
    expect(parseMullvadStatus({ availability: "unavailable" })).toEqual({
      availability: "unavailable",
      integrationState: "unavailable",
      privacyScope: "networkOnly",
      connectionState: "notObserved",
    });
    expect(() => parseMullvadStatus({ availability: "connected" })).toThrow();
    expect(() => parseMullvadStatus({ availability: "installed", account: "secret" })).toThrow();
    expect(() => parseMullvadStatus({ availability: "installed", connectionState: "connected" })).toThrow();
  });

  it("strictly validates a newly-created isolated account profile", () => {
    expect(parseLinkedAccount({ id: "instagram-1", label: "Account 1", displayHandle: "Sign in on the service", state: "notLinked", provider: null }).id).toBe("instagram-1");
    expect(() => parseLinkedAccount({ id: "../profile", label: "Account 1", displayHandle: "Sign in", state: "notLinked", provider: null })).toThrow();
  });

  it("accepts new and legacy allowlisted email providers", () => {
    for (const provider of ["aol", "gmx", "maildotcom", "icloud", "tuta", "zoho"]) {
      expect(parseLinkedAccount({ id: `email-${provider}`, label: "Personal", displayHandle: "Sign in", state: "notLinked", provider }).provider).toBe(provider);
    }
  });

  it("presents only Discord from the original app roster as working", () => {
    const services = parseLinkedServices(validRegistry())!;
    for (const service of services) service.accounts = [];
    const apps = homeAppsFromServices(services);
    const launch = apps.filter((app) => app.visibility === "launch");
    const working = launch.filter((app) => app.launchState === "available");
    const roadmap = launch.filter((app) => app.launchState === "comingSoon");

    expect(launch.map((app) => app.id)).toEqual([...originalAppRoster]);
    expect(working.map((app) => app.id)).toEqual(["discord"]);
    expect(roadmap.map((app) => app.id)).toEqual([...unsupportedOriginalApps]);
    expect(launch.every((app) => !app.linked && app.accountCount === 0)).toBe(true);
    expect(launch.filter((app) => app.setupEligible).map((app) => app.id)).toEqual(["discord"]);
    for (const unsupported of unsupportedOriginalApps) {
      expect(apps.find((app) => app.id === unsupported)).toMatchObject({ launchState: "comingSoon", setupEligible: false });
    }
    expect(launch.filter((app) => app.section === "social").map((app) => app.id)).toEqual([
      "discord", "telegram", "instagram", "snapchat", "x", "messenger", "signal", "whatsapp",
    ]);
    expect(launch.filter((app) => app.section === "email").map((app) => app.id)).toEqual([
      "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud",
    ]);

    const fallbackLaunch = homeAppsFromServices([]).filter((app) => app.visibility === "launch");
    expect(fallbackLaunch.map((app) => app.id)).toEqual(launch.map((app) => app.id));
    expect(fallbackLaunch.filter((app) => app.launchState === "available").map((app) => app.id)).toEqual(["discord"]);
    expect(fallbackLaunch.every((app) => !app.linked && !app.setupEligible)).toBe(true);
  });

  it("keeps every configured app in a deterministic top strip", () => {
    const services = parseLinkedServices(validRegistry())!;
    for (const service of services) service.accounts = [];
    services.find((service) => service.id === "discord")!.accounts = [
      { id: "discord-personal", label: "Personal", displayHandle: "Sign in", state: "notLinked", provider: null },
      { id: "discord-work", label: "Work", displayHandle: "Sign in", state: "notLinked", provider: null },
    ];
    services.find((service) => service.id === "email")!.accounts = [
      { id: "gmail-one", label: "Mail", displayHandle: "Sign in", state: "notLinked", provider: "gmail" },
      { id: "outlook-one", label: "Work", displayHandle: "Sign in", state: "notLinked", provider: "outlook" },
    ];
    const catalog = homeAppsFromServices(services);
    expect(configuredTopStripApps(catalog).map((app) => app.id)).toEqual(["discord"]);
    expect(configuredTopStripApps(catalog, ["outlook", "unknown", "outlook"]).map((app) => app.id))
      .toEqual(["discord"]);
    expect(notificationIntegrationEligibility(catalog)).toEqual({ configuredAppCount: 1, eligible: false });
    expect(notificationIntegrationEligibility(catalog.filter((app) => app.id !== "gmail" && app.id !== "outlook")))
      .toEqual({ configuredAppCount: 1, eligible: false });
  });

  it("scopes embedded opening to the exact app and email provider", () => {
    const services = parseLinkedServices(validRegistry())!;
    const email = services.find((service) => service.id === "email")!;
    email.accounts = [
      { id: "gmail-one", label: "Gmail", displayHandle: "Sign in", state: "notLinked", provider: "gmail" },
      { id: "outlook-one", label: "Outlook", displayHandle: "Sign in", state: "notLinked", provider: "outlook" },
    ];
    const catalog = homeAppsFromServices(services);
    expect(embeddedAccountsForHomeApp(catalog.find((app) => app.id === "gmail")!, services).map((account) => account.id))
      .toEqual(["gmail-one"]);
    expect(embeddedAccountsForHomeApp(catalog.find((app) => app.id === "outlook")!, services).map((account) => account.id))
      .toEqual(["outlook-one"]);
  });

  it("accepts only an exact bounded embedded-host receipt", () => {
    expect(parseEmbeddedServiceHost({ serviceId: "discord", accountId: "acct-123", generation: 7 }))
      .toEqual({ serviceId: "discord", accountId: "acct-123", generation: 7 });
    expect(() => parseEmbeddedServiceHost({ serviceId: "discord", accountId: "../cookies", generation: 7 })).toThrow();
    expect(() => parseEmbeddedServiceHost({ serviceId: "discord", accountId: "acct-123", generation: 0 })).toThrow();
    expect(() => parseEmbeddedServiceHost({ serviceId: "discord", accountId: "acct-123", generation: 7, url: "https://evil.example" })).toThrow();
  });

  it("reports provider-specific linked state and the shared Email setup limit", () => {
    const services = parseLinkedServices(validRegistry())!;
    for (const service of services) service.accounts = [];
    const email = services.find((service) => service.id === "email")!;
    email.accounts = [
      { id: "gmail-personal", label: "Personal", displayHandle: "Sign in", state: "notLinked", provider: "gmail" },
      { id: "gmail-work", label: "Work", displayHandle: "Sign in", state: "notLinked", provider: "gmail" },
      { id: "proton-private", label: "Private", displayHandle: "Sign in", state: "notLinked", provider: "proton" },
    ];
    const apps = homeAppsFromServices(services);
    expect(apps.find((app) => app.id === "gmail")).toMatchObject({ linked: true, accountCount: 2, launchState: "comingSoon", setupEligible: false });
    expect(apps.find((app) => app.id === "proton")).toMatchObject({ linked: true, accountCount: 1, launchState: "comingSoon", setupEligible: false });
    expect(apps.find((app) => app.id === "outlook")).toMatchObject({ linked: false, accountCount: 0, setupEligible: false, visibility: "launch", launchState: "comingSoon" });

    while (email.accounts.length < 10) {
      const index = email.accounts.length;
      email.accounts.push({ id: `yahoo-${index}`, label: `Profile ${index}`, displayHandle: "Sign in", state: "notLinked", provider: "yahoo" });
    }
    expect(homeAppsFromServices(services).filter((app) => app.serviceId === "email").every((app) => !app.setupEligible)).toBe(true);
  });

  it("scopes a provider tile to only that provider's isolated profiles", () => {
    const services = parseLinkedServices(validRegistry())!;
    const email = services.find((service) => service.id === "email")!;
    email.accounts = [
      { id: "outlook-one", label: "Outlook", displayHandle: "Sign in", state: "notLinked", provider: "outlook" },
      { id: "gmail-one", label: "Gmail", displayHandle: "Sign in", state: "notLinked", provider: "gmail" },
      { id: "gmail-two", label: "Gmail work", displayHandle: "Sign in", state: "notLinked", provider: "gmail" },
    ];

    expect(serviceAccountsForProvider(email, "gmail").map((account) => account.id)).toEqual(["gmail-one", "gmail-two"]);
    expect(serviceAccountsForProvider(email, "outlook").map((account) => account.id)).toEqual(["outlook-one"]);
    expect(serviceAccountsForProvider(email, null)).toHaveLength(3);
  });

  it("keeps unsupported app tiles out of launch apps as coming-soon work", () => {
    const apps = homeAppsFromServices(parseLinkedServices(validRegistry())!);
    expect(apps.filter((app) => app.visibility === "launch" && app.launchState === "comingSoon")).toEqual([
      expect.objectContaining({ id: "telegram", setupEligible: false }),
      expect.objectContaining({ id: "instagram", setupEligible: false }),
      expect.objectContaining({ id: "snapchat", setupEligible: false }),
      expect.objectContaining({ id: "x", setupEligible: false }),
      expect.objectContaining({ id: "messenger", setupEligible: false }),
      expect.objectContaining({ id: "signal", setupEligible: false }),
      expect.objectContaining({ id: "whatsapp", setupEligible: false }),
      expect.objectContaining({ id: "gmail", setupEligible: false }),
      expect.objectContaining({ id: "outlook", setupEligible: false }),
      expect.objectContaining({ id: "proton", setupEligible: false }),
      expect.objectContaining({ id: "yahoo", setupEligible: false }),
      expect.objectContaining({ id: "aol", setupEligible: false }),
      expect.objectContaining({ id: "gmx", setupEligible: false }),
      expect.objectContaining({ id: "maildotcom", setupEligible: false }),
      expect.objectContaining({ id: "icloud", setupEligible: false }),
    ]);
    expect(apps.filter((app) => app.visibility === "later")).toEqual([
      expect.objectContaining({ id: "slack", launchState: "comingSoon", setupEligible: false }),
      expect.objectContaining({ id: "linkedin", launchState: "comingSoon", setupEligible: false }),
    ]);
  });
});

/**
 * The TS catalog is a FALLBACK, not a second opinion.
 *
 * `loadNativeApps` returns `nativePreviewApps` verbatim whenever the app is not
 * running under Tauri, so this list is what a user sees before the backend has
 * answered. Rust owns the support decision (`native_apps.rs` `NATIVE_APPS`);
 * this list is only allowed to agree with it. Two catalogs that can disagree is
 * how a provider gets ungated on one side and not the other.
 *
 * The expected values are READ OUT OF THE RUST SOURCE rather than written here,
 * so this test cannot be satisfied by editing it to match a drifted catalog.
 */
describe("native app catalog agrees with the Rust support decision", () => {
  const nativeAppsRs = readFileSync(
    new URL("../../osl-hub/src/native_apps.rs", import.meta.url),
    "utf8",
  );

  /** `SupportLevel` -> the `NativeAppSupportStatus` `native_app_support_status` maps it to. */
  const publicStatusOf: Record<string, string> = {
    Supported: "beta",
    Experimental: "beta",
    ComingSoon: "comingSoon",
    ExternallyBlocked: "externallyBlocked",
  };

  function rustSupportLevel(rustId: string): string {
    const manifest = nativeAppsRs.split("NativeAppManifest {")
      .find((block) => block.includes(`id: NativeAppId::${rustId},`));
    expect(manifest, `native_apps.rs has no manifest for ${rustId}`).toBeTruthy();
    const level = /adapter_support: SupportLevel::(\w+),/.exec(manifest as string);
    expect(level, `no adapter_support for ${rustId}`).toBeTruthy();
    return (level as RegExpExecArray)[1];
  }

  it("reads a support level per app, and they are not all the same", () => {
    const levels = ["Discord", "Telegram", "Signal", "Whatsapp", "Outlook"].map(rustSupportLevel);
    expect(levels).toHaveLength(5);
    expect(new Set(levels).size).toBeGreaterThan(1);
  });

  it("never claims more than Rust does", async () => {
    const rustIdFor: Record<string, string> = {
      discord: "Discord", telegram: "Telegram", signal: "Signal",
      whatsapp: "Whatsapp", outlook: "Outlook",
    };
    const catalog = await loadNativeApps();
    expect(catalog.length).toBe(Object.keys(rustIdFor).length);
    for (const app of catalog) {
      const expected = publicStatusOf[rustSupportLevel(rustIdFor[app.id])];
      expect(expected, `unmapped SupportLevel for ${app.id}`).toBeTruthy();
      expect(app.supportStatus, `${app.id} disagrees with native_apps.rs`).toBe(expected);
    }
  });
});
