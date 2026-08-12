import type { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { englishCatalogue } from "./catalogue/en";

export type AccountExportState =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "success"; archiveBytes: number; keyBytes: number; blockCount: number }
  | { kind: "failure"; message: string };

type Invoke = typeof tauriInvoke;

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character]!);
}

export function accountExportSettingsContent(state: AccountExportState): string {
  const status = state.kind === "saving"
    ? `<p class="machine-fact" role="status">Saving and verifying both files…</p>`
    : state.kind === "success"
      ? `<p class="machine-fact export-success" role="status">Export verified · ${state.archiveBytes} archive bytes · ${state.keyBytes} key bytes · ${state.blockCount} authenticated blocks</p>`
      : state.kind === "failure"
        ? `<p class="unlock-error" role="alert">${escapeHtml(state.message)}</p>`
        : "";
  return `<h2>Export my data</h2><p>Create one portable encrypted copy of your OSL account data and referenced attachments.</p><div class="warning account-export-warning"><strong>Keep the key separate</strong><p>${englishCatalogue.accountExportKeyWarning}</p><p>${englishCatalogue.accountExportKeyStorageWarning}</p><p>${englishCatalogue.accountExportIndependentCopyWarning}</p></div><form id="account-export-form" class="setup-surface" novalidate><label for="account-export-password">Sign in again with your current password</label><div class="password-input-row"><input id="account-export-password" name="password" type="password" minlength="6" maxlength="128" autocomplete="current-password" required/><button class="password-eye" type="button" data-password-toggle="account-export-password" aria-controls="account-export-password" aria-label="Show current password">◉</button></div><p>OSL will open two native save windows: first for the archive, then for its separate key.</p><button class="button primary" type="submit" ${state.kind === "saving" ? "disabled" : ""}>Choose archive and key locations</button>${status}</form>`;
}

function exactRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseAccountExportReceipt(raw: unknown): Extract<AccountExportState, { kind: "success" }> {
  if (!exactRecord(raw)) throw new Error("Export verification receipt was unavailable.");
  const keys = Object.keys(raw).sort();
  const expected = ["archiveBytes", "authenticatedBlocks", "classCounts", "keyBytes", "manifestBlocks"].sort();
  if (keys.length !== expected.length || keys.some((key, index) => key !== expected[index])) {
    throw new Error("Export verification receipt was incomplete.");
  }
  const archiveBytes = raw.archiveBytes;
  const keyBytes = raw.keyBytes;
  const authenticated = raw.authenticatedBlocks;
  const manifest = raw.manifestBlocks;
  const classes = raw.classCounts;
  if (!Number.isSafeInteger(archiveBytes) || Number(archiveBytes) <= 0
    || !Number.isSafeInteger(keyBytes) || Number(keyBytes) <= 0
    || !Array.isArray(authenticated) || authenticated.length === 0
    || !Array.isArray(manifest) || manifest.length !== authenticated.length
    || authenticated.some((value, index) => !Number.isSafeInteger(value) || value !== manifest[index] || value !== index)
    || !exactRecord(classes)
    || ["identity_profile", "settings", "friend_relationships", "messages", "attachments"]
      .some((name) => !Number.isSafeInteger(classes[name]) || Number(classes[name]) < 0)) {
    throw new Error("Export verification receipt was incomplete.");
  }
  return {
    kind: "success",
    archiveBytes: Number(archiveBytes),
    keyBytes: Number(keyBytes),
    blockCount: authenticated.length,
  };
}

export async function runAccountExport(password: string, invoke: Invoke): Promise<AccountExportState> {
  if (!/^[\x20-\x7e]{6,128}$/.test(password)) {
    return { kind: "failure", message: "Enter your current password." };
  }
  try {
    return parseAccountExportReceipt(await invoke("export_hub_account_data", { password }));
  } catch (error) {
    const message = error instanceof Error && error.message
      ? error.message
      : "The export was not saved and verified.";
    return { kind: "failure", message };
  }
}

export { englishCatalogue as accountExportEnglishCatalogue };
