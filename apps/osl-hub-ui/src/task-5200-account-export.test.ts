import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import {
  accountExportEnglishCatalogue,
  accountExportSettingsContent,
  parseAccountExportReceipt,
  runAccountExport,
} from "./account-export";

const receipt = {
  archiveBytes: 98765,
  keyBytes: 312,
  authenticatedBlocks: [0, 1, 2, 3],
  manifestBlocks: [0, 1, 2, 3],
  classCounts: {
    identity_profile: 1,
    settings: 1,
    friend_relationships: 4,
    messages: 41,
    attachments: 7,
  },
};

describe("TASK 5200 packaged account export", () => {
  it("is reachable through the actual packaged Settings sidebar and command binding", () => {
    const shippingMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    expect(shippingMain).toContain('["export", "Export my data"]');
    expect(shippingMain).toContain('settingsSection === "export"');
    expect(shippingMain).toContain('querySelector<HTMLFormElement>("#account-export-form")');
    expect(shippingMain).toContain("runAccountExport(password, invoke)");
  });
  it("renders the exact shipping catalogue warnings before its reauthorization form", () => {
    const markup = accountExportSettingsContent({ kind: "idle" });
    expect(markup).toContain("Export my data");
    expect(markup).toContain(accountExportEnglishCatalogue.accountExportKeyWarning);
    expect(markup).toContain(accountExportEnglishCatalogue.accountExportKeyStorageWarning);
    expect(markup).toContain(accountExportEnglishCatalogue.accountExportIndependentCopyWarning);
    expect(markup.indexOf(accountExportEnglishCatalogue.accountExportKeyWarning)).toBeLessThan(markup.indexOf("account-export-form"));
    expect(markup.indexOf(accountExportEnglishCatalogue.accountExportKeyStorageWarning)).toBeLessThan(markup.indexOf("account-export-form"));
    expect(markup.indexOf(accountExportEnglishCatalogue.accountExportIndependentCopyWarning)).toBeLessThan(markup.indexOf("account-export-form"));
    expect(markup).toContain('autocomplete="current-password"');
    expect(markup).toContain("two native save windows");
  });

  it("reauthorizes through the one packaged native command and accepts only a full receipt", async () => {
    const invoke = vi.fn().mockResolvedValue(receipt);
    await expect(runAccountExport("correct horse", invoke)).resolves.toEqual({
      kind: "success", archiveBytes: 98765, keyBytes: 312, blockCount: 4,
    });
    expect(invoke).toHaveBeenCalledWith("export_hub_account_data", { password: "correct horse" });
    expect(parseAccountExportReceipt(receipt).kind).toBe("success");
  });

  it.each([
    ["missing key", { ...receipt, keyBytes: undefined }],
    ["sample only", { ...receipt, authenticatedBlocks: [0], manifestBlocks: [0, 1, 2, 3] }],
    ["missing final block", { ...receipt, authenticatedBlocks: [0, 1, 2], manifestBlocks: [0, 1, 2, 3] }],
    ["reordered block", { ...receipt, authenticatedBlocks: [0, 2, 1, 3] }],
    ["missing class", { ...receipt, classCounts: { messages: 41 } }],
  ])("refuses a successful-looking %s receipt", async (_name, mutant) => {
    const invoke = vi.fn().mockResolvedValue(mutant);
    await expect(runAccountExport("correct horse", invoke)).resolves.toMatchObject({ kind: "failure" });
  });

  it("does not invoke native for invalid reauthorization and reports cancel/failure", async () => {
    const invoke = vi.fn();
    await expect(runAccountExport("", invoke)).resolves.toMatchObject({ kind: "failure" });
    expect(invoke).not.toHaveBeenCalled();
    invoke.mockRejectedValueOnce(new Error("Key save was cancelled"));
    await expect(runAccountExport("correct horse", invoke)).resolves.toEqual({ kind: "failure", message: "Key save was cancelled" });
  });
});
