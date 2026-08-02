/**
 * Copy for destructive actions has two independent outcomes.  Local cleanup is
 * immediate, whereas the relay can only report a server outcome after it has
 * evidence.  Keep the two rows even when one is pending: a combined action
 * state would turn a completed local deletion into a misleading spinner.
 */
export type DestructAction = "burn" | "expiry";
export type LocalDestructStatus = "complete";
export type ServerDestructStatus = "confirmed" | "pending" | "not-confirmed";

export interface DestructStatusInput {
  action: DestructAction;
  local: LocalDestructStatus;
  server: ServerDestructStatus;
}

interface DestructCopy {
  local: string;
  server: Readonly<Record<ServerDestructStatus, string>>;
}

const DESTRUCT_COPY: Readonly<Record<DestructAction, DestructCopy>> = Object.freeze({
  burn: Object.freeze({
    local: "Deleted from this device.",
    server: Object.freeze({
      confirmed: "The server confirmed that it can no longer be downloaded.",
      pending: "Removing from the server when you're back online.",
      "not-confirmed": "Server deletion was not confirmed.",
    }),
  }),
  expiry: Object.freeze({
    local: "Expired on this device.",
    server: Object.freeze({
      confirmed: "The server confirmed that it stops being downloadable.",
      pending: "Server expiry is pending confirmation.",
      "not-confirmed": "Server expiry was not confirmed.",
    }),
  }),
});

/**
 * Renders the client and server facts as distinct rows.  There is deliberately
 * no aggregate action label or progress indicator: it would imply that the
 * already-complete local result depends on the server result.
 */
export function destructStatusMarkup(input: DestructStatusInput): string {
  const copy = DESTRUCT_COPY[input.action];
  return `<dl data-destruct-action="${input.action}"><div data-destruct-tier="local"><dt>On this device:</dt><dd>${copy.local}</dd></div><div data-destruct-tier="server" data-server-status="${input.server}"><dt>On the server:</dt><dd>${copy.server[input.server]}</dd></div></dl>`;
}
