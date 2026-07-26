import { describe, expect, it } from "vitest";
import { configuredTopStripApps, embeddedAccountsForHomeApp, escapeHtml, homeAppsFromServices, loadLinkedServices, notificationIntegrationEligibility, parseEmbeddedServiceHost, parseFirefoxStatus, parseLinkedAccount, parseLinkedServices, parseMullvadStatus, parseNativeAppAction, parseNativeApps, serviceAccountsForProvider } from "./services";

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

  it("offers WhatsApp as an available consumer service in browser preview", async () => {
    const whatsapp = (await loadLinkedServices()).find((service) => service.id === "whatsapp");
    expect(whatsapp).toMatchObject({ displayName: "WhatsApp", category: "consumer", launchState: "available" });
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
    expect(parseNativeApps([{ id: "discord", displayName: "Discord", availability: "installed", isolatedProfileAvailable: false, supportsOverlay: false }]))
      .toEqual([{ id: "discord", displayName: "Discord", availability: "installed", isolatedProfileAvailable: false, supportsOverlay: false }]);
    expect(parseNativeApps([{ id: "telegram", displayName: "Telegram", availability: "installed", isolatedProfileAvailable: true, supportsOverlay: false }]))
      .toEqual([{ id: "telegram", displayName: "Telegram", availability: "installed", isolatedProfileAvailable: true, supportsOverlay: false }]);
    expect(parseNativeAppAction({ id: "discord", started: true }, false)).toEqual({ id: "discord", started: true });
    expect(parseNativeAppAction({ id: "signal", started: true, packageId: "OpenWhisperSystems.Signal" }, true).packageId)
      .toBe("OpenWhisperSystems.Signal");
    expect(() => parseNativeApps([{ id: "discord", displayName: "Discord", availability: "web", isolatedProfileAvailable: false, supportsOverlay: true }])).toThrow();
    expect(() => parseNativeApps([{ id: "discord", displayName: "Discord", availability: "installed", supportsOverlay: false }])).toThrow();
    expect(() => parseNativeAppAction({ id: "instagram", started: true }, false)).toThrow();
  });

  it("strictly validates Firefox workspace availability", () => {
    expect(parseFirefoxStatus({ availability: "installed" })).toEqual({ availability: "installed" });
    expect(() => parseFirefoxStatus({ availability: "installed", profile: "secret" })).toThrow();
    expect(() => parseFirefoxStatus({ availability: "embedded" })).toThrow();
  });

  it("strictly validates the narrow Mullvad availability receipt", () => {
    expect(parseMullvadStatus({ availability: "installed" })).toEqual({ availability: "installed" });
    expect(parseMullvadStatus({ availability: "installable" })).toEqual({ availability: "installable" });
    expect(() => parseMullvadStatus({ availability: "connected" })).toThrow();
    expect(() => parseMullvadStatus({ availability: "installed", account: "secret" })).toThrow();
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

  it("keeps every promised launch app visible before any profile is linked", () => {
    const services = parseLinkedServices(validRegistry())!;
    for (const service of services) service.accounts = [];
    const apps = homeAppsFromServices(services);
    const launch = apps.filter((app) => app.visibility === "launch");
    expect(launch.map((app) => app.id)).toEqual([
      "discord", "instagram", "snapchat", "x", "telegram", "signal", "whatsapp", "messenger",
      "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud",
    ]);
    expect(launch.every((app) => !app.linked && app.accountCount === 0)).toBe(true);
    expect(launch.find((app) => app.id === "discord")?.setupEligible).toBe(true);
    expect(launch.find((app) => app.id === "signal")).toMatchObject({ launchState: "comingSoon", setupEligible: false });
    expect(launch.filter((app) => app.section === "social").map((app) => app.id)).toEqual([
      "discord", "instagram", "snapchat", "x", "telegram", "signal", "whatsapp", "messenger",
    ]);
    expect(launch.filter((app) => app.section === "email").map((app) => app.id)).toEqual([
      "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud",
    ]);

    const fallbackLaunch = homeAppsFromServices([]).filter((app) => app.visibility === "launch");
    expect(fallbackLaunch.map((app) => app.id)).toEqual(launch.map((app) => app.id));
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
    expect(configuredTopStripApps(catalog).map((app) => app.id)).toEqual(["discord", "gmail", "outlook"]);
    expect(configuredTopStripApps(catalog, ["outlook", "unknown", "outlook"]).map((app) => app.id))
      .toEqual(["outlook", "discord", "gmail"]);
    expect(notificationIntegrationEligibility(catalog)).toEqual({ configuredAppCount: 3, eligible: true });
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
    expect(apps.find((app) => app.id === "gmail")).toMatchObject({ linked: true, accountCount: 2, setupEligible: true });
    expect(apps.find((app) => app.id === "proton")).toMatchObject({ linked: true, accountCount: 1, setupEligible: true });
    expect(apps.find((app) => app.id === "outlook")).toMatchObject({ linked: false, accountCount: 0, setupEligible: true });

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

  it("keeps Slack and LinkedIn out of launch apps as coming-soon work", () => {
    const apps = homeAppsFromServices(parseLinkedServices(validRegistry())!);
    expect(apps.filter((app) => app.visibility === "later")).toEqual([
      expect.objectContaining({ id: "slack", launchState: "comingSoon", setupEligible: false }),
      expect.objectContaining({ id: "linkedin", launchState: "comingSoon", setupEligible: false }),
    ]);
  });
});
