import { describe, expect, it } from "vitest";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionBody(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start).toBeGreaterThan(-1);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("Discord QA renderer/backend state boundary", () => {
  it("keeps the QA header available while the Discord route is selected before renderer host recovery", () => {
    const controls = functionBody("nativeDiscordHeaderControls", "trustedHeader");

    expect(controls).toContain(
      'const discordQaRoute = discordQaShell && activeHomeAppId === "discord"',
    );
    expect(controls).toContain(
      'activeNativeHostId !== "discord" && !discordQaRoute',
    );
    expect(controls).toContain('id="discord-qa-toggle-composer"');
    expect(controls).toContain('id="discord-qa-transcript-visibility"');
  });

  it("bypasses the setup guide header from QA route identity without waiting for renderer host recovery", () => {
    const header = functionBody("trustedHeader", "homeHeader");
    const guideGuard = header.slice(0, header.indexOf("const localProtection"));

    expect(guideGuard).toContain(
      '!(discordQaShell && activeHomeAppId === "discord")',
    );
    expect(guideGuard).not.toContain(
      'discordQaShell && activeNativeHostId === "discord"',
    );
  });

  it("reconciles a backend-hosted Discord window before the one-click encrypted send", () => {
    const runStart = source.indexOf("async function runDiscordQaOneClick()");
    const send = source.indexOf(
      "const qaProbe = await runNativeDiscordHeadlessQa()",
      runStart,
    );
    expect(runStart).toBeGreaterThan(-1);
    expect(send).toBeGreaterThan(runStart);
    const beforeSend = source.slice(runStart, send);
    expect(beforeSend).toContain("await ensureDiscordQaNativeHost()");
    const reconcileStart = source.indexOf("async function ensureDiscordQaNativeHost(");
    const composerStart = source.indexOf("async function openDiscordQaComposer(");
    expect(reconcileStart).toBeGreaterThan(-1);
    const reconcile = source.slice(reconcileStart, composerStart);
    expect(reconcile).toContain("resizeNativeAppWindow()");
    expect(reconcile).toContain('"Reconcile hosted Discord QA window"');
    expect(reconcile).toContain('activeNativeHostId = "discord"');
    expect(reconcile).toContain('activeNativeHostMode = "existingSession"');
    expect(reconcile).not.toMatch(
      /activeNativeHostId\s*===\s*"discord"[\s\S]*?return;/u,
    );
    expect(reconcile.indexOf("resizeNativeAppWindow()"))
      .toBeLessThan(reconcile.indexOf('activeNativeHostId = "discord"'));
  });

  it("commits a verified late native-host result instead of discarding it after the renderer deadline", () => {
    expect(source).toContain("const hostOperation = hostNativeAppWindow(appId, requestedMode, discordTakeover)");
    expect(source).toContain("void hostOperation.then((lateResult) => {");
    expect(source).toContain("if (!rendererHostDeadlinePassed");
    expect(source).toContain('lateResult.mode !== "existingNativeCompanion"');
    expect(source).toContain('activeNativeHostMode = "existingSession"');
    expect(source).toContain("discordQaShellStarted = true");
    expect(source).toContain("void openDiscordQaComposer()");
  });

  it("starts the QA route without racing the composer against unrelated support IPC", () => {
    const bootstrapStart = source.indexOf("async function bootstrap()");
    const supportJoin = source.indexOf(
      "void Promise.all([servicesRequest, nativeAppsRequest",
      bootstrapStart,
    );
    const qaStart = source.indexOf("void startDiscordQaShell()", bootstrapStart);

    expect(bootstrapStart).toBeGreaterThan(-1);
    expect(qaStart).toBeGreaterThan(bootstrapStart);
    expect(supportJoin).toBeGreaterThan(qaStart);
    expect(source).toContain("void openDiscordQaComposerAfterHostReady()");
    expect(source).toContain('discordQaHostState !== "hosted"');
    expect(source).not.toContain("scheduleDiscordQaComposerAutoOpen()");
  });
});
