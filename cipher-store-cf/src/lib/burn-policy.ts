/// Offline-safe server-side burn policy.
///
/// The caller supplies a store adapter so this policy stays independent of
/// D1/R2 details. A missing, malformed, expired, ACKed, or already-burned
/// blob is deliberately indistinguishable from a successful burn: a queued
/// offline delete must always be terminal for the client.

import { constantTimeEqualHex, sha256Hex } from "./digest.js";

const MANAGE_CAP_RE = /^[0-9a-f]{32}$/;

export interface BurnStore {
  manageCapabilityDigestFor(blobId: string): Promise<string | null>;
  destroy(blobId: string): Promise<void>;
}

function normaliseManageCapability(value: string): string | null {
  const capability = value.trim().toLowerCase();
  return MANAGE_CAP_RE.test(capability) ? capability : null;
}

/**
 * Applies one queued burn and always supplies the terminal response shape.
 *
 * There is intentionally no timestamp or freshness argument: possession of
 * the non-expiring manage capability is the authority, so a burn queued
 * while offline can be sent unchanged on reconnect days later.
 */
export async function applyBurn(
  store: BurnStore,
  blobId: string,
  presentedManageCapability: string | null,
): Promise<Response> {
  const capability = presentedManageCapability === null
    ? null
    : normaliseManageCapability(presentedManageCapability);
  const storedDigest = await store.manageCapabilityDigestFor(blobId);

  if (capability !== null && storedDigest !== null) {
    const presentedDigest = await sha256Hex(capability);
    if (constantTimeEqualHex(storedDigest, presentedDigest)) {
      await store.destroy(blobId);
    }
  }

  return new Response(null, { status: 204 });
}
