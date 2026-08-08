export interface EnclaveSidebarChannel {
  readonly id: string;
  readonly name: string;
  /** Member ids with access to this channel. */
  readonly memberIds: readonly string[];
}

export const ENCLAVE_SERVER_ACTIONS = ["read", "send", "invite", "make channels", "remove messages", "remove people", "change server"] as const;

export type EnclaveServerAction = (typeof ENCLAVE_SERVER_ACTIONS)[number];

export interface EnclaveSidebarMember {
  readonly id: string;
  readonly name: string;
  /** Explicit server actions available to this member. */
  readonly serverActions: readonly EnclaveServerAction[];
}

export interface EnclaveSidebarServer {
  readonly id: string;
  readonly name: string;
  readonly initials: string;
  readonly channels: readonly EnclaveSidebarChannel[];
  readonly members: readonly EnclaveSidebarMember[];
}

export interface ServerChannelSidebarModel {
  readonly servers: readonly EnclaveSidebarServer[];
  readonly selectedServerId: string;
  readonly selectedChannelId: string;
  readonly viewerMemberId: string;
}

/** A stable, local fixture used until the Enclave roster is wired into this view. */
export const SERVER_CHANNEL_SIDEBAR_FIXTURE: ServerChannelSidebarModel = {
  selectedServerId: "osl-community",
  selectedChannelId: "general",
  viewerMemberId: "liam",
  servers: [
    {
      id: "osl-community",
      name: "OSL Community",
      initials: "OC",
      members: [
        { id: "avery", name: "Avery Chen", serverActions: ["read", "send", "invite", "make channels", "remove messages", "remove people", "change server"] },
        { id: "morgan", name: "Morgan Reyes", serverActions: ["read", "send", "invite", "remove messages"] },
        { id: "liam", name: "Liam", serverActions: ["read", "send"] },
      ],
      channels: [
        { id: "welcome", name: "welcome", memberIds: ["avery", "morgan"] },
        { id: "general", name: "general", memberIds: ["avery", "morgan", "liam"] },
        { id: "release-notes", name: "release-notes", memberIds: ["avery"] },
      ],
    },
    {
      id: "design-circle",
      name: "Design Circle",
      initials: "DC",
      channels: [],
      members: [],
    },
  ],
};

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] ?? character);
}

/** Renders the server rail and the selected server's channel list. */
export function serverChannelSidebarMarkup(
  model: ServerChannelSidebarModel = SERVER_CHANNEL_SIDEBAR_FIXTURE,
  memberPanelOnly = false,
): string {
  const selected = model.servers.find((server) => server.id === model.selectedServerId) ?? model.servers[0];
  if (!selected) return "";

  const selectedChannel = selected.channels.find((channel) => channel.id === model.selectedChannelId) ?? selected.channels[0];
  const viewer = selected.members.find((member) => member.id === model.viewerMemberId);
  const viewerActions = new Set(viewer?.serverActions ?? []);
  const canInvite = viewerActions.has("invite");
  const canRemovePeople = viewerActions.has("remove people");

  const servers = model.servers.map((server) => {
    const isSelected = server.id === selected.id;
    return `<li><button class="enclave-server-button ${isSelected ? "is-selected" : ""}" data-enclave-server-id="${escapeHtml(server.id)}" type="button" aria-pressed="${isSelected}"><span aria-hidden="true">${escapeHtml(server.initials)}</span><span class="sr-only">${escapeHtml(server.name)}</span></button></li>`;
  }).join("");
  const channels = selected.channels.map((channel) => {
    const isSelected = channel.id === selectedChannel?.id;
    return `<li><button class="enclave-channel-button ${isSelected ? "is-selected" : ""}" data-enclave-channel-id="${escapeHtml(channel.id)}" type="button" aria-pressed="${isSelected}"><span aria-hidden="true">#</span>${escapeHtml(channel.name)}</button></li>`;
  }).join("");
  const selectedChannelName = selectedChannel ? `#${selectedChannel.name}` : "no channel";
  const members = selected.members.map((member) => {
    const isInSelectedChannel = selectedChannel?.memberIds.includes(member.id) ?? false;
    const removeControl = canRemovePeople && member.id !== viewer?.id
      ? `<button class="enclave-member-control" data-enclave-remove-member="${escapeHtml(member.id)}" type="button">Remove person</button>`
      : "";
    return `<li class="enclave-member-row" data-enclave-member-id="${escapeHtml(member.id)}"><div><strong>${escapeHtml(member.name)}</strong><small>${isInSelectedChannel ? `Member of ${escapeHtml(selectedChannelName)}` : `Not in ${escapeHtml(selectedChannelName)}`}</small><p class="enclave-member-actions" aria-label="Allowed server actions for ${escapeHtml(member.name)}"><span>Allowed:</span> ${member.serverActions.map(escapeHtml).join(" · ")}</p></div>${removeControl}</li>`;
  }).join("");
  const inviteControl = canInvite ? `<button class="enclave-invite-button" data-enclave-invite type="button">Invite</button>` : "";

  const memberPanel = `<section class="enclave-member-panel" aria-labelledby="enclave-members-heading" data-selected-server-id="${escapeHtml(selected.id)}" data-selected-channel-id="${escapeHtml(selectedChannel?.id ?? "")}"><header><div><h2 id="enclave-members-heading">Members</h2><p>${escapeHtml(selectedChannelName)}</p></div>${inviteControl}</header><ul class="enclave-member-list" aria-label="Members of ${escapeHtml(selected.name)} in ${escapeHtml(selectedChannelName)}">${members}</ul></section>`;
  if (memberPanelOnly) return memberPanel;
  return `<aside class="server-channel-sidebar" aria-label="Enclave navigation" data-fixture-screen="server-channel-sidebar"><nav class="server-list" aria-label="Servers"><ul>${servers}</ul></nav><section class="channel-sidebar" aria-labelledby="selected-server-name"><h2 id="selected-server-name">${escapeHtml(selected.name)}</h2><ul class="channel-list" aria-label="Channels in ${escapeHtml(selected.name)}">${channels}</ul></section>${memberPanel}</aside>`;
}

/** Reuse the same permission rendering beneath the richer five-region surface. */
export function serverMemberPermissionsMarkup(model: ServerChannelSidebarModel = SERVER_CHANNEL_SIDEBAR_FIXTURE): string {
  return serverChannelSidebarMarkup(model, true);
}
