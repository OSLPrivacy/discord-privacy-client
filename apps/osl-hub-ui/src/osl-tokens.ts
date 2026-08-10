// OSL design tokens — the single source of colour, type, shape and motion.
//
// Transcribed from the design handoff at
// OSL-AUDITS/reference/design-export-2026-08-06/README.md ("Design Tokens"),
// which states: "High-fidelity. Colors, spacing, typography, copy, and
// interaction details are final and should be recreated pixel-perfectly."
//
// WHY THIS FILE EXISTS. On 2026-08-08 an audit of 140 UI captures found 104
// distinct hard-coded colours across 21 files and **not one** of them was a
// design colour. The most common colour in the app was `#06b6d4` — Tailwind
// cyan-600 — standing in for the OSL accent `#2ac0f0`, close enough to pass a
// glance and wrong everywhere. Discord's own palette (`#5865f2`, `#313338`)
// and Tailwind violet had also leaked in. Four separate visual systems were
// shipping side by side.
//
// RULE: no hex literal belongs anywhere else in the UI. Import from here.
// A colour that is not in this file is not an OSL colour.

export const colour = {
  // Grounds. The window is full-bleed; there is no panel-on-page look.
  background: '#080c0d',
  surface: '#0a0e10',
  modalSurface: '#0b1013',
  inputFill: '#0f1416',
  composerFill: '#14191c',

  // Dividers. No shadows anywhere, except a modal drop shadow.
  hairline: '#161b1e',
  hairlineAlt: '#14191c',
  hairlineQuiet: '#131719',

  // Control outlines. The default control is a 1.5px border, not a fill.
  border: '#2a343a',
  borderQuiet: '#1e2326',

  // Text, then the muted ladder in descending importance.
  text: '#f4f7f8',
  muted1: '#97a3a9',
  muted2: '#8d989e',
  muted3: '#66727a',
  muted4: '#5b666c',
  muted5: '#4a555b',
  muted6: '#3d474c',
  muted7: '#333d42',

  // Accent. Cyan means "hover, active, selection, OSL is acting".
  accent: '#2ac0f0',
  accentBorderDim: '#17303a',
  accentBorderDim2: '#1c3a45',

  // Safe. Verified, reachable, success.
  safe: '#3dd68c',
  safeBorderDim: '#1e3a2a',
  safeBorderDim2: '#26543c',

  // Timers and view-once. Distinct from the amber key-changed warning.
  timer: '#f0b429',
  timerBorderDim: '#3d2f10',
  warning: '#f0a93a',

  // Pro-locked features.
  pro: '#a79bff',
  proBorderDim: '#2f2a3d',

  // Danger. Burn, revoke, irreversible.
  danger: '#e05656',
  dangerAlt: '#e5383b',
  dangerBorderDim: '#3a1618',
  dangerText: '#f0a9ab',
} as const

// Avatar tiles are keyed deterministically by the first letter of the name.
export const avatarPalette = {
  M: '#2ac0f0', R: '#7ee0b8', K: '#f0b429', T: '#a79bff',
  J: '#f08a6c', D: '#6cd0f0', S: '#9fe07e',
} as const
export const avatarFallback = '#5b8f9e'
export const avatarGlyph = '#08161a'

export function avatarColour(name: string): string {
  const initial = (name.trim()[0] ?? '').toUpperCase()
  return (avatarPalette as Record<string, string>)[initial] ?? avatarFallback
}

// Typography. Monospace is not decoration: it *means* "this is a fact the
// system asserts", so status labels, counts and timestamps must use it and
// prose must not.
export const type = {
  interface: "'Segoe UI', system-ui, sans-serif",
  interfaceWeight: 600,
  prose: "'Source Sans 3', system-ui, sans-serif",
  mono: 'Consolas, ui-monospace, monospace',
  emoji: "'Segoe UI Emoji', 'Apple Color Emoji', 'Noto Color Emoji'",
  chatDefault: "system-ui, -apple-system, 'Segoe UI', sans-serif",
  chatSizeDefault: 15.5,
  chatSizeMin: 13,
  chatSizeMax: 20,
} as const

// The style for anything machine-truthy: 10px, 700, uppercase, letter-spaced.
export const statusStyle = {
  fontFamily: type.mono,
  fontSize: '10px',
  fontWeight: 700,
  letterSpacing: '0.14em',
  textTransform: 'uppercase',
} as const

// Shape. Squares, not pills.
export const shape = {
  control: '2px',        // 0-3px on controls
  composer: '8px',       // the Chats composer is the one exception
  reactionChip: '4px',
  avatar: '50%',
  controlBorderWidth: '1.5px',
} as const

export const motion = {
  colour: '0.2s',            // 0.15-0.2s colour transitions
  arrowNudge: '0.35s',
  arrowEasing: 'cubic-bezier(0.34,1.56,0.64,1)',
  arrowTravel: '3px',
} as const

// The one button shape used everywhere: transparent fill, 1.5px border,
// hover turns border and text cyan. A filled button appears only where the
// action is the point.
export const button = {
  background: 'transparent',
  border: `${shape.controlBorderWidth} solid ${colour.border}`,
  color: colour.text,
  fontFamily: type.interface,
  fontWeight: type.interfaceWeight,
  borderRadius: shape.control,
  transition: `color ${motion.colour}, border-color ${motion.colour}`,
  hoverBorderColor: colour.accent,
  hoverColor: colour.accent,
} as const

// 34x18 track, 1.5px border, 12px SQUARE knob. Never an iOS pill.
export const toggle = {
  trackWidth: 34, trackHeight: 18, knobSize: 12,
  onTrackTint: '#0f2a34', onBorder: colour.accent, onKnob: colour.accent,
  offTrack: 'transparent', offBorder: colour.border, offKnob: colour.muted6,
} as const

// One control vocabulary for booleans: a 17-18px square, filled cyan when on.
export const checkbox = {
  size: 18,
  onFill: colour.accent,
  onCheck: '#071013',
  offBorder: colour.border,
  borderRadius: shape.control,
} as const

// 58px tall, surface ground, 1px hairline bottom, 28px left padding.
export const header = {
  height: 58,
  background: colour.surface,
  borderBottom: `1px solid ${colour.hairline}`,
  paddingLeft: 28,
  logoBox: 32,
  glowSize: 44,
  glow: 'radial-gradient(circle, rgba(42,192,240,0.66) 0%, rgba(42,192,240,0.205) 30%, transparent 70%)',
  windowControlWidth: 46,
  windowControlHoverBg: '#171d20',
  closeHoverBg: '#c93636',
} as const

// Every colour this file blesses, for the conformance test to assert against.
export const allColours: readonly string[] = Object.freeze([
  ...Object.values(colour),
  ...Object.values(avatarPalette),
  avatarFallback,
  avatarGlyph,
  toggle.onTrackTint,
  checkbox.onCheck,
  header.windowControlHoverBg,
  header.closeHoverBg,
])

export function isOslColour(hex: string): boolean {
  return allColours.includes(hex.toLowerCase())
}
