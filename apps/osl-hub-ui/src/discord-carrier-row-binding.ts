export interface NativeDiscordCarrierRowBinding {
  messageId: string;
  nativeLocatorSha256: string;
  carrierSha256: string;
  leftPx: number;
  topPx: number;
  widthPx: number;
  heightPx: number;
  backgroundColor: string;
  foregroundColor: string;
  fontFamily: string;
  fontSizePx: number;
  fontWeight: number;
  lineHeightPx: number;
  letterSpacingPx: number;
  zoom: number;
  density: number;
}

const MAX_VISIBLE_CARRIER_ROWS = 32;
const MAX_OVERLAY_EDGE_PX = 16_384;
const SHA256 = /^[0-9a-f]{64}$/u;

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length
    && expected.every((key, index) => key === actual[index]);
}

function boundedMessageId(value: unknown): value is string {
  return typeof value === "string"
    && value.length > 0
    && new TextEncoder().encode(value).length <= 96
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

export function parseNativeDiscordCarrierRowBinding(
  value: unknown,
): NativeDiscordCarrierRowBinding | null {
  if (!exactRecord(value, [
    "messageId",
    "nativeLocatorSha256",
    "carrierSha256",
    "leftPx",
    "topPx",
    "widthPx",
    "heightPx",
    "backgroundColor",
    "foregroundColor",
    "fontFamily",
    "fontSizePx",
    "fontWeight",
    "lineHeightPx",
    "letterSpacingPx",
    "zoom",
    "density",
  ])) return null;
  const coordinates = [value.leftPx, value.topPx, value.widthPx, value.heightPx];
  const color = /^rgb\((?:[0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5]) (?:[0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5]) (?:[0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])\)$/u;
  const fontFamily = /^[\p{L}\p{N} ._-]{1,64}$/u;
  if (!boundedMessageId(value.messageId)
    || typeof value.nativeLocatorSha256 !== "string" || !SHA256.test(value.nativeLocatorSha256)
    || typeof value.carrierSha256 !== "string" || !SHA256.test(value.carrierSha256)
    || !coordinates.every((coordinate) => typeof coordinate === "number" && Number.isFinite(coordinate))
    || Number(value.leftPx) < 0 || Number(value.topPx) < 0
    || Number(value.widthPx) < 1 || Number(value.heightPx) < 12
    || typeof value.backgroundColor !== "string" || !color.test(value.backgroundColor)
    || typeof value.foregroundColor !== "string" || !color.test(value.foregroundColor)
    || typeof value.fontFamily !== "string" || !fontFamily.test(value.fontFamily)
    || typeof value.fontSizePx !== "number" || !Number.isFinite(value.fontSizePx)
    || value.fontSizePx < 8 || value.fontSizePx > 128
    || typeof value.fontWeight !== "number" || !Number.isInteger(value.fontWeight)
    || value.fontWeight < 100 || value.fontWeight > 1_000
    || typeof value.lineHeightPx !== "number" || !Number.isFinite(value.lineHeightPx)
    || value.lineHeightPx < 10 || value.lineHeightPx > 128
    || typeof value.letterSpacingPx !== "number" || !Number.isFinite(value.letterSpacingPx)
    || value.letterSpacingPx < -4 || value.letterSpacingPx > 16
    || typeof value.zoom !== "number" || !Number.isFinite(value.zoom)
    || value.zoom < 0.5 || value.zoom > 4
    || typeof value.density !== "number" || !Number.isFinite(value.density)
    || value.density < 0.7 || value.density > 3
    || Number(value.leftPx) + Number(value.widthPx) > MAX_OVERLAY_EDGE_PX
    || Number(value.topPx) + Number(value.heightPx) > MAX_OVERLAY_EDGE_PX) return null;
  return value as unknown as NativeDiscordCarrierRowBinding;
}

export function parseNativeDiscordCarrierRowBindings(
  value: unknown,
): NativeDiscordCarrierRowBinding[] | null {
  if (!Array.isArray(value) || value.length > MAX_VISIBLE_CARRIER_ROWS) return null;
  const parsed: NativeDiscordCarrierRowBinding[] = [];
  const messageIds = new Set<string>();
  const nativeLocators = new Set<string>();
  for (const candidate of value) {
    const binding = parseNativeDiscordCarrierRowBinding(candidate);
    if (!binding || messageIds.has(binding.messageId)
      || nativeLocators.has(binding.nativeLocatorSha256)) return null;
    messageIds.add(binding.messageId);
    nativeLocators.add(binding.nativeLocatorSha256);
    parsed.push(binding);
  }
  return parsed;
}

export function clearCarrierRowGeometry(row: HTMLElement): void {
  row.classList.remove("osl-discord-transcript__row--carrier-bound");
  delete row.dataset.nativeLocatorSha256;
  delete row.dataset.carrierSha256;
  for (const property of [
    "--osl-carrier-left",
    "--osl-carrier-top",
    "--osl-carrier-width",
    "--osl-carrier-height",
    "--osl-carrier-background",
    "--osl-carrier-foreground",
    "--osl-carrier-font-family",
    "--osl-carrier-font-size",
    "--osl-carrier-font-weight",
    "--osl-carrier-line-height",
    "--osl-carrier-letter-spacing",
    "--osl-carrier-zoom",
    "--osl-carrier-density",
  ]) row.style.removeProperty(property);
  row.hidden = true;
}

export function applyCarrierRowGeometry(
  row: HTMLElement,
  binding: NativeDiscordCarrierRowBinding,
): void {
  clearCarrierRowGeometry(row);
  row.dataset.nativeLocatorSha256 = binding.nativeLocatorSha256;
  row.dataset.carrierSha256 = binding.carrierSha256;
  row.style.setProperty("--osl-carrier-left", `${binding.leftPx}px`);
  row.style.setProperty("--osl-carrier-top", `${binding.topPx}px`);
  row.style.setProperty("--osl-carrier-width", `${binding.widthPx}px`);
  row.style.setProperty("--osl-carrier-height", `${binding.heightPx}px`);
  row.style.setProperty("--osl-carrier-background", binding.backgroundColor);
  row.style.setProperty("--osl-carrier-foreground", binding.foregroundColor);
  row.style.setProperty("--osl-carrier-font-family", JSON.stringify(binding.fontFamily));
  row.style.setProperty("--osl-carrier-font-size", `${binding.fontSizePx}px`);
  row.style.setProperty("--osl-carrier-font-weight", String(binding.fontWeight));
  row.style.setProperty("--osl-carrier-line-height", `${binding.lineHeightPx}px`);
  row.style.setProperty("--osl-carrier-letter-spacing", `${binding.letterSpacingPx}px`);
  row.style.setProperty("--osl-carrier-zoom", String(binding.zoom));
  row.style.setProperty("--osl-carrier-density", String(binding.density));
  row.classList.add("osl-discord-transcript__row--carrier-bound");
  row.hidden = false;
}
