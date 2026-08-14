import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  SETUP_APP_IDS,
  SETUP_APP_INSTALLING,
  SETUP_APP_ROW_STATES,
  SETUP_APP_DETECTION_STATES,
  setupAppRows,
  setupAppsMarkup,
} from "./setup-apps";
import { WINDOWS_INSTALL_PRODUCTS, isWindowsInstallProduct } from "./services";
import type { NativeApp } from "./services";

/**
 * TASK 6810 — the row half of "install missing Windows apps".
 *
 * The engine that resolves, verifies, installs and re-detects is proved by
 * `apps/osl-hub/tests/task_6810_windows_app_install.rs`. This file proves the
 * other end of the same sentence: that a real row shows NOT DETECTED, then
 * Installing, then DETECTED, that Open exists only on a detected row, and that
 * a failed install lands back on NOT DETECTED carrying the backend's honest
 * reason and claiming nothing else.
 */

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const services = readFileSync(new URL("./services.ts", import.meta.url), "utf8");

/** Claims a row may never make once an install has failed or been cancelled. */
const FORBIDDEN_AFTER_FAILURE = ["queued", "Queued", "Installing", "DETECTED"];

function nativeApp(id: string, availability: NativeApp["availability"]): NativeApp {
  return {
    id,
    displayName: id,
    availability,
  } as unknown as NativeApp;
}

const CLEAN: NativeApp[] = SETUP_APP_IDS.map((id) => nativeApp(id, "installable"));

describe("TASK 6810 — the five products and the three row states", () => {
  it("keeps the pinned install list at exactly the five named products", () => {
    expect([...WINDOWS_INSTALL_PRODUCTS]).toEqual([
      "signal",
      "discord",
      "telegram",
      "whatsapp",
      "mullvad",
    ]);
    for (const product of WINDOWS_INSTALL_PRODUCTS) {
      expect(isWindowsInstallProduct(product)).toBe(true);
    }
    expect(isWindowsInstallProduct("notepad")).toBe(false);
  });

  it("keeps the merged page's four rows and Mullvad's own retained row", () => {
    // Owner ruling D4/D5 gave Mullvad its own retained onboarding page, so the
    // merged page keeps the four it owns and Mullvad keeps its card. Both rows
    // are driven by the same backend product list above.
    expect([...SETUP_APP_IDS]).toEqual(["signal", "discord", "telegram", "whatsapp"]);
    expect(source).toContain('installWindowsApp("mullvad")');
  });

  it("adds Installing as a state a row passes through, not a third detection", () => {
    expect([...SETUP_APP_DETECTION_STATES].sort()).toEqual(["DETECTED", "NOT DETECTED"]);
    expect([...SETUP_APP_ROW_STATES]).toEqual(["NOT DETECTED", SETUP_APP_INSTALLING, "DETECTED"]);
  });

  it("draws a clean profile as NOT DETECTED with a reason, an Install and no Open", () => {
    const rows = setupAppRows(CLEAN, new Set());
    expect(rows).toHaveLength(4);
    const markup = setupAppsMarkup(rows, () => "");
    for (const row of rows) {
      expect(row.state).toBe("NOT DETECTED");
      expect(row.detection).toBe("NOT DETECTED");
      expect(row.reason).not.toBe("");
      expect(row.installOffered).toBe(true);
      expect(row.openOffered).toBe(false);
      expect(row.enabled).toBe(false);
      expect(markup).toContain(`data-setup-app-install="${row.id}"`);
      expect(markup).not.toContain(`data-setup-app-open="${row.id}"`);
    }
    expect(markup).toContain('data-setup-app-state="NOT DETECTED"');
  });

  it("shows Installing on the pressed row only, and offers it no Install", () => {
    const rows = setupAppRows(CLEAN, new Set(), new Set(["discord"]));
    const discord = rows.find((row) => row.id === "discord");
    const signal = rows.find((row) => row.id === "signal");
    expect(discord?.state).toBe(SETUP_APP_INSTALLING);
    // The measurement underneath has not changed: nothing is on the PC yet.
    expect(discord?.detection).toBe("NOT DETECTED");
    expect(discord?.installOffered).toBe(false);
    expect(discord?.openOffered).toBe(false);
    expect(signal?.state).toBe("NOT DETECTED");
    expect(signal?.installOffered).toBe(true);

    const markup = setupAppsMarkup(rows, () => "");
    expect(markup).toContain('data-setup-app-state="Installing"');
    expect(markup).not.toContain('data-setup-app-install="discord"');
    expect(markup).toContain('data-setup-app-install="signal"');
  });

  it("turns DETECTED only for an installed app, and only then offers Open", () => {
    const catalogue = [
      nativeApp("signal", "installed"),
      nativeApp("discord", "installable"),
      nativeApp("telegram", "unavailable"),
      nativeApp("whatsapp", "installable"),
    ];
    const rows = setupAppRows(catalogue, new Set(["signal"]));
    const signal = rows.find((row) => row.id === "signal");
    expect(signal?.state).toBe("DETECTED");
    expect(signal?.openOffered).toBe(true);
    expect(signal?.installOffered).toBe(false);
    expect(signal?.reason).toBe("");
    expect(signal?.enabled).toBe(true);
    for (const row of rows.filter((candidate) => candidate.id !== "signal")) {
      expect(row.state).toBe("NOT DETECTED");
      expect(row.openOffered).toBe(false);
    }

    const markup = setupAppsMarkup(rows, () => "");
    expect(markup).toContain('data-setup-app-open="signal"');
    expect(markup).toContain('data-setup-app-state="DETECTED"');
    expect(markup).not.toContain('data-setup-app-open="discord"');
  });

  it("returns a failed install to NOT DETECTED carrying the backend's own reason", () => {
    const reason = "Discord was not installed: you cancelled before anything was installed.";
    const rows = setupAppRows(CLEAN, new Set(), new Set(), new Map([["discord", reason]]));
    const discord = rows.find((row) => row.id === "discord");
    expect(discord?.state).toBe("NOT DETECTED");
    expect(discord?.reason).toBe(reason);
    expect(discord?.installOffered).toBe(true);
    expect(discord?.openOffered).toBe(false);
    for (const claim of FORBIDDEN_AFTER_FAILURE) {
      expect(discord?.reason).not.toContain(claim);
    }
    const markup = setupAppsMarkup(rows, () => "");
    expect(markup).toContain(reason.replace(/'/gu, "&#39;"));
  });
});

describe("TASK 6810 — what the row is wired to", () => {
  it("presses Install into the pinned signed install and waits for it", () => {
    expect(services).toContain('invoke<unknown>("install_windows_app", { product })');
    expect(services).toContain('invoke<unknown>("open_windows_app", { product })');
    expect(services).toContain('invoke<unknown>("list_windows_app_rows")');
    expect(source).toContain("const row = await installWindowsApp(appId);");
    expect(source).toContain('if (row.state !== "detected") setupAppInstallFailures.set(appId, row.reason);');
  });

  it("binds Open to a press and never opens anything while installing", () => {
    expect(source).toContain("[data-setup-app-open]");
    expect(source).toContain("void openSetupApp(appId);");
    const installBody = source.slice(
      source.indexOf("async function installSetupApp("),
      source.indexOf("async function openSetupApp("),
    );
    expect(installBody).not.toContain("openWindowsApp");
  });

  it("no longer queues installs behind a package manager", () => {
    expect(source).not.toContain("enqueueBackgroundInstalls([appId])");
    expect(source).not.toContain("Windows is installing it. This row turns DETECTED");
  });

  it("waits on the installer rather than on a clock", () => {
    // The old Mullvad path polled `loadMullvadStatus` on a one-second timer for
    // three minutes and called whatever it saw at the end "installed".
    const installBranch = source.slice(
      source.indexOf("async function runMullvadSetupAction("),
      source.indexOf("const hosted = await hostMullvadUntilReady("),
    );
    expect(installBranch).toContain('installWindowsApp("mullvad")');
    expect(installBranch).not.toContain("setTimeout");
    expect(installBranch).not.toContain("Date.now()");
    expect(source).not.toContain("Mullvad installation did not finish within three minutes");
    expect(source).toContain("const rowState = found ? \"DETECTED\" : mullvadInstalling ? \"Installing\" : \"NOT DETECTED\";");
    expect(source).toContain('data-setup-app-state="${rowState}"');
  });
});
