/**
 * The destination for connected-platform server capabilities.
 *
 * OSL does not ship first-party servers or communities. Keeping this copy in a
 * dedicated view makes that boundary explicit instead of letting a bare
 * "Servers" heading imply a product that does not exist.
 */
export function oslServersViewMarkup(statusTag: (label: string) => string): string {
  const capabilities = [
    ["Discord servers", "Not available yet"],
    ["Telegram groups and channels", "Not available yet"],
    ["Signal groups", "Not available yet"],
    ["Snapchat groups", "Not available yet"],
  ];
  return `<main class="content-viewport osl-servers-page"><header class="osl-chat-page-header"><button class="text-button" data-route="home" type="button">Back</button><h1 id="route-heading" tabindex="-1">Third-party servers</h1></header><p>Shared encrypted spaces will appear here when their sender, membership, delivery, and history security are complete.</p><section class="settings-list" aria-label="Planned third-party server capabilities">${capabilities.map(([name, state]) => `<div class="setting-line"><span><strong>${name}</strong><small>${state}</small></span>${statusTag("Coming later")}</div>`).join("")}</section><p class="scope-approval-note">OSL does not claim provider-server access or read provider pages. Direct OSL Chats are available now.</p></main>`;
}
