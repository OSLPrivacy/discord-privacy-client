/**
 * Channel visibility is deliberately separate from channel access.  A member
 * always receives every local channel row; a per-channel rule only changes
 * whether opening that row is allowed.
 */
export type EnclaveChannelPermissionOverride = {
  readonly memberId: string;
  readonly decision: "allow" | "deny";
  /** The human-readable rule which made this decision. */
  readonly reason: string;
};

export type EnclaveChannel = {
  readonly channelId: string;
  readonly name: string;
  readonly permissionOverrides?: readonly EnclaveChannelPermissionOverride[];
};

export type EnclaveChannelListRow = {
  readonly channelId: string;
  readonly name: string;
  readonly denied: boolean;
  /** Present for denied rows and used unchanged by hover and open refusal. */
  readonly reason: string | null;
};

export type EnclaveChannelList = {
  readonly memberId: string;
  readonly rows: readonly EnclaveChannelListRow[];
};

export type EnclaveChannelOpenResult =
  | { readonly opened: true; readonly channelId: string; readonly refusal: null }
  | { readonly opened: false; readonly channelId: string; readonly refusal: string };

function requireText(value: string, label: string): string {
  if (!value.trim()) throw new Error(`Enclave ${label} is required`);
  return value;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function permissionOverrideFor(
  memberId: string,
  channel: EnclaveChannel,
): EnclaveChannelPermissionOverride | undefined {
  const matches = (channel.permissionOverrides ?? []).filter((override) => override.memberId === memberId);
  if (matches.length > 1) throw new Error(`Duplicate Enclave channel override for ${memberId}`);
  return matches[0];
}

/** Projects every channel into a visible row, including rows which cannot open. */
export function createEnclaveChannelList(
  memberId: string,
  channels: readonly EnclaveChannel[],
): EnclaveChannelList {
  requireText(memberId, "member ID");
  const seenChannelIds = new Set<string>();
  const rows = channels.map((channel) => {
    requireText(channel.channelId, "channel ID");
    requireText(channel.name, "channel name");
    if (seenChannelIds.has(channel.channelId)) throw new Error(`Duplicate Enclave channel ${channel.channelId}`);
    seenChannelIds.add(channel.channelId);

    const override = permissionOverrideFor(memberId, channel);
    if (override?.decision === "deny") {
      return { channelId: channel.channelId, name: channel.name, denied: true, reason: requireText(override.reason, "denial reason") };
    }
    return { channelId: channel.channelId, name: channel.name, denied: false, reason: null };
  });
  return { memberId, rows };
}

/** Attempts to open a rendered row without recomputing or rewriting its rule. */
export function openEnclaveChannel(list: EnclaveChannelList, channelId: string): EnclaveChannelOpenResult {
  const row = list.rows.find((candidate) => candidate.channelId === channelId);
  if (!row) throw new Error(`Unknown Enclave channel ${channelId}`);
  if (row.denied) return { opened: false, channelId, refusal: row.reason as string };
  return { opened: true, channelId, refusal: null };
}

/**
 * Renders a channel row as a button so the keyboard path has the same denied
 * state as the pointer path.  The title is intentionally the exact rule used
 * for the refusal, rather than a generic access-denied message.
 */
export function oslEnclaveChannelListMarkup(list: EnclaveChannelList): string {
  const rows = list.rows.map((row) => {
    const reason = row.reason === null ? "" : ` title="${escapeHtml(row.reason)}" data-refusal-reason="${escapeHtml(row.reason)}"`;
    // Keep denied rows activatable: callers use openEnclaveChannel to return
    // the rule, and native disabled controls do not reliably expose title on
    // hover across browsers.
    const disabled = row.denied ? " aria-disabled=\"true\"" : " aria-disabled=\"false\"";
    const classes = row.denied ? "osl-enclave-channel-row is-denied" : "osl-enclave-channel-row";
    return `<li><button class="${classes}" type="button" data-enclave-channel-id="${escapeHtml(row.channelId)}"${disabled}${reason}># ${escapeHtml(row.name)}</button></li>`;
  }).join("");
  return `<section class="osl-enclave-channel-list" aria-label="Enclave channels"><h2>Channels</h2><ul>${rows}</ul></section>`;
}
