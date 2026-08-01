/**
 * Honest status for actions that require the OSL relay.
 *
 * Offline is intentionally not inferred from a browser event alone. A caller
 * must positively report an active connection before this projection makes a
 * network-backed action available; an unknown connection is handled as
 * unavailable so the UI never promises an operation that cannot happen.
 */
export type OslConnectionState = "online" | "offline" | "unknown";

export type OfflineUnavailableCapability =
  | "receiveNewMessages"
  | "sendMessage"
  | "lookUpNewContactKey"
  | "confirmBurnOnServer"
  | "enforceExpiryOnServer"
  | "enforceViewOnceOnServer";

export interface OfflineCapabilityStatus {
  available: boolean;
  title: string;
  detail: string;
}

const AVAILABLE: OfflineCapabilityStatus = {
  available: true,
  title: "Available",
  detail: "OSL is connected.",
};

const OFFLINE_STATUSES: Readonly<Record<OfflineUnavailableCapability, OfflineCapabilityStatus>> = {
  receiveNewMessages: {
    available: false,
    title: "New messages need a connection",
    detail: "OSL cannot receive new messages until it reconnects. Messages already on this device remain readable.",
  },
  sendMessage: {
    available: false,
    title: "Sending needs a connection",
    detail: "You can compose and encrypt a message now, but OSL cannot send it until it reconnects.",
  },
  lookUpNewContactKey: {
    available: false,
    title: "Adding a new contact needs a connection",
    detail: "OSL cannot look up a new contact's encryption key until it reconnects.",
  },
  confirmBurnOnServer: {
    available: false,
    title: "Server deletion is pending",
    detail: "Your local copy can be removed now, but OSL cannot confirm deletion from the server until it reconnects.",
  },
  enforceExpiryOnServer: {
    available: false,
    title: "Server expiry is pending",
    detail: "Local expiry can still run, but OSL cannot enforce expiry on the server until it reconnects.",
  },
  enforceViewOnceOnServer: {
    available: false,
    title: "Server view-once protection is pending",
    detail: "View-once is enforced for a message already on this device, but OSL cannot enforce it on the server until it reconnects.",
  },
};

/**
 * Describes whether an action can happen now without erasing the distinction
 * between local enforcement and work that still needs the relay.
 */
export function offlineCapabilityStatus(
  capability: OfflineUnavailableCapability,
  connection: OslConnectionState,
): OfflineCapabilityStatus {
  return connection === "online" ? AVAILABLE : OFFLINE_STATUSES[capability];
}
