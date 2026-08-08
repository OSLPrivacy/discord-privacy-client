export interface EnclaveSidebarChannel {
  readonly id: string;
  readonly name: string;
}

export interface EnclaveSidebarServer {
  readonly id: string;
  readonly name: string;
  readonly initials: string;
  readonly channels: readonly EnclaveSidebarChannel[];
}

export interface ServerChannelSidebarModel {
  readonly servers: readonly EnclaveSidebarServer[];
  readonly selectedServerId: string;
}

/** A stable, local fixture used until the Enclave roster is wired into this view. */
export const SERVER_CHANNEL_SIDEBAR_FIXTURE: ServerChannelSidebarModel = {
  selectedServerId: "osl-community",
  servers: [
    {
      id: "osl-community",
      name: "OSL Community",
      initials: "OC",
      channels: [
        { id: "welcome", name: "welcome" },
        { id: "general", name: "general" },
        { id: "release-notes", name: "release-notes" },
      ],
    },
    {
      id: "design-circle",
      name: "Design Circle",
      initials: "DC",
      channels: [],
    },
  ],
};

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] ?? character);
}

/** Renders the server rail and the selected server's channel list. */
export function serverChannelSidebarMarkup(model: ServerChannelSidebarModel = SERVER_CHANNEL_SIDEBAR_FIXTURE): string {
  const selected = model.servers.find((server) => server.id === model.selectedServerId) ?? model.servers[0];
  if (!selected) return "";

  const servers = model.servers.map((server) => {
    const isSelected = server.id === selected.id;
    return `<li><button class="enclave-server-button ${isSelected ? "is-selected" : ""}" data-enclave-server-id="${escapeHtml(server.id)}" type="button" aria-pressed="${isSelected}"><span aria-hidden="true">${escapeHtml(server.initials)}</span><span class="sr-only">${escapeHtml(server.name)}</span></button></li>`;
  }).join("");
  const channels = selected.channels.map((channel) => `<li><button class="enclave-channel-button" data-enclave-channel-id="${escapeHtml(channel.id)}" type="button"><span aria-hidden="true">#</span>${escapeHtml(channel.name)}</button></li>`).join("");

  return `<aside class="server-channel-sidebar" aria-label="Enclave navigation" data-fixture-screen="server-channel-sidebar"><nav class="server-list" aria-label="Servers"><ul>${servers}</ul></nav><section class="channel-sidebar" aria-labelledby="selected-server-name"><h2 id="selected-server-name">${escapeHtml(selected.name)}</h2><ul class="channel-list" aria-label="Channels in ${escapeHtml(selected.name)}">${channels}</ul></section></aside>`;
}
