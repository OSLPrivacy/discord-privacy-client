import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const source = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");

function between(start: string, end: string): string {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + start.length);
  expect(from, `${start} should exist`).toBeGreaterThanOrEqual(0);
  expect(to, `${end} should follow ${start}`).toBeGreaterThan(from);
  return source.slice(from, to);
}

describe("existing Discord takeover consent", () => {
  it("probes first and only ever quits after an affirmative click", () => {
    // Closing a client the operator is using is destructive-adjacent: the
    // read-only probe decides whether there is anything to consent to, and
    // every non-affirmative path -- refusal, dismissal, a probe that failed --
    // must land on the borrow OSL has always done.
    const consent = between(
      "async function requestedNativeTakeover(",
      "async function openNativeHostedApp(",
    );
    expect(consent.indexOf("nativeAppTakeoverRequiresConsent(appId)"))
      .toBeLessThan(consent.indexOf("window.confirm("));
    expect(consent).toContain('if (appId !== "discord" || mode !== "existingSession") return "borrowExisting";');
    expect(consent).toContain('if (running === null) return "borrowExisting";');
    expect(consent).toContain('if (!running) return "quitAndRelaunch";');
    expect(consent).toContain('return accepted ? "quitAndRelaunch" : "borrowExisting";');
    expect(consent).toContain("close your running ${name} and reopen it inside OSL");
    expect(consent).toContain("Your account and your conversations are untouched");
  });

  it("offers exactly one borrow retry when the client refuses to close", () => {
    const host = between(
      "async function openNativeHostedApp(",
      "function browserAccountModeForLaunch()",
    );
    expect(host).toContain("const discordTakeover = await requestedNativeTakeover(appId, requestedMode, app.displayName);");
    expect(host).toContain("hostNativeAppWindow(appId, requestedMode, discordTakeover)");
    const refused = host.indexOf('result.reason === "existingSessionQuitRefused"');
    expect(refused).toBeGreaterThan(-1);
    const recovery = host.slice(refused);
    expect(recovery.indexOf("window.confirm("))
      .toBeLessThan(recovery.indexOf('hostNativeAppWindow(appId, requestedMode, "borrowExisting")'));
    expect(host.match(/hostNativeAppWindow\(appId, requestedMode, "borrowExisting"\)/gu)).toHaveLength(1);
    expect(recovery).toContain("did not close, most likely because it is set to keep running in its tray");
  });

  it("keeps the caller-bug takeover refusal out of the operator-facing surface", () => {
    // `takeoverNotPermitted` can only be reached by asking for a takeover the
    // backend has no contract for, which the request path already prevents.
    expect(source).not.toContain("takeoverNotPermitted");
  });
});
