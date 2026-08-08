import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { configuredTopStripApps, embeddedAccountsForHomeApp, escapeHtml, homeAppsFromServices, loadLinkedServices, loadNativeApps, nativeAppGeneratedLabel, notificationIntegrationEligibility, parseEmbeddedServiceHost, parseFirefoxStatus, parseLinkedAccount, parseLinkedServices, parseMullvadStatus, parseNativeAppAction, parseNativeApps, serviceAccountsForProvider } from "./services";

const originalAppRoster = [
  "discord", "telegram", "instagram", "signal", "whatsapp", "x", "messenger",
  "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud", "tuta",
] as const;
const unsupportedOriginalApps = originalAppRoster.filter((id) => id !== "discord");

function validRegistry(): unknown[] {
  const ids = ["discord", "telegram", "instagram", "email", "signal", "whatsapp", "x", "messenger"];
  return ids.map((id, sidebarOrder) => ({
    id,
    displayName: id,
    sidebarGlyph: id.slice(0, 2).toUpperCase(),
    sidebarOrder,
    category: "consumer",
    launchState: "available",
    supportsNativePreview: true,
    supportsProtectedPreview: true,
    accounts: [{ id: `${id}-preview`, label: "Personal", displayHandle: "@preview", state: "demoLinked", provider: id === "email" ? "gmail" : null }],
  }));
}

describe("linked-service contract", () => {
  it("accepts and orders the exact ruled-service Rust payload", () => {
    expect(parseLinkedServices(validRegistry())).toHaveLength(8);
  });

  it("task 4256 returns Instagram from the app service catalog without making it sendable or Ready", async () => {
    const services = await loadLinkedServices();
    const instagram = services.find((service) => service.id === "instagram");
    const homeTile = homeAppsFromServices(services).find((app) => app.id === "instagram");
    const nativePreview = (await loadNativeApps()).find((app) => app.id === "instagram");
    const missingLists = [
      services.some((service) => service.id === "instagram"),
      homeTile?.id === "instagram",
      nativePreview?.id === "instagram",
    ].filter((present) => !present).length;

    expect(instagram).toMatchObject({
      id: "instagram",
      displayName: "Instagram",
      sidebarGlyph: "IG",
      launchState: "available",
    });
    expect(homeTile).toMatchObject({
      id: "instagram",
      displayName: "Instagram",
      serviceId: "instagram",
      launchState: "comingSoon",
      setupEligible: false,
    });
    expect(nativePreview).toMatchObject({
      id: "instagram",
      displayName: "Instagram",
      availability: "unavailable",
      supportStatus: "comingSoon",
      carrierEvidence: "notBuilt",
      deliveryEvidence: "neverProvenLive",
      protectedMode: "unavailable",
      supportsOverlay: false,
    });
    expect(missingLists).toBe(0);
    console.log(`TASK4256_APP_SERVICE_RESULT id=${instagram?.id} displayName=${instagram?.displayName} shortName=${instagram?.sidebarGlyph}`);
    console.log(`TASK4256_HOME_TILE id=${homeTile?.id} launchState=${homeTile?.launchState} setupEligible=${homeTile?.setupEligible}`);
    console.log(`TASK4256_NATIVE_PREVIEW id=${nativePreview?.id} availability=${nativePreview?.availability} supportStatus=${nativePreview?.supportStatus} carrierEvidence=${nativePreview?.carrierEvidence} protectedMode=${nativePreview?.protectedMode} supportsOverlay=${nativePreview?.supportsOverlay}`);
    console.log(`TASK4256_MISSING_INSTAGRAM_LIST_COUNT=${missingLists}`);
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
    ((malformed[1] as Record<string, unknown>).accounts as Array<Record<string, unknown>>)[0].id = "../cookie";
    const parsed = parseLinkedServices(malformed);
    expect(parsed).toHaveLength(8);
    expect(parsed?.find((service) => service.id === "telegram")?.accounts).toEqual([]);
    expect(parsed?.find((service) => service.id === "discord")?.accounts).toHaveLength(1);
  });

  it("escapes backend labels before innerHTML rendering", () => {
    expect(escapeHtml('<img src=x onerror="boom">')).toBe("&lt;img src=x onerror=&quot;boom&quot;&gt;");
  });

  it("strictly validates native launcher state and action receipts", () => {
    const claim = {
      carrierEvidence: "builtNeverProvenLive", deliveryEvidence: "neverProvenLive",
      claimBlockers: [], claimNote: "Nothing has been proven on this surface yet.",
    } as const;
    const statusPage = (generatedLabel: string) => ({
      capability: "carrier capability is wired but not live-proven",
      generatedLabel,
      explanation: "Nothing has been proven on this surface yet.",
    });
    const discord = { id: "discord", displayName: "Discord", availability: "installed", supportStatus: "noClaim", ...claim, statusPage: statusPage("Not claimed"), protectedMode: "assistOnly", isolatedProfileAvailable: false, supportsOverlay: false };
    const telegram = { id: "telegram", displayName: "Telegram", availability: "installed", supportStatus: "comingSoon", ...claim, statusPage: statusPage("Coming later"), protectedMode: "unavailable", isolatedProfileAvailable: true, supportsOverlay: false };
    expect(parseNativeApps([discord])).toEqual([{ ...discord, claimBlockers: [] }]);
    expect(parseNativeApps([telegram])).toEqual([{ ...telegram, claimBlockers: [] }]);
    expect(parseNativeAppAction({ id: "discord", started: true }, false)).toEqual({ id: "discord", started: true });
    expect(parseNativeAppAction({ id: "signal", started: true, packageId: "OpenWhisperSystems.Signal" }, true).packageId)
      .toBe("OpenWhisperSystems.Signal");
    expect(() => parseNativeApps([{ ...discord, availability: "web", supportsOverlay: true }])).toThrow();
    expect(() => parseNativeApps([{ id: "discord", displayName: "Discord", availability: "installed", supportStatus: "noClaim", ...claim, protectedMode: "assistOnly", supportsOverlay: false }])).toThrow();
    expect(() => parseNativeApps([{ ...telegram, protectedMode: "assistOnly" }])).toThrow();
    expect(() => parseNativeApps([{ id: "signal", displayName: "Signal", availability: "installed", supportStatus: "comingSoon", ...claim, protectedMode: "unavailable", isolatedProfileAvailable: true, supportsOverlay: true }])).toThrow();
    // The claim state's own fields are validated as strictly as the rest: an
    // unknown label, a missing reason, or an evidence value this build does not
    // understand is a refusal, not a shrug.
    expect(() => parseNativeApps([{ ...telegram, supportStatus: "supported" }])).toThrow();
    expect(() => parseNativeApps([{ ...telegram, carrierEvidence: "probablyFine" }])).toThrow();
    expect(() => parseNativeApps([{ ...telegram, deliveryEvidence: "" }])).toThrow();
    expect(() => parseNativeApps([{ ...telegram, claimNote: "" }])).toThrow();
    expect(() => parseNativeApps([{ ...telegram, claimBlockers: "none" }])).toThrow();
    expect(() => parseNativeApps([{ ...telegram, statusPage: { ...telegram.statusPage, generatedLabel: "Available" } }])).toThrow();
    expect(() => parseNativeApps([{ ...telegram, statusPage: { ...telegram.statusPage, explanation: "A second source of truth." } }])).toThrow();
    expect(() => parseNativeAppAction({ id: "snapchat", started: true }, false)).toThrow();
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
    for (const provider of ["aol", "gmx", "maildotcom", "icloud", "tuta"]) {
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
      "discord", "telegram", "instagram", "signal", "whatsapp", "x", "messenger",
    ]);
    expect(launch.filter((app) => app.section === "email").map((app) => app.id)).toEqual([
      "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud", "tuta",
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
      expect.objectContaining({ id: "signal", setupEligible: false }),
      expect.objectContaining({ id: "whatsapp", setupEligible: false }),
      expect.objectContaining({ id: "x", setupEligible: false }),
      expect.objectContaining({ id: "messenger", setupEligible: false }),
      expect.objectContaining({ id: "gmail", setupEligible: false }),
      expect.objectContaining({ id: "outlook", setupEligible: false }),
      expect.objectContaining({ id: "proton", setupEligible: false }),
      expect.objectContaining({ id: "yahoo", setupEligible: false }),
      expect.objectContaining({ id: "aol", setupEligible: false }),
      expect.objectContaining({ id: "gmx", setupEligible: false }),
      expect.objectContaining({ id: "maildotcom", setupEligible: false }),
      expect.objectContaining({ id: "icloud", setupEligible: false }),
      expect.objectContaining({ id: "tuta", setupEligible: false }),
    ]);
    // Superseded by owner ruling 2026-08-05: the stale later-only Slack and
    // LinkedIn specs must not silently re-enter the active home catalog.
    expect(apps.filter((app) => app.visibility === "later")).toEqual([]);
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

  const claimStateRs = readFileSync(
    new URL("../../osl-hub/src/claim_state.rs", import.meta.url),
    "utf8",
  );

  /**
   * The claim state's own derivation, transcribed from `claim_state.rs`
   * `derived_claim` + `claim_for`. It is short on purpose: if it stops being
   * short, the Rust derivation has grown a special case and this test should be
   * the thing that notices.
   */
  const derivedClaim: Record<string, string> = {
    ExternallyBlocked: "externallyBlocked",
    BuiltNeverProvenLive: "experimental",
    ProvenLiveWithReceipt: "experimental",
    MeasuredAndRefused: "comingSoon",
    NotBuilt: "comingSoon",
  };
  const matrixCeiling: Record<string, string> = {
    NoCapabilityClaim: "comingSoon",
    ExternallyBlocked: "externallyBlocked",
  };
  /** The capability-strength order; `externallyBlocked` is deliberately absent. */
  const strength = ["noClaim", "comingSoon", "experimental", "beta", "available"];

  // Extractor guards THROW rather than `expect`: a missing anchor means this
  // test could not run at all, which is a different thing from a claim being
  // wrong, and it is not an assertion about source text either way.
  function rustField(rustSurface: string, field: string): string {
    const row = claimStateRs.split("SurfaceClaim {")
      .find((block) => block.includes(`surface: Surface::${rustSurface},`));
    if (!row) throw new Error(`claim_state.rs has no row for ${rustSurface}`);
    const value = new RegExp(`${field}: (?:CarrierEvidence|DeliveryEvidence|MatrixPosition)::(\\w+),`)
      .exec(row);
    if (!value) throw new Error(`no ${field} for ${rustSurface}`);
    return value[1];
  }

  function rustBlockers(rustSurface: string): string[] {
    const row = claimStateRs.split("SurfaceClaim {")
      .find((block) => block.includes(`surface: Surface::${rustSurface},`)) as string;
    const list = /blockers: &\[([\s\S]*?)\],/.exec(row);
    return list ? [...list[1].matchAll(/ClaimBlocker::(\w+)/g)].map((m) => m[1]) : [];
  }

  /** `claim_state.rs` `claim_for`, re-implemented so a drift in Rust shows up here. */
  function rustClaim(rustSurface: string): string {
    if (rustBlockers(rustSurface).some((b) => b === "OpenSecurityFinding" || b === "UnknownRecheckRequired")) {
      return "noClaim";
    }
    const carrier = rustField(rustSurface, "carrier");
    const delivery = rustField(rustSurface, "delivery");
    let derived: string;
    if (carrier === "NoCarrierByConstruction") {
      derived = delivery === "ProvenLiveBothWays" ? "beta" : "comingSoon";
    } else if (carrier === "ProvenLiveWithReceipt" && delivery === "ProvenLiveBothWays") {
      derived = "beta";
    } else {
      derived = derivedClaim[carrier];
    }
    if (!derived) throw new Error(`unmapped CarrierEvidence ${carrier} for ${rustSurface}`);
    const ceiling = matrixCeiling[rustField(rustSurface, "matrix")];
    if (!ceiling) return derived;
    // Incomparable authorities earn no claim; comparable ones take the weaker.
    if (strength.includes(derived) !== strength.includes(ceiling)) return "noClaim";
    return strength.indexOf(derived) <= strength.indexOf(ceiling) ? derived : ceiling;
  }

  function rustSupportLevel(rustId: string): string {
    const manifest = nativeAppsRs.split("NativeAppManifest {")
      .find((block) => block.includes(`id: NativeAppId::${rustId},`));
    expect(manifest, `native_apps.rs has no manifest for ${rustId}`).toBeTruthy();
    const level = /adapter_support: SupportLevel::(\w+),/.exec(manifest as string);
    expect(level, `no adapter_support for ${rustId}`).toBeTruthy();
    return (level as RegExpExecArray)[1];
  }

  it("reads a support level per app, and they are not all the same", () => {
    const levels = ["Discord", "Telegram", "Instagram", "Signal", "Whatsapp", "Outlook", "X"].map(rustSupportLevel);
    expect(levels).toHaveLength(7);
    expect(new Set(levels).size).toBeGreaterThan(1);
  });

  it("never claims more than Rust does", async () => {
    const rustSurfaceFor: Record<string, string> = {
      discord: "Discord", telegram: "Telegram", instagram: "Instagram", signal: "Signal",
      whatsapp: "Whatsapp", outlook: "OutlookDesktop", x: "X",
    };
    const catalog = await loadNativeApps();
    expect(catalog.length).toBe(Object.keys(rustSurfaceFor).length);
    for (const app of catalog) {
      const surface = rustSurfaceFor[app.id];
      expect(app.supportStatus, `${app.id} disagrees with claim_state.rs`).toBe(rustClaim(surface));
      expect(app.carrierEvidence, `${app.id} evidence disagrees with claim_state.rs`)
        .toBe(rustField(surface, "carrier").replace(/^./, (c) => c.toLowerCase()));
      // Every row ships its reason. A badge with nothing behind it is how two
      // different evidence states become one state to a reader.
      expect(app.claimNote.length, `${app.id} has no reason line`).toBeGreaterThan(20);
      expect(app.statusPage.generatedLabel, `${app.id} direct status label disagrees with generated tile label`)
        .toBe(nativeAppGeneratedLabel(app.supportStatus));
      expect(app.statusPage.explanation, `${app.id} status page explanation diverged from claim note`)
        .toBe(app.claimNote);
      expect(app.statusPage.capability, `${app.id} status page data does not name the real capability`)
        .toMatch(/\bcapability\b/u);
    }

    // The evidence is per surface and is NOT one value stamped on everything --
    // asserted over the catalog the app actually loads, not over the Rust
    // source, so it executes rather than reading text.
    expect(new Set(catalog.map((app) => app.carrierEvidence)).size).toBeGreaterThan(1);
    expect(new Set(catalog.map((app) => app.claimNote)).size).toBe(catalog.length);
  });

  /**
   * THE PUBLIC CLAIM FLOOR, checked at the frontend boundary.
   *
   * This used to assert that NO app may claim `provenLiveWithReceipt`, on the
   * premise that `carry-receipts/` did not exist at all. That premise was true
   * when it was written and became FALSE on 2026-08-05, when Telegram earned
   * the first live carry receipt this project has ever held.
   *
   * A floor stated as an absolute ("nobody has one") stops being a floor the
   * moment somebody legitimately does -- it can then only be satisfied by
   * un-earning the receipt. So the floor is now stated as the property it was
   * always meant to enforce: **a surface may claim a receipt IF AND ONLY IF a
   * usable one exists on disk for it.** That refuses exactly what the absolute
   * refused (a claim with nothing behind it) and additionally refuses the
   * inverse the absolute could not see -- a real receipt the catalogue fails
   * to report.
   *
   * v1 receipts do NOT count: `verify_receipt_bytes` rates them `Stale` BY
   * NAME, which is precisely how "Telegram is already proven" survived as a
   * stale claim in the plan for weeks.
   */
  it("claims a live carry receipt if and only if a current-schema one exists", async () => {
    const receiptsDir = new URL("../../osl-hub/carry-receipts/", import.meta.url);
    // D-240: THIS IS A WEAKER PREDICATE THAN THE VERIFIER, AND IT IS NAMED FOR
    // WHAT IT MEASURES.
    //
    // It used to be called `usableReceipt` and its failures said "usable". It
    // reads ONE field. `verify_receipt` reads eighteen -- the seam-contract
    // binding, the adapter hash, byte_exact, enter_sent, the recovered payload,
    // the element count. A receipt with `byte_exact: false` still has
    // `"schema": "osl-live-carry-receipt-v2"`, so this function calls it present
    // and current while the Rust gate rates the same bytes `Invalid`. That gap
    // IS D-240, one layer out, and the honest fix here is to stop borrowing the
    // verifier's word for it rather than to grow a second verifier in
    // TypeScript -- a second answer to the one question the receipt exists to
    // answer is exactly what went wrong.
    //
    // SOUNDNESS IS DECIDED IN RUST AND NOWHERE ELSE:
    // `claim_state::tests::the_carrier_receipt_census_is_computed_and_states_the_truth`
    // and `native_apps::tests::no_native_app_is_published_above_coming_soon_...`
    // both go red on a receipt this function would still let through. What THIS
    // test owns is the frontend contract: the catalogue may not claim a receipt
    // that is not on disk in the current schema, and may not fail to report one
    // that is.
    const receiptFileIsCurrentSchema = (id: string): boolean => {
      try {
        const raw = readFileSync(new URL(`${id}.json`, receiptsDir), "utf8");
        return JSON.parse(raw).schema === "osl-live-carry-receipt-v2";
      } catch {
        return false;
      }
    };

    const catalog = await loadNativeApps();
    for (const app of catalog) {
      const earned = receiptFileIsCurrentSchema(app.id);
      expect(
        app.carrierEvidence === "provenLiveWithReceipt",
        earned
          ? `${app.id} has a current-schema receipt on disk but the catalogue does not report it`
          : `${app.id} claims a receipt that does not exist`,
      ).toBe(earned);
      // A receipt is EVIDENCE FOR a label, not permission to move one: an
      // earned receipt still may not promote a surface on its own.
      expect(["beta", "available"], `${app.id} claims capability with no receipt`)
        .not.toContain(app.supportStatus);
    }

    const promoted = {
      id: "telegram", displayName: "Telegram", availability: "installed",
      supportStatus: "beta", carrierEvidence: "builtNeverProvenLive",
      deliveryEvidence: "neverProvenLive", claimBlockers: [],
      claimNote: "Telegram works.",
      statusPage: {
        capability: "carrier capability is wired but not live-proven",
        generatedLabel: "Beta",
        explanation: "Telegram works.",
      },
      protectedMode: "unavailable",
      isolatedProfileAvailable: true, supportsOverlay: false,
    };
    expect(() => parseNativeApps([promoted])).toThrow();
    // And the same row without the promotion is accepted, so the refusal is
    // measuring the claim and not the shape.
    expect(parseNativeApps([{
      ...promoted,
      supportStatus: "comingSoon",
      statusPage: { ...promoted.statusPage, generatedLabel: "Coming later" },
    }])).toHaveLength(1);
  });
});
