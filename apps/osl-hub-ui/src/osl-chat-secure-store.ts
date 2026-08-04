//! Construct the OSL Chat `SecureLocalStore` and migrate off plaintext.
//
// D-108: `secure-local-store.ts` is implemented and unit-tested, and nothing in
// the shipping graph ever built one, because there was no key to build it with.
// This module is that missing line. It asks the native side for the HKDF subkey
// (`get_osl_chat_local_state_key`) and returns a store bound to it.
//
// Fail-closed on purpose: if the native side refuses — a locked main-password
// gate, an ACL rejection, a non-Tauri context — this returns `null` and the
// caller leaves the store unconfigured. The UI then persists nothing rather
// than writing correspondent identifiers to `localStorage` in the clear.

import { invoke } from "@tauri-apps/api/core";
import { SecureLocalStore } from "./secure-local-store";

/// The native command that derives the renderer's local-state key. Registered
/// in `hub_tauri_commands!`, declared in `permissions/hub.toml`, granted by
/// `capabilities/hub.json` — all three proven in
/// `hub_command_surface::tauri_registration_surface_tests`.
export const oslChatLocalStateKeyCommand = "get_osl_chat_local_state_key";

const RAW_KEY_BYTES = 32;

type SecureLocalStorageBackend = Pick<Storage, "getItem" | "setItem">;

function decodeBase64UrlKey(value: string): Uint8Array {
  if (!/^[A-Za-z0-9_-]+$/u.test(value)) throw new Error("osl-chat local-state key is not base64url");
  const padded = `${value.replace(/-/gu, "+").replace(/_/gu, "/")}${"=".repeat((4 - value.length % 4) % 4)}`;
  const binary = atob(padded);
  if (binary.length !== RAW_KEY_BYTES) throw new Error("osl-chat local-state key is not 256-bit");
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

/// Build the store for `storage`, or `null` when no key authority is available.
export async function createOslChatSecureLocalStore(
  storage: SecureLocalStorageBackend,
): Promise<SecureLocalStore | null> {
  let encodedKey: string;
  try {
    encodedKey = await invoke<string>(oslChatLocalStateKeyCommand);
  } catch (error) {
    console.info(`[OSL][chat] secure local store unavailable: ${String(error)}`);
    return null;
  }

  let rawKey: Uint8Array | null = null;
  try {
    rawKey = decodeBase64UrlKey(encodedKey);
    return new SecureLocalStore({ storage, key: await SecureLocalStore.importRawKey(rawKey) });
  } catch (error) {
    console.info(`[OSL][chat] secure local store refused its key: ${String(error)}`);
    return null;
  } finally {
    rawKey?.fill(0);
  }
}
