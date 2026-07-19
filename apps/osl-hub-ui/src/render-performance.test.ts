import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("bounded desktop rendering", () => {
  it("mounts the desktop shell once and patches a memoized keyed surface", () => {
    const render = functionSource("renderWorkspace", "appLauncherStrip");
    expect(render).toContain('root.querySelector<HTMLElement>("#workspace-render-surface")');
    expect(render).toContain('id="workspace-render-surface"');
    expect(render).toContain("lastWorkspaceMarkup === markup");
    expect(render).toContain("nextViewKey === lastWorkspaceViewKey");
    expect(render).toContain("surface.innerHTML = markup");
    expect(render).not.toContain("root.innerHTML = markup");
  });

  it("preserves safe same-view fields and focus without copying passwords or files", () => {
    const capture = functionSource("captureWorkspaceFocus", "restoreWorkspaceFocus");
    const restore = functionSource("restoreWorkspaceFocus", "containBackgroundFailure");
    expect(capture).toContain('field.type !== "password"');
    expect(capture).toContain('field.type !== "file"');
    expect(restore).toContain("setSelectionRange");
    expect(restore).toContain("focus({ preventScroll: true })");
  });
});

describe("truthful bounded startup", () => {
  it("gates the UI on the local security core and offers a real retry", () => {
    const recovery = functionSource("showBootstrapRecovery", "usableBootCore");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    expect(recovery).toContain("Couldn’t open OSL");
    expect(recovery).toContain('id="boot-retry"');
    expect(recovery).toContain("void bootstrap()");
    expect(recovery).not.toContain("qaDiagnostic");
    expect(recovery).not.toContain("<small>");
    expect(bootstrap).toContain('withNativeDeadline(loadCoreIntegration(), "Start OSL", bootCoreDeadlineMs)');
    expect(bootstrap).toContain("if (!usableBootCore(coreIntegration))");
    expect(bootstrap).not.toContain("Opening local workspace");
  });

  it("loads supporting app data behind bounded noncritical requests", () => {
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    const readinessGate = bootstrap.indexOf("if (!usableBootCore(coreIntegration))");
    const preferencesStart = bootstrap.indexOf("loadOnboardingPreferences()");
    const supportStart = bootstrap.indexOf("loadLinkedServices()");
    expect(readinessGate).toBeGreaterThanOrEqual(0);
    expect(preferencesStart).toBeGreaterThan(readinessGate);
    expect(supportStart).toBeGreaterThan(readinessGate);
    expect(bootstrap).toContain('withNativeDeadline(loadLinkedServices(), "Load apps", bootSupportDeadlineMs)');
    expect(bootstrap).toContain('savedAccountMode === "use"');
    expect(bootstrap).toContain('withNativeDeadline(loadNativeApps(), "Load selected Windows apps", bootSupportDeadlineMs)');
    expect(bootstrap).toContain('savedAccountsReady');
    expect(bootstrap).toContain('withNativeDeadline(loadFirefoxStatus(), "Check selected Firefox profile", bootSupportDeadlineMs)');
    expect(bootstrap).not.toContain("const mullvadRequest =");
    expect(bootstrap).not.toContain("const browserImportsRequest =");
    expect(bootstrap).toContain('withNativeDeadline(loadHubLicenseState(), "Load plan", bootSupportDeadlineMs)');
    expect(bootstrap).toContain("renderNow();");
    expect(bootstrap).toContain('onboardingRoute === "browser") void refreshBrowserImportReadiness()');
    expect(bootstrap).toContain('onboardingRoute === "mullvad") void refreshMullvadSetup()');
    expect(bootstrap).toContain("Promise.all([servicesRequest, nativeAppsRequest, firefoxRequest, licenseRequest])");
    expect(bootstrap).toContain("if (nativeCatalog && isCompleteNativeCatalog(nativeCatalog))");
    expect(bootstrap).not.toContain("nativeAppsReady");
    expect(bootstrap).not.toContain("currentMullvadStatus");
  });
});
