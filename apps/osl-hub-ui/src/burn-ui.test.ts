import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { BurnGuaranteeCopy } from "./two-step-burn";
import { friendVerificationCopy } from "./ui-behavior";

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
  it("defines the five-part Burn guarantee copy contract without unsend claims", () => {
    expect(BurnGuaranteeCopy.summary).toBe("Burn cleans up. It does not un-send.");
    expect(BurnGuaranteeCopy.items.map((item) => item.id)).toEqual([
      "local_osl_copy",
      "osl_server_copy",
      "other_person_app",
      "connected_service_message",
      "already_opened_copies",
    ]);
    expect(new Set(BurnGuaranteeCopy.items.map((item) => item.title)).size).toBe(5);
    expect(BurnGuaranteeCopy.items.map((item) => item.state)).toEqual([
      "available",
      "request_only",
      "unavailable",
      "request_only",
      "not_possible",
    ]);

    const copy = [
      BurnGuaranteeCopy.summary,
      BurnGuaranteeCopy.intro,
      BurnGuaranteeCopy.limit,
      ...BurnGuaranteeCopy.items.flatMap((item) => [item.title, item.body]),
    ].join("\n");
    expect(copy).toContain("one success does not prove the others");
    expect(copy).toContain("The service decides.");
    expect(copy).toContain("That workflow is unavailable in this build.");
    expect(copy).toContain("cannot take back access someone already had");
    expect(copy).not.toMatch(/cryptographic burn|disappears forever|permanently undecryptable|gone for good/i);
    expect(copy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
    expect(copy).not.toMatch(/\b\d+%\b/);
  });

  it("offers exactly the requested scopes and gates app-wide burn on proven coverage", () => {
    const dialog = functionSource("burnDialogMarkup", "ownedConfirmationMarkup");
    expect(dialog).toContain('title: "This chat"');
    expect(dialog).toContain('title: "This app"');
    expect(dialog).toContain('title: "Entire OSL account"');
    expect(source).toContain("getHubServiceBurnReadiness");
    expect(source).toContain("OSL cannot prove complete coverage for this account yet.");
    expect(source).toContain("Open a supported chat first.");
  });

  it("renders the five Burn guarantee outcomes from the shared copy contract", () => {
    const helper = functionSource("burnGuaranteeMarkup", "burnDialogMarkup");
    const dialog = functionSource("burnDialogMarkup", "ownedConfirmationMarkup");
    expect(dialog).toContain("${burnGuaranteeMarkup(effects)}");
    expect(helper).toContain("BurnGuaranteeCopy.summary");
    expect(helper).toContain("BurnGuaranteeCopy.intro");
    expect(helper).toContain("BurnGuaranteeCopy.limit");
    expect(helper).toContain("BurnGuaranteeCopy.items.map");
    expect(helper).toContain('data-burn-guarantee="${escapeHtml(item.id)}"');
    expect(helper).toContain('case "available": return "Available"');
    expect(helper).toContain('case "request_only": return "Request only"');
    expect(helper).toContain('case "unavailable": return "Unavailable"');
    expect(helper).toContain('case "not_possible": return "Not possible"');
    expect(helper).not.toMatch(/cryptographic burn|disappears forever|permanently undecryptable|gone for good/i);
    expect(helper).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
    expect(helper).not.toMatch(/\b\d+%\b/);
  });

  it("states deletion limits before typed local confirmation", () => {
    const dialog = functionSource("burnDialogMarkup", "ownedConfirmationMarkup");
    expect(dialog).toContain("local decrypt material and caches");
    expect(dialog).toContain("revokes local approval, display, and expiry settings");
    expect(dialog).toContain("attempts to delete sent relay blobs");
    expect(dialog).toContain("local settings and caches");
    expect(dialog).toContain("indexed");
    expect(dialog).not.toContain("removes local decrypt keys for this app + friend");
    expect(dialog).not.toContain("Incoming OSL messages are already included");
    expect(BurnGuaranteeCopy.items.find((item) => item.id === "connected_service_message")?.body).toContain("The service decides");
    expect(BurnGuaranteeCopy.items.find((item) => item.id === "already_opened_copies")?.body).toContain("screenshots");
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
    expect(execute).toContain("Local approval, display, and expiry settings");
    expect(execute).toContain("Sent relay cleanup was acknowledged");
    expect(execute).toContain("Login profile, cookies, provider history, and other copies remain");
    expect(execute).not.toContain("Local decrypt keys for this app + friend");
    expect(execute).not.toContain("Local OSL decrypt material and caches for ${result.scopesBurned}");
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
    const copy = friendVerificationCopy("Rosalind", "12345 67890 12345 67890 12345 67890");
    const shown = [copy.heading, copy.instruction, copy.consequence, copy.invalidationNotice].join(" ");
    expect(shown).toContain("verification code");
    expect(shown).toContain("does not turn on decryption in any chat");
    // "safety number" is the protocol's name for it, not the operator's.
    expect(shown.toLowerCase()).not.toContain("safety number");
  });
});
