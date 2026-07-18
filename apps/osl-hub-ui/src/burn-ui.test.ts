import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("truthful Burn UI", () => {
  it("offers exactly the requested scopes and gates app-wide burn on proven coverage", () => {
    const dialog = functionSource("burnDialogMarkup", "ownedConfirmationMarkup");
    expect(dialog).toContain('title: "This chat"');
    expect(dialog).toContain('title: "This app"');
    expect(dialog).toContain('title: "Entire OSL account"');
    expect(source).toContain("getHubServiceBurnReadiness");
    expect(source).toContain("OSL cannot prove complete coverage for this account yet.");
    expect(source).toContain("Open a supported chat first.");
  });

  it("states deletion limits before typed local confirmation", () => {
    const dialog = functionSource("burnDialogMarkup", "ownedConfirmationMarkup");
    expect(dialog).toContain("local decrypt material and caches");
    expect(dialog).toContain("Messages and history in the service remain");
    expect(dialog).toContain("Screenshots, exports, backups, and copies");
    expect(source).toContain("BURN CHAT");
    expect(source).toContain("BURN APP");
    expect(source).toContain("BURN ACCOUNT");
    expect(dialog).toContain('id="burn-confirm-submit" type="submit" disabled');
  });

  it("does not fake remote friend burn or recipient acknowledgments", () => {
    const dialog = functionSource("burnDialogMarkup", "ownedConfirmationMarkup");
    expect(dialog).toContain("Burn for friends · Pro");
    expect(dialog).toContain("prior signed consent");
    expect(dialog).toContain("acknowledgment from each device");
    expect(dialog).toContain("workflow is unavailable in this build");
    expect(dialog).toContain('<input type="checkbox" disabled/>');
  });

  it("uses real local commands, guards repeats, and reports partial results inline", () => {
    const dialog = functionSource("burnDialogMarkup", "ownedConfirmationMarkup");
    const execute = functionSource("executeBurn", "ttlSeconds");
    const burnUi = `${dialog}\n${execute}`;
    expect(execute).toContain("burnBusy");
    expect(execute).toContain("burnActiveHubContext(contextToken)");
    expect(execute).toContain("burnHubServiceAccount");
    expect(execute).toContain("readiness?.coverageComplete");
    expect(execute).toContain("login profile, cookies, and service history remain");
    expect(execute).toContain("executeHubFullCleanup()");
    expect(execute).toContain("localCleanupComplete");
    expect(execute).toContain("no remote deletion success is being claimed");
    expect(burnUi).not.toContain("window.confirm");
    expect(burnUi).not.toContain("window.alert");
  });

  it("keeps uninstall separate and uses square scope cards", () => {
    const dialog = functionSource("burnDialogMarkup", "ownedConfirmationMarkup");
    expect(dialog).toContain("Uninstall after burn");
    expect(dialog).toContain("ms-settings:appsfeatures");
    expect(styles).toContain(".burn-scope-grid");
    expect(styles).toContain(".burn-scope-card");
  });
});

describe("OSL-owned confirmations", () => {
  it("uses plain verification-code language for local friend approval", () => {
    const dialog = functionSource("ownedConfirmationMarkup", "serviceContent");
    expect(dialog).toContain("verification code");
    expect(dialog).toContain("does not turn on decryption in any chat");
    expect(dialog).not.toContain("safety number");
  });
});
