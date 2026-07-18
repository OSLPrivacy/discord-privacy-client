const clamp = (value, min, max) => Math.min(max, Math.max(min, Number(value)));
const hexPattern = /^#[0-9a-f]{6}$/i;

export const BASE_STATE = Object.freeze({
  size: 380,
  padding: 36,
  stroke: 0,
  color: "#06b6d4",
  shield: 0,
  glow: 18,
  shadow: 12,
  wordmark: false,
  wordmarkSpacing: 28,
  context: "dark",
});

export const PRESETS = Object.freeze({
  homepage: { ...BASE_STATE, size: 420, padding: 40, glow: 20, shadow: 14, context: "dark" },
  desktop: { ...BASE_STATE, size: 320, padding: 30, stroke: 0.5, glow: 16, shadow: 10, context: "device" },
  website: { ...BASE_STATE, size: 160, padding: 20, glow: 12, shadow: 7, context: "light" },
  favicon: { ...BASE_STATE, size: 112, padding: 14, stroke: 1.5, glow: 7, shadow: 4, context: "transparent" },
});

export function sanitizeState(raw = {}) {
  return {
    size: Math.round(clamp(raw.size ?? BASE_STATE.size, 48, 560)),
    padding: Math.round(clamp(raw.padding ?? BASE_STATE.padding, 0, 100)),
    stroke: clamp(raw.stroke ?? BASE_STATE.stroke, 0, 10),
    color: hexPattern.test(String(raw.color ?? "")) ? String(raw.color).toLowerCase() : BASE_STATE.color,
    shield: clamp(raw.shield ?? BASE_STATE.shield, -100, 100),
    glow: clamp(raw.glow ?? BASE_STATE.glow, 0, 40),
    shadow: clamp(raw.shadow ?? BASE_STATE.shadow, 0, 40),
    wordmark: Boolean(raw.wordmark),
    wordmarkSpacing: Math.round(clamp(raw.wordmarkSpacing ?? BASE_STATE.wordmarkSpacing, 0, 100)),
    context: ["transparent", "dark", "light", "device"].includes(raw.context) ? raw.context : BASE_STATE.context,
  };
}

export function safeRandomState(random = Math.random) {
  const colors = ["#06b6d4", "#22d3ee", "#38bdf8", "#2dd4bf", "#60a5fa"];
  return sanitizeState({
    ...BASE_STATE,
    size: 250 + random() * 150,
    padding: 18 + random() * 30,
    stroke: random() * 2,
    color: colors[Math.min(colors.length - 1, Math.floor(random() * colors.length))],
    shield: -18 + random() * 36,
    glow: random() * 16,
    shadow: 4 + random() * 14,
    wordmark: random() > 0.35,
    wordmarkSpacing: 18 + random() * 28,
    context: ["transparent", "dark", "light", "device"][Math.min(3, Math.floor(random() * 4))],
  });
}

function escapeXml(value) {
  return String(value).replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&apos;" })[character]);
}

export function validatePaths(paths) {
  if (!Array.isArray(paths) || paths.length !== 2) throw new Error("The public OSL mark must contain exactly two paths.");
  return paths.map((path) => {
    if (!path || typeof path.d !== "string" || path.d.length < 20 || path.d.length > 30_000) throw new Error("Invalid OSL path data.");
    if (typeof path.transform !== "string" || path.transform.length > 160) throw new Error("Invalid OSL path transform.");
    return { d: path.d, transform: path.transform };
  });
}

export function logoMetrics(rawState) {
  const state = sanitizeState(rawState);
  const markWidth = state.size;
  const markHeight = state.size * 475 / 511;
  const wordmarkSize = state.size * 0.19;
  const wordmarkWidth = state.wordmark ? wordmarkSize * 3.6 : 0;
  const contentHeight = Math.max(markHeight, state.wordmark ? wordmarkSize * 1.3 : 0);
  return {
    width: Math.ceil(state.padding * 2 + markWidth + (state.wordmark ? state.wordmarkSpacing + wordmarkWidth : 0)),
    height: Math.ceil(state.padding * 2 + contentHeight),
    markWidth,
    markHeight,
    wordmarkSize,
    contentHeight,
  };
}

export function buildLogoSvg(rawState, sourcePaths) {
  const state = sanitizeState(rawState);
  const paths = validatePaths(sourcePaths);
  const metrics = logoMetrics(state);
  const scale = metrics.markWidth / 511;
  const markY = state.padding + (metrics.contentHeight - metrics.markHeight) / 2;
  const wordmarkX = state.padding + metrics.markWidth + state.wordmarkSpacing;
  const wordmarkY = state.padding + metrics.contentHeight / 2 + metrics.wordmarkSize * 0.34;
  const shieldScaleX = 1 + state.shield * 0.0012;
  const shieldScaleY = 1 - state.shield * 0.00055;
  const filter = state.glow || state.shadow ? ` filter="url(#osl-effects)"` : "";
  const defs = `<defs><filter id="osl-effects" x="-70%" y="-70%" width="240%" height="240%" color-interpolation-filters="sRGB"><feDropShadow dx="0" dy="${(state.shadow * 0.16).toFixed(2)}" stdDeviation="${(state.shadow * 0.22).toFixed(2)}" flood-color="#000000" flood-opacity="${Math.min(0.55, state.shadow / 70).toFixed(3)}"/><feDropShadow dx="0" dy="0" stdDeviation="${(state.glow * 0.24).toFixed(2)}" flood-color="${state.color}" flood-opacity="${Math.min(0.7, state.glow / 45).toFixed(3)}"/></filter></defs>`;
  const pathMarkup = paths.map((path, index) => {
    const geometry = index === 1 ? ` transform="translate(255.5 237.5) scale(${shieldScaleX.toFixed(4)} ${shieldScaleY.toFixed(4)}) translate(-255.5 -237.5)"` : "";
    return `<g${geometry}><path d="${escapeXml(path.d)}" transform="${escapeXml(path.transform)}"/></g>`;
  }).join("");
  const wordmark = state.wordmark ? `<text x="${wordmarkX}" y="${wordmarkY.toFixed(3)}" fill="${state.color}" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="${metrics.wordmarkSize.toFixed(2)}" font-weight="600" letter-spacing="-0.035em">OSL Privacy</text>` : "";
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${metrics.width}" height="${metrics.height}" viewBox="0 0 ${metrics.width} ${metrics.height}" role="img" aria-label="OSL Privacy logo">${defs}<g fill="${state.color}" stroke="${state.color}" stroke-width="${state.stroke}" stroke-linejoin="round" paint-order="stroke fill" vector-effect="non-scaling-stroke"${filter} transform="translate(${state.padding} ${markY.toFixed(3)}) scale(${scale.toFixed(6)})">${pathMarkup}</g>${wordmark}</svg>`;
}

export function cssVariables(rawState) {
  const state = sanitizeState(rawState);
  return `:root {\n  --osl-logo-color: ${state.color};\n  --osl-logo-size: ${state.size}px;\n  --osl-logo-padding: ${state.padding}px;\n  --osl-logo-stroke: ${state.stroke}px;\n  --osl-logo-shield-shape: ${state.shield};\n  --osl-logo-glow: ${state.glow}px;\n  --osl-logo-shadow: ${state.shadow}px;\n  --osl-logo-wordmark-gap: ${state.wordmarkSpacing}px;\n}`;
}
