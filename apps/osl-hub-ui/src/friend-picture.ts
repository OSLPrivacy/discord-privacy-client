export interface FriendPictureData {
  picture: string | null;
  fallbackLetter: string;
  fallbackColour: string;
}

export function friendPictureMarkup(data: FriendPictureData): string {
  if (data.picture) {
    return `<img class="friend-picture" src="${escapeHtml(data.picture)}" alt="Friend picture"/>`;
  }
  return `<span class="friend-picture-fallback" style="background-color: ${escapeHtml(data.fallbackColour)}">${escapeHtml(data.fallbackLetter)}</span>`;
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
