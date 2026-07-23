import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

function functionSource(source: string, name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("home workspace hierarchy", () => {
  const source = readRelative("./main.ts");
  const styles = readRelative("./styles.css");
  const home = functionSource(source, "workspaceContent", "peopleListMarkup");

  it("keeps a radically simple tile grid without a persistent friends rail", () => {
    const apps = home.indexOf('class="app-grid');

    expect(apps).toBeGreaterThanOrEqual(0);
    expect(home).not.toContain('class="friends-rail');
    expect(home).not.toContain("home-walkthrough");
    expect(home).not.toContain("osl-chat-tutorial");
    expect(home).toContain("osl-chats");
    expect(home).toContain("osl-notes");
    expect(home).toContain('name: "Scrub"');
    expect(home).not.toContain('name: "Activity"');
    expect(home).not.toContain('name: "Servers"');
    expect(home).toMatch(/class="[^"]*\bhome-dashboard\b/);
    expect(home).toMatch(/class="[^"]*\bhome-primary\b/);
    expect(home).toContain('class="home-profile-dock"');
  });

  it("uses compact square app launchers instead of a service dropdown", () => {
    expect(home).toMatch(/class="[^"]*\bapp-tile\b/);
    expect(home).toContain("homeAppLogo(app)");
    expect(source).toContain("providerLogo(app.provider)");
    expect(styles).toMatch(/\.app-logo-plate[^}]*aspect-ratio\s*:\s*1(?:\s*\/\s*1)?/s);
    expect(source).not.toContain('id="service-picker"');
    expect(home).toContain('id="home-app-${app.id}"');
    expect(source).not.toContain("Choose an app");
  });

  it("keeps default Home focused on usable modules without removing edit controls", () => {
    expect(home).toContain('module.available ? "" : "disabled"');
    expect(home).toContain("const oslSection = oslTiles ?");
    expect(home).toContain("${oslSection}");
    expect(home).toContain("data-tile-move");
    expect(home).toContain("data-tile-toggle");
    expect(home).toContain("data-edit-home");
  });

  it("removes tile and logo-plate chrome while retaining visible keyboard focus", () => {
    expect(styles).toMatch(/\.app-tile\s*\{[^}]*border:\s*0[^}]*background:\s*transparent/s);
    expect(styles).toMatch(/\.app-tile:hover\s*\{[^}]*border-color:\s*transparent[^}]*background:\s*transparent/s);
    expect(styles).toMatch(/\.app-logo-plate\s*\{[^}]*border:\s*0[^}]*background:\s*transparent[^}]*box-shadow:\s*none/s);
    expect(styles).toMatch(/\.app-tile > button:first-child:focus-visible\s*\{[^}]*outline:\s*3px solid var\(--brand\)/s);
  });

  it("puts only configured OSL profiles in the trusted top strip", () => {
    const strip = functionSource(source, "appLauncherStrip", "simpleDeviceStatusMarkup");
    expect(strip).toContain("configuredTopStripApps(homeAppsFromServices(services), homeTileOrder)");
    expect(strip).not.toContain("installedNative");
    expect(strip).not.toContain("nativeApps.filter");
  });

  it("forces hard corners across OSL-owned surfaces", () => {
    expect(styles).toContain("* { border-radius: 0 !important; }");
  });

  it("renders provider-specific email catalog entries as Home app buttons", () => {
    expect(home).toContain("homeAppsFromServices(services)");
    expect(home).toMatch(/homeApps\.map\(\(app\)[\s\S]*?data-home-app="\$\{app\.id\}"/);
    expect(source).toMatch(/function homeAppLogo[\s\S]*?app\.provider \? providerLogo\(app\.provider\)/);
    for (const provider of ["gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud"]) {
      expect(styles).toContain(`.app-tile > button[data-home-app="${provider}"], .app-launcher[data-home-app="${provider}"]`);
    }
    expect(styles).toMatch(/\.app-launcher\s*\{[^}]*color:\s*var\(--service, var\(--muted\)\)/s);
    expect(styles).toMatch(/\.app-launcher\.active\s*\{[^}]*color:\s*var\(--service, var\(--text\)\)/s);
  });

  it("does not repeat the old Privacy OSL Privacy subtitle beneath the OSL mark", () => {
    expect(source).not.toContain("<small>Privacy OSL Privacy</small>");
  });

  it("uses the official OSL mark alone in the Home corner", () => {
    const homeHeader = functionSource(source, "homeHeader", "settingsButtonMarkup");
    expect(homeHeader).toContain('class="home-logo-button"');
    expect(homeHeader).toContain('src="${oslVectorLogoUrl}"');
    expect(homeHeader).not.toContain("OSL Privacy</strong>");
    expect(homeHeader).not.toContain('src="${oslLogoUrl}"');
    expect(styles).toMatch(/\.home-logo-button\s*\{[^}]*width:\s*44px[^}]*height:\s*44px/s);
    expect(styles).toMatch(/\.logo-treatment\s*\{[^}]*filter:[^}]*drop-shadow[^}]*drop-shadow/s);
    expect(functionSource(source, "trustedHeader", "homeHeader")).not.toContain("home-brand-mark");
  });

  it("uses the same crisp vector logo treatment throughout the visible app chrome", () => {
    expect(source.match(/src="\$\{oslVectorLogoUrl\}"/g)?.length ?? 0).toBeGreaterThanOrEqual(4);
    expect(source.match(/class="[^"]*logo-treatment[^"]*"/g)?.length ?? 0).toBeGreaterThanOrEqual(4);
    expect(styles).toMatch(/\.signin-logo\s*\{[^}]*width:\s*64px[^}]*height:\s*64px/s);
    expect(styles).toMatch(/\.command-brand \.osl-logo\s*\{[^}]*width:\s*34px[^}]*height:\s*34px/s);
  });

  it("keeps friends, notifications, settings, and profile at the screen edges", () => {
    const homeHeader = functionSource(source, "homeHeader", "settingsButtonMarkup");
    expect(homeHeader).toContain("data-open-friends");
    expect(homeHeader).toContain("data-notification-settings");
    expect(homeHeader).toContain('data-route="settings"');
    expect(styles).toMatch(/\.home-command-bar\s*\{[^}]*padding:\s*0 24px[^}]*justify-content:\s*space-between/s);
    expect(styles).toMatch(/\.home-profile-dock\s*\{[^}]*position:\s*fixed[^}]*right:\s*26px[^}]*bottom:\s*24px/s);
  });
});

describe("home interaction regressions", () => {
  const source = readRelative("./main.ts");
  const styles = readRelative("./styles.css");
  const home = functionSource(source, "workspaceContent", "peopleListMarkup");

  it("binds every friends entry point without duplicate IDs", () => {
    expect(source).toContain('data-open-friends');
    expect(source).toMatch(/querySelectorAll(?:<[^>]+>)?\("\[data-open-friends\]"\)/);
    expect(source).not.toContain('id="open-friends"');
  });

  it("guards the friends modal against rapid repeated activation", () => {
    const binding = functionSource(source, "bindWorkspace", "ttlSeconds");
    expect(binding).toMatch(/if\s*\(dialog\s*&&\s*!dialog\.open\)\s*\{?\s*dialog\.showModal\(\)/);
  });

  it("registers one coalesced resize hook only for an active native host", () => {
    expect(source.match(/window\.addEventListener\("resize"/g) ?? []).toHaveLength(1);
    const binding = functionSource(source, "bindWorkspace", "ttlSeconds");
    expect(binding).not.toContain('window.addEventListener("resize"');
    expect(binding).not.toContain('document.addEventListener("keydown"');
    expect(source).toMatch(/function scheduleNativeHostRealignment[\s\S]*?if \(\(!activeNativeHostId && !activeDefaultBrowserCompanion && !mullvadWindowHosted\) \|\| nativeHostResizeFrame\) return;[\s\S]*?requestAnimationFrame[\s\S]*?validateNativeSurfaces\(\)/);
    expect(source).toContain('window.addEventListener("resize", scheduleNativeHostRealignment)');
    expect(source).toContain("desktopWindow.onMoved(scheduleNativeHostRealignment)");
    expect(source).toContain("desktopWindow.onResized(scheduleNativeHostRealignment)");
    expect(source).toContain("desktopWindow.onFocusChanged");
  });

  it("coalesces background state updates into one paint", () => {
    expect(source).toContain("new FrameRenderScheduler");
    expect(source).toMatch(/function render\(\): void \{\s*renderScheduler\.request\(\);\s*\}/);
    expect(source).toMatch(/function scheduleBackgroundRender\(\): void \{\s*render\(\);\s*\}/);
    expect(source).toMatch(/function renderWhenIdle[\s\S]*?scheduleBackgroundRender\(\)/);
    expect(source).toContain("lastWorkspaceMarkup === markup");
    expect(source).toContain('root.querySelector<HTMLElement>("#workspace-render-surface")');
  });

  it("has no background polling or interval leak", () => {
    expect(source).not.toContain("setInterval(");
    expect(source).toContain('window.addEventListener("unhandledrejection"');
    expect(source).toContain("containBackgroundFailure");
    expect(source).toMatch(/function containBackgroundFailure[\s\S]*?showToast\("That action failed\. Nothing changed\."\)/);
    expect(source).not.toContain('window.addEventListener("unhandledrejection", (event) => { event.preventDefault(); showRenderRecovery(); });');
  });

  it("does not eagerly duplicate the friends list on Home", () => {
    expect(home).not.toContain('peopleListMarkup("home"');
    expect(source).toMatch(/function friendsDialogMarkup[\s\S]*?route !== "home" \|\| !friendsDialogOpen/);
  });

  it("uses concise empty Friends copy", () => {
    expect(source).toContain("<strong>No friends yet</strong><p>Add one with an invite.</p>");
    expect(source).not.toContain("No OSL friends yet");
    expect(styles).toMatch(/\.friends-rail-list \.empty-state\s*\{[^}]*border:\s*0/s);
  });

  it("bounds and pages the expanded friends view and caps visible scopes", () => {
    expect(source).toContain("const friendsDialogPageSize = 24");
    expect(source).toContain("const friendScopeRenderLimit = 16");
    expect(source).toContain('peopleListMarkup("manage", friendsDialogPageSize, pageStart)');
    expect(source).toContain('data-friends-page=');
    expect(source).toContain("person.whitelistedScopes.slice(0, friendScopeRenderLimit)");
  });

  it("keeps nicknames local and shows only proven friend connections", () => {
    expect(source).toContain('data-nickname-person=');
    expect(source).toContain("setHubFriendNickname(personId, input.value)");
    expect(source).not.toContain("Connected accounts");
    expect(source).not.toContain("None linked");
    expect(source).toContain("whitelistedScopes");
    expect(source).not.toContain("linkedInstagram");
  });

  it("uses one trusted browser companion only for web apps while Discord stays native", () => {
    const importedAppsStart = source.indexOf("const importedFirefoxHomeAppIds");
    const importedAppsEnd = source.indexOf("]);", importedAppsStart);
    const importedApps = source.slice(importedAppsStart, importedAppsEnd);
    const opening = functionSource(source, "openHomeAppFromLauncher", "startBackgroundInstall");
    expect(source).toContain("openEmbeddedHomeApp(app, services)");
    expect(source).toContain("setupEmbeddedHomeApp(app,");
    expect(source).not.toContain("Firefox workspace");
    expect(importedApps).toContain('"instagram"');
    expect(importedApps).toContain('"gmail"');
    expect(importedApps).not.toContain('"discord"');
    expect(opening).toMatch(/selectedNativeAppIntent\(app\.id\)[\s\S]*?if \(nativeIntent\)[\s\S]*?openNativeHostedApp/);
    expect(opening).toContain("defaultBrowserCompanionEligible(app.id)");
    expect(opening).toContain("openBrowserCompanionApp(app, service)");
    expect(opening.indexOf("if (nativeIntent)")).toBeLessThan(opening.indexOf("openBrowserCompanionApp(app, service)"));
    expect(opening).toContain("else if (app.linked)");
    expect(source).not.toContain("setup-needed");
    expect(home).not.toContain("<small>${module.state}</small>");
    expect(home).toContain('<span class="app-tile-copy"><strong>${escapeHtml(app.displayName)}</strong>${pending ? "<small>Opening…</small>" : ""}</span>');
    expect(home).toContain("Social</h2>");
    expect(home).toContain("Email</h2>");
    expect(home).toContain('aria-label="OSL tools"');
  });

  it("acknowledges app clicks immediately and bounds the profile refresh", () => {
    const binding = functionSource(source, "bindWorkspace", "openHomeAppFromLauncher");
    const opening = functionSource(source, "openHomeAppFromLauncher", "startBackgroundInstall");
    expect(binding).toMatch(/appLaunchPendingId = appId;[\s\S]*?renderNow\(\);[\s\S]*?openHomeAppFromLauncher/);
    expect(opening).toContain('withNativeDeadline(loadLinkedServices(), "Refresh apps", 450)');
    expect(opening).toContain("intent !== navigationIntentEpoch");
  });

  it("reopens the linked isolated profile instead of creating one per click", () => {
    const opening = functionSource(source, "openHomeAppFromLauncher", "startBackgroundInstall");
    expect(opening).toContain("else if (app.linked)");
    expect(opening).toContain("void openEmbeddedApp(app, service)");
    expect(opening).toContain("void setupEmbeddedApp()");
    expect(opening).not.toContain('app.linked && savedAccountMode !== "clean"');
    expect(opening).not.toContain("setupEmbeddedApp(true)");
  });

  it("loads the new identity's friend profile before first Home render", () => {
    const completion = functionSource(source, "completeOnboarding", "bindImportForm");
    expect(completion).toContain("await refreshIdentityScopedState()");
    expect(completion.indexOf("await refreshIdentityScopedState()"))
      .toBeLessThan(completion.indexOf("render()"));
  });

  it("preserves an exact email-provider tile in its isolated embedded profile", () => {
    const binding = functionSource(source, "bindWorkspace", "ttlSeconds");
    const opening = functionSource(source, "openServiceRoute", "persistServiceGuideState");
    expect(binding).toContain("openServiceRoute(service, app.provider, app.id, true)");
    expect(opening).toContain("activeHomeAppId = appId");
    expect(source).toContain("setupEmbeddedHomeApp(app,");
    expect(source).toContain("openEmbeddedHomeApp(app, services)");
  });

  it("closes the owned service surface before top-level navigation", () => {
    const closing = functionSource(source, "closeActiveServiceSurface", "toggleLocalProtectedSheet");
    expect(closing).toContain("closeEmbeddedServiceHost()");
    expect(closing).toContain("activeEmbeddedHost = null");
    expect(closing).toContain("detachNativeAppWindow()");
    expect(closing).toContain("activeNativeHostId = null");
  });

  it("supports keyboard-operable tile edit controls", () => {
    expect(source).toContain("data-tile-move");
    expect(source).toContain("data-tile-toggle");
    expect(source).toContain("saveHomeTilePreferences");
  });

  it("preserves explicit native intent and never silently falls back to the web", () => {
    const nativeOpening = functionSource(source, "openNativeHostedApp", "setupEmbeddedApp");
    const nativePresentation = functionSource(source, "focusActiveNativeCompanion", "renderOnboarding");
    expect(source).toContain("const requestedMode = nativeSessionModeForApp(appId)");
    expect(source).toContain("hostNativeAppWindow(appId, requestedMode)");
    expect(source).toContain("selectedNativeAppIntent(app.id)");
    expect(source).toContain("openNativeHostedApp(app, service, nativeIntent)");
    expect(source).toContain("installNativeApp(appId)");
    expect(source).toContain("openEmbeddedHomeApp(app, services)");
    expect(source).toContain("setupEmbeddedHomeApp(app,");
    expect(source).toContain('candidate.availability === "installed"');
    expect(source).not.toContain("openSafeEmbeddedFallback");
    expect(source).not.toContain("opened in a separate OSL web profile; the normal app stayed open");
    expect(source).toContain("await detachNativeAppWindow().catch(() => undefined)");
    expect(source).toContain('const hostDeadlineMs = appId === "discord" && requestedMode === "dedicated" ? 190_000 : 30_000');
    expect(source).toContain("hostNativeAppWindow(appId, requestedMode)");
    expect(source).toContain("hostDeadlineMs");
    expect(source).toContain("activeNativeHostMode === requestedMode");
    expect(source).toContain("outlookSessionModeChoices()");
    expect(source).toContain("savedNativeApps.has(nativeId)");
    expect(source).toContain('catalogApp?.availability === "installed" && catalogApp.isolatedProfileAvailable');
    expect(source).toContain('finishNativeAccountChoice("telegram")');
    expect(source).not.toContain("function selectedNativeApps");
    expect(source).toContain(">Use existing account</button>");
    expect(source).toContain(">Use separate account</button>");
    expect(source).toContain('aria-label="Open Telegram"');
    expect(source).toContain('let telegramSessionMode: NativeSessionMode = "existingSession"');
    expect(source).toContain('let signalSessionMode: NativeSessionMode = "existingSession"');
    expect(source).toContain('let whatsappSessionMode: NativeSessionMode = "existingSession"');
    expect(source).toContain('storedTelegramMode === null ? "existingSession"');
    expect(source).toContain('storedSignalMode === null ? "existingSession"');
    expect(source).toContain('storedWhatsappMode === null ? "existingSession"');
    expect(source).toContain('data-signal-session-mode="dedicated"');
    expect(source).toContain('data-whatsapp-session-mode="dedicated"');
    expect(source).toContain('function separateNativeAccountAvailable');
    expect(source).toContain('supportedNativeAppIds.has(app.id as NativeAppId)');
    expect(source).toContain('A separate ${app.displayName} app account is unavailable');
    expect(source).toContain('["telegram", "signal", "whatsapp"].includes(activeHomeAppId)');
    expect(source).toContain('reason === "profileInitializationFailed"');
    expect(source).toContain("your normal ${name} is untouched");
    expect(nativeOpening).toMatch(/result\.status !== "hosted"[\s\S]*?activeNativeHostId = appId[\s\S]*?savedAccountMode = "use"[\s\S]*?savedNativeApps\.add\(appId\)[\s\S]*?persistSavedAccountPreferences\(\)/);
    expect(nativeOpening).toMatch(/focusActiveNativeCompanion\(\)[\s\S]*?detachNativeAppWindow\(\)/);
    expect(nativePresentation).toMatch(/focusNativeAppWindow\(\)[\s\S]*?status !== "focused"[\s\S]*?resizeNativeAppWindow\(\)[\s\S]*?status === "resized"/);
    expect(nativeOpening).not.toContain("openEmbeddedHomeApp");
    expect(nativeOpening).not.toContain("setupEmbeddedHomeApp");
  });

  it("realigns the native app in true fullscreen and windowed modes", () => {
    expect(source).toContain('event.key !== "F11"');
    expect(source).toContain("await appWindow.isFullscreen()");
    expect(source).toContain("await appWindow.setFullscreen(!fullscreen)");
    expect(source).toContain("if (activeNativeHostId) await resizeNativeAppWindow()");
    expect(source).toContain("await focusActiveNativeCompanion()");
    expect(source).toContain('id="native-companion-focus"');
    expect(source).toContain("async function reopenActiveNativeCompanion");
    expect(source).toMatch(/reopenActiveNativeCompanion[\s\S]*?focusActiveNativeCompanion\(\)[\s\S]*?detachNativeAppWindow\(\)[\s\S]*?openNativeHostedApp/);
    expect(source).toContain("Bring forward or reopen");
    expect(source).toContain("activeNativeHostMode === \"existingSession\"");
    expect(source).toContain("Native companion");
    expect(source).toContain("async function validateNativeSurfaces");
    expect(source).toContain('showToast(`${name} closed. Use Bring forward or reopen.`)');
    expect(source).toContain("await reopenActiveNativeCompanion()");
    expect(source).toContain('showToast(`${name} closed and could not be reopened safely.`)');
    expect(source).toContain('const reopened = await hostMullvadWithDeadline("Reopen Mullvad")');
    expect(source).toContain('showToast("Mullvad could not be reopened")');
  });

  it("never falls an explicit native choice back to WebView", () => {
    const nativeOpen = functionSource(source, "openNativeHostedApp", "setupEmbeddedApp");
    expect(nativeOpen).toContain("nativeHostFailureNotice = nativeHostFailureMessage(result.reason, app.displayName)");
    expect(nativeOpen).toContain("showToast(nativeHostFailureNotice)");
    expect(nativeOpen).toContain("serviceGuideStep = 0");
    expect(source).toContain('role="status">${escapeHtml(nativeHostFailureNotice)}');
    expect(nativeOpen).not.toContain("openEmbeddedHomeApp");
    expect(nativeOpen).not.toContain("setupEmbeddedHomeApp");
    expect(source).toContain("supportedNativeAppIds.has(app.id as NativeAppId)");
    expect(source).toContain("A separate ${app.displayName} app account is unavailable");
    expect(source).toContain('if (!nativeSessionModeConfirmed(nativeId)) return undefined;');
    expect(source).toContain('if (existingNativeSessionRequested(appId)) return nativeId;');
  });

  it("asks for native account mode once and persists later auto-open behavior", () => {
    const opening = functionSource(source, "openHomeAppFromLauncher", "startBackgroundInstall");
    const persistence = functionSource(source, "persistSavedAccountPreferences", "bindSavedAccountControls");
    expect(source).toContain('if (!nativeSessionModeConfirmed(nativeId)) return undefined;');
    expect(opening).toContain('supportedNativeAppIds.has(app.id as NativeAppId) && !nativeSessionModeConfirmed(app.id as NativeAppId)');
    expect(opening).toContain('openServiceRoute(service, app.provider, app.id, true)');
    expect(persistence).toContain('confirmedNativeSessionModesStorageKey');
    expect(source).toContain('data-native-mode-app');
    expect(source).toContain('Account opening');
  });

  it("refreshes profiles before routing and asks which local profile to open", () => {
    const binding = functionSource(source, "bindWorkspace", "ttlSeconds");
    expect(binding).toContain("services = await loadLinkedServices().catch(() => services)");
    expect(source).toContain("embeddedAccountsForHomeApp(app, services)");
    expect(source).toContain("serviceAccountPickerOpen = true");
    expect(source).toContain("data-service-account");
  });
});
