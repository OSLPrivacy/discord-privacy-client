function escapeTooltipText(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

/**
 * Tooltip copy that remains inside the protected webview rather than asking
 * Chromium to create an uncapturable native `title` window.
 */
export function inDomTooltipMarkup(label: string): string {
  return `<span class="in-dom-tooltip" role="tooltip">${escapeTooltipText(label)}</span>`;
}
