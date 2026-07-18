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

  it("keeps a radically simple tile grid and friends in their intended regions", () => {
    const apps = home.indexOf('class="app-grid');
    const friends = home.indexOf('class="friends-rail');

    expect(apps).toBeGreaterThanOrEqual(0);
    expect(friends).toBeGreaterThan(apps);
    expect(home).not.toContain("home-walkthrough");
    expect(home).not.toContain("osl-chat-tutorial");
    expect(home).toContain("osl-chats");
    expect(home).toContain("osl-notes");
    expect(home).toMatch(/class="[^"]*\bhome-dashboard\b/);
    expect(home).toMatch(/class="[^"]*\bhome-primary\b/);
    expect(home).toMatch(/<aside class="friends-rail"[^>]*aria-label(?:ledby)?=/);
  });

  it("uses compact square app launchers instead of a service dropdown", () => {
    expect(home).toMatch(/class="[^"]*\bapp-tile\b/);
    expect(home).toContain("homeAppLogo(app)");
    expect(source).toContain("providerLogo(app.provider)");
    expect(styles).toMatch(/\.app-logo-plate[^}]*aspect-ratio\s*:\s*1(?:\s*\/\s*1)?/s);
    expect(source).not.toContain('id="service-picker"');
    expect(source).not.toContain("Choose an app");
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
  });

  it("does not repeat the old Privacy OSL Privacy subtitle beneath the OSL mark", () => {
    expect(source).not.toContain("<small>Privacy OSL Privacy</small>");
  });

  it("gives only the Home brand a refined large square mark with compact balance", () => {
    const homeHeader = functionSource(source, "homeHeader", "settingsButtonMarkup");
    expect(homeHeader).toContain('class="home-brand home-brand-home"');
    expect(homeHeader).toContain('class="home-brand-mark"');
    expect(homeHeader).toContain('src="${oslVectorLogoUrl}"');
    expect(styles).toMatch(/\.home-brand-mark\s*\{[^}]*width:\s*56px[^}]*height:\s*56px[^}]*border:[^}]*background:[^}]*box-shadow:/s);
    expect(styles).toMatch(/\.home-brand-mark \.osl-logo\s*\{[^}]*width:\s*52px[^}]*height:\s*52px[^}]*filter:[^}]*drop-shadow/s);
    expect(styles).toMatch(/@media \(max-width: 620px\)[\s\S]*?\.home-brand-home \.osl-logo\s*\{[^}]*width:\s*48px[^}]*height:\s*48px/s);
    expect(functionSource(source, "trustedHeader", "homeHeader")).not.toContain("home-brand-mark");
  });

  it("keeps friends persistently visible on wide screens and responsive on compact screens", () => {
    expect(styles).toMatch(/\.home-dashboard[^}]*grid-template-columns\s*:[^;]*(?:minmax|fr)[^;]*(?:px|rem|clamp|minmax)/s);
    expect(styles).toMatch(/\.friends-rail\s*\{/);
    expect(styles).toMatch(/@media[^{}]*\(max-width:[^)]+\)[\s\S]*?\.home-dashboard[^}]*grid-template-columns\s*:\s*1fr/s);
  });
});

describe("home interaction regressions", () => {
  const source = readRelative("./main.ts");
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

  it("does not register resize work for removed embedded service surfaces", () => {
    expect(source.match(/window\.addEventListener\("resize"/g) ?? []).toHaveLength(0);
    const binding = functionSource(source, "bindWorkspace", "ttlSeconds");
    expect(binding).not.toContain('window.addEventListener("resize"');
    expect(binding).not.toContain('document.addEventListener("keydown"');
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

  it("does not eagerly duplicate the entire friends list", () => {
    expect(source).toContain('peopleListMarkup("home", 8)');
    expect(source).toMatch(/function friendsDialogMarkup[\s\S]*?route !== "home" \|\| !friendsDialogOpen/);
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

  it("uses isolated embedded app profiles without tile subtitles", () => {
    expect(source).toContain("openEmbeddedHomeApp(app, services)");
    expect(source).toContain("setupEmbeddedHomeApp(app,");
    expect(source).not.toContain("Firefox workspace");
    expect(source).not.toContain("launchFirefoxService(serviceId)");
    expect(source).not.toContain("setup-needed");
    expect(home).toContain("<small>${module.state}</small>");
    expect(home).toContain('<span class="app-tile-copy"><strong>${escapeHtml(app.displayName)}</strong>${pending ? "<small>Opening…</small>" : ""}</span>');
    expect(home).toContain("Social</h2>");
    expect(home).toContain("Email</h2>");
    expect(home).toContain("OSL</h2>");
  });

  it("acknowledges app clicks immediately and bounds the profile refresh", () => {
    const binding = functionSource(source, "bindWorkspace", "openHomeAppFromLauncher");
    const opening = functionSource(source, "openHomeAppFromLauncher", "runNativeAppAction");
    expect(binding).toMatch(/appLaunchPendingId = appId;[\s\S]*?renderNow\(\);[\s\S]*?openHomeAppFromLauncher/);
    expect(opening).toContain('withNativeDeadline(loadLinkedServices(), "Refresh apps", 450)');
    expect(opening).toContain("intent !== navigationIntentEpoch");
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
    const binding = functionSource(source, "bindWorkspace", "ttlSeconds");
    expect(binding).toContain("closeEmbeddedServiceHost()");
    expect(binding).toContain("activeEmbeddedHost = null");
  });

  it("supports keyboard-operable tile edit controls", () => {
    expect(source).toContain("data-tile-move");
    expect(source).toContain("data-tile-toggle");
    expect(source).toContain("saveHomeTilePreferences");
  });

  it("keeps native launchers explicit while ordinary tiles open embedded profiles", () => {
    expect(source).toContain("launchNativeApp(appId)");
    expect(source).toContain("installNativeApp(appId)");
    expect(source).toContain("openEmbeddedHomeApp(app, services)");
    expect(source).toContain("setupEmbeddedHomeApp(app,");
    expect(source).toContain('data-native-launch="${nativeApp.id}"');
    expect(source).toContain("Use existing account opens the installed app with its current login");
    expect(source).toContain("Use existing account</button>");
    expect(source).toContain("Start fresh</button>");
    expect(source).toContain('id="native-account-choice-existing"');
    expect(source).toContain('id="native-account-choice-new"');
    expect(source).toContain("!app.linked && installedNative");
    expect(source).toContain('savedAccountMode === "use" && savedNativeApps.has(installedNative.id)');
    expect(source).toContain('savedAccountMode !== "ask"');
  });

  it("refreshes profiles before routing and asks which local profile to open", () => {
    const binding = functionSource(source, "bindWorkspace", "ttlSeconds");
    expect(binding).toContain("services = await loadLinkedServices().catch(() => services)");
    expect(source).toContain("embeddedAccountsForHomeApp(app, services)");
    expect(source).toContain("serviceAccountPickerOpen = true");
    expect(source).toContain("data-service-account");
  });
});
