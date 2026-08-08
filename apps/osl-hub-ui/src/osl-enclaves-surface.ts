/**
 * The five-region Enclaves surface (canon: OSLServers.dc.html, README §3).
 *
 * Left to right: 52px app rail · 64px enclave rail · 232px channel sidebar ·
 * channel pane (header / intro / ledger / composer) · 216px member list.
 * Clicking a member opens the profile card modal.
 *
 * Every piece of room data on this screen flows through the existing state
 * layer rather than being read straight off the fixtures:
 *   - `enclaveServerScreen` (osl-enclaves-server-access.ts) gates the message,
 *     member and channel lists on the local user's membership; a non-member
 *     renders the refusal, not the rooms.
 *   - `channelAttention` / `markChannelRead` (osl-enclaves-state.ts) derive the
 *     sidebar unread counts from this device's own read frontier.
 *   - `visibleSpaceChannels` / `visibleSpaceMessages` apply the local
 *     hide/block filters before anything reaches the markup.
 *   - `projectKnownSpaceMembers` (osl-enclaves-members.ts) validates the
 *     roster (no blanks, no duplicates) before it becomes rows. Presence is
 *     deliberately joined on afterwards from per-space presence facts and
 *     rendered in Consolas: it is a statement the system asserts, not decor.
 *
 * No inline styles anywhere: the shipped CSP is `style-src 'self'`, so all
 * styling lives in osl-enclaves-surface.css (colours transcribed from
 * osl-tokens.ts).
 */
import "./osl-enclaves-surface.css";

import {
  channelAttention,
  createOslEnclavesLocalState,
  emptyEnclaveLocalFilters,
  markChannelRead,
  visibleSpaceChannels,
  visibleSpaceMessages,
  type EnclaveLocalFilters,
  type OslEnclavesLocalState,
  type OslEnclaveStoredMessage,
} from "./osl-enclaves-state";
import {
  enclaveChannelsWithKeyChanges,
  enclaveServerScreen,
  isOslKeyChangesChannelId,
  OSL_KEY_CHANGES_CHANNEL_ID,
  OSL_KEY_CHANGES_CHANNEL_NOTE,
  requestEnclaveMessagePost,
  type EnclaveServerSnapshot,
} from "./osl-enclaves-server-access";
import { projectKnownSpaceMembers, type KnownSpaceMember } from "./osl-enclaves-members";

// ---------------------------------------------------------------------------
// Fixtures — ported from the design prototype (OSLServers.dc.html). These are
// the demo rooms the surface ships with until the Enclave transport feeds it.
// ---------------------------------------------------------------------------

type EnclaveRole = "" | "OWNER" | "MOD" | "SYSTEM";
type Presence = "ONLINE" | "REVERIFY" | "AWAY" | "OFFLINE";

interface SurfaceMessage {
  readonly messageId: string;
  /** `SpaceMessage.id` for the local block/hide filter layer. */
  readonly id: string;
  /** A key-change record belongs to one enclave; ordinary fixture rows are shared by name. */
  readonly spaceId?: string;
  readonly channelId: string;
  readonly senderId: string;
  readonly author: string;
  readonly role: EnclaveRole;
  readonly time: string;
  readonly body: string;
}

interface SurfaceChannel {
  readonly channelId: string;
  readonly name: string;
  readonly glyph: string;
  readonly topic: string;
  readonly intro: string;
  readonly kind?: "text" | "osl-key-changes";
}

interface RosterEntry {
  readonly name: string;
  readonly presence: Presence;
}

interface RosterGroup {
  readonly label: string;
  readonly people: readonly RosterEntry[];
}

interface SurfaceSpace {
  readonly id: string;
  readonly name: string;
  readonly sub: string;
  readonly kind: "enclave";
  readonly memberCount: number;
  readonly foot: string;
  readonly channels: readonly SurfaceChannel[];
  readonly roster: readonly RosterGroup[];
}

interface PersonFact {
  readonly handle: string;
  readonly state: string;
  readonly tone: "safe" | "warn" | "muted";
  readonly bio: string;
}

/** The local user. Rooms render only what this member may read. */
const LOCAL_USER = "Kestrel";

const SPACES: readonly SurfaceSpace[] = [
  {
    id: "frontier",
    name: "Frontier",
    sub: "34 members · invite only",
    kind: "enclave",
    memberCount: 34,
    foot: "Every message here is encrypted to the member list. Adding someone re-keys the room.",
    channels: [
      { channelId: "general", name: "general", glyph: "#", topic: "Anything, as long as it stays in here", intro: "The start of #general. Everyone here has a verified key. If someone's key changes, OSL says so in the room before you type." },
      { channelId: "builds", name: "builds", glyph: "#", topic: "Signed builds only", intro: "Only signed builds get posted here. OSL checks the signature and shows the signer, not just the filename." },
    ],
    roster: [
      { label: "OWNER", people: [{ name: "Mara", presence: "ONLINE" }] },
      { label: "MODS", people: [{ name: "Kit", presence: "ONLINE" }] },
      { label: "MEMBERS — 32", people: [
        { name: "Ravi", presence: "ONLINE" },
        { name: "Tess", presence: "REVERIFY" },
        { name: "Jae", presence: "AWAY" },
        { name: "Sol", presence: "OFFLINE" },
      ] },
    ],
  },
  {
    id: "ward",
    name: "Ward",
    sub: "8 members · closed",
    kind: "enclave",
    memberCount: 8,
    foot: "Small enclave. Every join is approved by two existing members.",
    channels: [
      { channelId: "ops", name: "ops", glyph: "#", topic: "Day to day", intro: "Eight people, two approvals to add a ninth. OSL enforces that at the key level, not just in the interface." },
      { channelId: "incidents", name: "incidents", glyph: "#", topic: "When something breaks", intro: "Incidents get their own room so the timer here can be short without losing everything else." },
    ],
    roster: [
      { label: "OWNER", people: [{ name: "Dara", presence: "ONLINE" }] },
      { label: "MEMBERS — 7", people: [
        { name: "Sol", presence: "ONLINE" },
        { name: "Ravi", presence: "OFFLINE" },
      ] },
    ],
  },
];

const MESSAGES: readonly SurfaceMessage[] = ([
  { messageId: "f1", channelId: "general", senderId: "Mara", author: "Mara", role: "OWNER", time: "09:12", body: "Pushed the new key-change banner. It reads the room roster, not the chat history, so it fires before you type instead of after." },
  { messageId: "f2", channelId: "general", senderId: "Ravi", author: "Ravi", role: "", time: "09:14", body: "Tested it on a rotated key. The banner showed and the encrypt chip greyed out. That's the right shape." },
  { messageId: "f3", channelId: "general", senderId: "Kit", author: "Kit", role: "MOD", time: "09:20", body: "One nit: it says “key changed” but not when. People will assume it just happened." },
  { messageId: "f4", channelId: "general", senderId: "Mara", author: "Mara", role: "OWNER", time: "09:21", body: "Fair. Adding the timestamp and who was in the room when it changed." },
  { messageId: "f5", channelId: "general", senderId: "Tess", author: "Tess", role: "", time: "09:38", body: "Can we get the same banner in DMs? That's where it actually matters for me." },
  { messageId: "f6", channelId: "builds", senderId: "Kit", author: "Kit", role: "MOD", time: "Tue", body: "osl-desktop 0.9.4 — signed with the release key, fingerprint ends 4C7A. Reproducible from the tag." },
  { messageId: "f7", channelId: "builds", senderId: "Mara", author: "Mara", role: "OWNER", time: "Tue", body: "Verified the fingerprint against the one in my notes. Matches." },
  { messageId: "f8", channelId: "builds", senderId: "Jae", author: "Jae", role: "", time: "Wed", body: "Built it myself from the tag and got the same hash." },
  { messageId: "f9", spaceId: "frontier", channelId: OSL_KEY_CHANGES_CHANNEL_ID, senderId: "OSL", author: "OSL", role: "SYSTEM", time: "Mon", body: "Tess added a device. Her key changed. Verify the new safety number before you treat old messages as trusted." },
  { messageId: "f10", spaceId: "frontier", channelId: OSL_KEY_CHANGES_CHANNEL_ID, senderId: "OSL", author: "OSL", role: "SYSTEM", time: "Thu", body: "Ravi's key was rotated on a device that was already verified. No action needed — this is the record." },
  { messageId: "w1", channelId: "ops", senderId: "Dara", author: "Dara", role: "OWNER", time: "08:02", body: "Rotating the shared invite tonight. The old link stops working at 23:00, no grace period." },
  { messageId: "w2", channelId: "ops", senderId: "Sol", author: "Sol", role: "", time: "08:19", body: "Understood. I'll tell the two people who still have it saved." },
  { messageId: "w3", channelId: "incidents", senderId: "Sol", author: "Sol", role: "", time: "Fri", body: "Delivery stalled for about nine minutes. OSL held the messages encrypted and retried. Nothing was dropped and nothing went out in the clear." },
] satisfies readonly Omit<SurfaceMessage, "id">[]).map((message) => ({ ...message, id: message.messageId }));

const PEOPLE: Readonly<Record<string, PersonFact>> = {
  Mara: { handle: "@mara", state: "VERIFIED · 2 DEVICES", tone: "safe", bio: "Runs Frontier. Writes the notices nobody reads until something breaks." },
  Ravi: { handle: "@ravi", state: "VERIFIED", tone: "safe", bio: "Tests the thing before it ships. Usually finds the one case you forgot." },
  Kit: { handle: "@kit", state: "VERIFIED · MOD", tone: "safe", bio: "Signs the builds. Will ask for the fingerprint." },
  Tess: { handle: "@tess", state: "KEY CHANGED MON · REVERIFY", tone: "warn", bio: "New device this week. Compare safety numbers before you trust old threads." },
  Jae: { handle: "@jae", state: "VERIFIED", tone: "safe", bio: "Builds from source, every time." },
  Dara: { handle: "@dara", state: "VERIFIED · OWNER", tone: "safe", bio: "Ward, eight people, no exceptions." },
  Sol: { handle: "@sol", state: "VERIFIED", tone: "safe", bio: "On call when delivery stalls." },
  OSL: { handle: "system", state: "SYSTEM ACCOUNT", tone: "muted", bio: "OSL writes key-change records. It cannot read your messages." },
  Kestrel: { handle: "@kestrel", state: "THIS IS YOU", tone: "safe", bio: "Your own key, on this device." },
};

// ---------------------------------------------------------------------------
// State-layer plumbing. Access gating, read frontier, local filters.
// ---------------------------------------------------------------------------

function snapshotFor(space: SurfaceSpace): EnclaveServerSnapshot {
  const memberNames = new Set<string>([LOCAL_USER]);
  for (const group of space.roster) for (const person of group.people) memberNames.add(person.name);
  return {
    serverId: space.id,
    messages: MESSAGES
      .filter((message) => (
        messageSpaceId(message) === space.id
        && channelsFor(space).some((channel) => channel.channelId === message.channelId)
      ))
      .map((message) => ({ messageId: message.messageId, channelId: message.channelId, senderId: message.senderId, plaintext: message.body })),
    members: [...memberNames].map((name) => ({ memberId: name, displayName: name })),
    channels: channelsFor(space).map((channel) => ({ channelId: channel.channelId, name: channel.name, kind: channel.kind })),
  };
}

const OSL_KEY_CHANGES_SURFACE_CHANNEL: SurfaceChannel = Object.freeze({
  channelId: OSL_KEY_CHANGES_CHANNEL_ID,
  name: "key-changes",
  glyph: "#",
  topic: "AUTOMATIC · OSL AUTHORED",
  intro: "OSL writes key-change records here. This record cannot be posted in or deleted by any enclave role.",
  kind: "osl-key-changes",
});

/** Materialize the mandatory channel for every enclave before it enters UI state. */
function channelsFor(space: SurfaceSpace): readonly SurfaceChannel[] {
  const byId = new Map(space.channels.map((channel) => [channel.channelId, channel]));
  return enclaveChannelsWithKeyChanges(space.channels).map((channel) => (
    isOslKeyChangesChannelId(channel.channelId)
      ? OSL_KEY_CHANGES_SURFACE_CHANNEL
      : byId.get(channel.channelId) ?? { ...channel, glyph: "#", topic: "", intro: "" }
  ));
}

function messageSpaceId(message: SurfaceMessage): string {
  if (message.spaceId) return message.spaceId;
  return message.channelId === "ops" || message.channelId === "incidents" ? "ward" : "frontier";
}

function scopedChannelId(spaceId: string, channelId: string): string {
  return `${spaceId}:${channelId}`;
}

/** The read frontier lives on this device only (osl-enclaves-state.ts). */
let localState: OslEnclavesLocalState = createOslEnclavesLocalState();
/** Hide/block/mute filters — empty until a control writes to them. */
const filters: EnclaveLocalFilters = emptyEnclaveLocalFilters();

/** Fixture messages as the message store's receipt-ordered records. */
const storedMessages: OslEnclaveStoredMessage[] = MESSAGES.map((message, index) => ({
  messageId: message.messageId,
  channelId: scopedChannelId(messageSpaceId(message), message.channelId),
  localSequence: index + 1,
  incoming: message.senderId !== LOCAL_USER,
  mentionsLocalUser: false,
}));
let nextLocalSequence = storedMessages.length + 1;

// ---------------------------------------------------------------------------
// View state (per device, module-local; survives shell re-renders).
// ---------------------------------------------------------------------------

interface ViewState {
  spaceId: string;
  channelBySpace: Map<string, string>;
  aboutOpen: boolean;
  draft: string;
  card: string | null;
  sent: Map<string, { readonly body: string }[]>;
}

const view: ViewState = {
  spaceId: SPACES[0].id,
  channelBySpace: new Map(),
  aboutOpen: false,
  draft: "",
  card: null,
  sent: new Map(),
};

if (typeof location !== "undefined" && typeof location.search === "string") {
  const query = new URLSearchParams(location.search);
  const requested = query.get("space");
  if (requested && SPACES.some((space) => space.id === requested)) view.spaceId = requested;
  const requestedChannel = query.get("channel");
  if (requestedChannel && channelsFor(activeSpace()).some((channel) => channel.channelId === requestedChannel)) {
    view.channelBySpace.set(view.spaceId, requestedChannel);
  }
}

function activeSpace(): SurfaceSpace {
  return SPACES.find((space) => space.id === view.spaceId) ?? SPACES[0];
}

function activeChannel(space: SurfaceSpace): SurfaceChannel {
  const wanted = view.channelBySpace.get(space.id);
  const channels = channelsFor(space);
  return channels.find((channel) => channel.channelId === wanted) ?? channels[0];
}

// Opening the surface reads the first channel: record that on the frontier.
localState = markChannelRead(storedMessages, localState, scopedChannelId(activeSpace().id, activeChannel(activeSpace()).channelId));

// ---------------------------------------------------------------------------
// Markup helpers
// ---------------------------------------------------------------------------

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

const AVATAR_KEYS = new Set(["m", "r", "k", "t", "j", "d", "s"]);

/** Deterministic avatar class from the first letter (osl-tokens avatarPalette). */
function avatarClass(name: string): string {
  const initial = (name.trim()[0] ?? "").toLowerCase();
  return AVATAR_KEYS.has(initial) ? `enclv-av-${initial}` : "enclv-av-x";
}

function initialOf(name: string): string {
  return (name.trim()[0] ?? "?").toUpperCase();
}

function presenceClass(presence: Presence): string {
  if (presence === "ONLINE") return "enclv-presence-online";
  if (presence === "REVERIFY") return "enclv-presence-reverify";
  if (presence === "AWAY") return "enclv-presence-away";
  return "enclv-presence-offline";
}

function roleChip(role: EnclaveRole): string {
  if (!role) return "";
  const cls = role === "OWNER" ? "enclv-role-owner" : role === "MOD" ? "enclv-role-mod" : "enclv-role-system";
  return `<span class="enclv-role-chip ${cls}">${role}</span>`;
}

// ---------------------------------------------------------------------------
// Regions
// ---------------------------------------------------------------------------

function appRailMarkup(): string {
  return `<nav class="enclv-apprail" aria-label="OSL apps">`
    + `<button class="enclv-apprail-btn" type="button" data-route="inbox" title="Chats" aria-label="Open Chats"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5.5h16v10H9l-5 4v-14Z"/><path d="M8 9h8M8 12h5"/></svg></button>`
    + `<button class="enclv-apprail-btn active" type="button" title="Enclaves" aria-label="Enclaves" aria-current="page"><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="4" width="7" height="7"/><rect x="13" y="4" width="7" height="7"/><rect x="4" y="13" width="7" height="7"/><rect x="13" y="13" width="7" height="7"/></svg></button>`
    + `</nav>`;
}

function enclaveRailMarkup(): string {
  const tiles = SPACES.map((space) => {
    const active = space.id === view.spaceId;
    return `<button class="enclv-tile ${active ? "active" : ""}" type="button" data-enclave-space="${escapeHtml(space.id)}" title="${escapeHtml(space.name)}" ${active ? 'aria-current="true"' : ""}>${active ? '<span class="enclv-tile-notch" aria-hidden="true"></span>' : ""}${escapeHtml(initialOf(space.name))}</button>`;
  }).join("");
  const add = `<button class="enclv-tile enclv-tile-add" type="button" aria-disabled="true" title="Joining or creating an enclave is not wired to the key service in this build yet">+</button>`;
  return `<nav class="enclv-rail" aria-label="Your enclaves">${tiles}${add}</nav>`;
}

function sidebarMarkup(space: SurfaceSpace, channel: SurfaceChannel, channelIds: readonly string[]): string {
  const rows = channelsFor(space)
    .filter((candidate) => channelIds.includes(candidate.channelId))
    .map((candidate) => {
      const active = !view.aboutOpen && candidate.channelId === channel.channelId;
      const attention = channelAttention(storedMessages, localState, scopedChannelId(space.id, candidate.channelId));
      const unread = attention.unreadCount > 0 && !active;
      const badge = unread ? `<span class="enclv-chan-badge">${attention.unreadCount}</span>` : "";
      const system = candidate.kind === "osl-key-changes";
      const title = system ? "OSL cannot accept posts or deletion in its own key-changes log." : "";
      const status = system ? `<span class="enclv-chan-system-state">OSL WRITES</span>` : "";
      return `<button class="enclv-chan ${active ? "active" : ""} ${unread ? "unread" : ""} ${system ? "system" : ""}" type="button" data-enclave-channel="${escapeHtml(candidate.channelId)}" title="${title}"><span class="enclv-chan-glyph">${escapeHtml(candidate.glyph)}</span><span class="enclv-chan-name">${escapeHtml(candidate.name)}</span>${status}${badge}</button>`;
    }).join("");
  const about = `<button class="enclv-about-link ${view.aboutOpen ? "active" : ""}" type="button" data-enclave-about title="What OSL can and cannot do inside Enclaves">About Enclaves</button>`;
  return `<div class="enclv-sidebar">`
    + `<header class="enclv-space-head"><div class="enclv-space-name">${escapeHtml(space.name)}</div><div class="enclv-space-sub">${escapeHtml(space.sub)}</div></header>`
    + `<div class="enclv-chanlist"><div class="enclv-group-label">TEXT</div>${rows}</div>`
    + `<footer class="enclv-space-foot">${escapeHtml(space.foot)}${about}</footer>`
    + `</div>`;
}

function messageRowMarkup(message: SurfaceMessage): string {
  const system = message.role === "SYSTEM";
  return `<div class="enclv-msg">`
    + `<button class="enclv-msg-avatar ${avatarClass(message.author)}" type="button" data-enclave-member="${escapeHtml(message.author)}" aria-label="Open ${escapeHtml(message.author)}'s profile">${escapeHtml(initialOf(message.author))}</button>`
    + `<div class="enclv-msg-body"><div class="enclv-msg-head"><button class="enclv-msg-author ${system ? "system" : ""}" type="button" data-enclave-member="${escapeHtml(message.author)}">${escapeHtml(message.author)}</button>${roleChip(message.role)}<span class="enclv-msg-time">${escapeHtml(message.time)}</span></div>`
    + `<div class="enclv-msg-text">${escapeHtml(message.body)}</div></div>`
    + `</div>`;
}

function sentRowMarkup(body: string): string {
  return `<div class="enclv-msg">`
    + `<span class="enclv-msg-avatar ${avatarClass(LOCAL_USER)}">${escapeHtml(initialOf(LOCAL_USER))}</span>`
    + `<div class="enclv-msg-body"><div class="enclv-msg-head"><span class="enclv-msg-author">${escapeHtml(LOCAL_USER)}</span><span class="enclv-msg-time">now</span></div>`
    + `<div class="enclv-msg-text">${escapeHtml(body)}</div></div>`
    + `</div>`;
}

function channelPaneMarkup(space: SurfaceSpace, channel: SurfaceChannel, stateNotices: string): string {
  const sealLabel = `SEALED TO ${space.memberCount}`;
  const header = `<header class="enclv-chan-head"><span class="enclv-chan-head-glyph">${escapeHtml(channel.glyph)}</span><span class="enclv-chan-head-name">${escapeHtml(channel.name)}</span><span class="enclv-chan-head-div"></span><span class="enclv-chan-head-topic">${escapeHtml(channel.topic)}</span><span class="enclv-seal-chip">${sealLabel}</span></header>`;

  // Membership-gated reads: a non-member gets the refusal, never the rooms.
  const screen = space.kind === "enclave" ? enclaveServerScreen(snapshotFor(space), LOCAL_USER) : null;
  if (screen && screen.refusals.length) {
    return `<section class="enclv-pane">${header}<div class="enclv-scroll"><p class="enclv-refusal">${escapeHtml(screen.refusals[0])}</p></div></section>`;
  }
  const readable = new Set((screen?.messageRows ?? []).map((row) => row.messageId));

  const channelRecords = MESSAGES
    .filter((message) => message.channelId === channel.channelId && readable.has(message.messageId));
  const rows = visibleSpaceMessages(filters, channelRecords).map((message) => messageRowMarkup(message)).join("");
  const sent = (view.sent.get(`${space.id}:${channel.channelId}`) ?? []).map((message) => sentRowMarkup(message.body)).join("");
  const body = `<div class="enclv-ledger">`
    + `<div class="enclv-intro"><div class="enclv-intro-title">${escapeHtml(channel.glyph)}${escapeHtml(channel.name)}</div><div class="enclv-intro-text">${escapeHtml(channel.intro)}</div></div>`
    + rows + sent
    + `</div>`;

  let composer: string;
  if (channel.kind === "osl-key-changes") {
    composer = `<div class="enclv-composer disabled"><input class="enclv-composer-input" type="text" disabled title="OSL cannot accept posts into its own key-changes log." placeholder="OSL-authored key-change log" aria-label="OSL-authored key-change log, posting refused"/><span class="enclv-composer-hint">OSL WRITES</span></div><p class="enclv-readonly-note">${escapeHtml(OSL_KEY_CHANGES_CHANNEL_NOTE)}</p>`;
  } else {
    composer = `<div class="enclv-composer"><input class="enclv-composer-input" type="text" data-enclave-draft value="${escapeHtml(view.draft)}" placeholder="Message ${escapeHtml(channel.glyph)}${escapeHtml(channel.name)}" aria-label="Message ${escapeHtml(channel.glyph)}${escapeHtml(channel.name)}"/><span class="enclv-composer-hint">SEALED TO ${space.memberCount}</span></div>`;
  }

  return `<section class="enclv-pane">${header}<div class="enclv-scroll">${stateNotices}${body}</div><div class="enclv-composer-row">${composer}</div></section>`;
}

function memberListMarkup(space: SurfaceSpace): string {
  // Validate the roster through the members projection before rendering, and
  // gate the whole list on this user's membership.
  const known: KnownSpaceMember[] = [];
  const presenceOf = new Map<string, Presence>();
  for (const group of space.roster) for (const person of group.people) {
    if (!presenceOf.has(person.name)) {
      known.push({ memberId: person.name, displayName: person.name });
      presenceOf.set(person.name, person.presence);
    }
  }
  const projected = projectKnownSpaceMembers(known);
  const allowed = new Set(enclaveServerScreen(snapshotFor(space), LOCAL_USER).memberRows.map((row) => row.memberId));
  const labelById = new Map(projected.rows.map((row) => [row.memberId, row.label]));

  const groups = space.roster.map((group) => {
    const rows = group.people
      .filter((person) => allowed.has(person.name))
      .map((person) => {
        const presence = presenceOf.get(person.name) ?? "OFFLINE";
        return `<button class="enclv-member ${presence === "OFFLINE" ? "offline" : ""}" type="button" data-enclave-member="${escapeHtml(person.name)}"><span class="enclv-member-avatar ${avatarClass(person.name)}">${escapeHtml(initialOf(person.name))}</span><span class="enclv-member-id"><span class="enclv-member-name">${escapeHtml(labelById.get(person.name) ?? person.name)}</span><span class="enclv-member-presence ${presenceClass(presence)}">${presence}</span></span></button>`;
      }).join("");
    return `<div class="enclv-group-label">${escapeHtml(group.label)}</div>${rows}`;
  }).join("");
  return `<aside class="enclv-members" aria-label="Members">${groups}</aside>`;
}

function profileCardMarkup(): string {
  if (!view.card) return "";
  const name = view.card;
  const facts = PEOPLE[name] ?? { handle: "", state: "", tone: "muted" as const, bio: "" };
  const toneClass = facts.tone === "safe" ? "enclv-state-safe" : facts.tone === "warn" ? "enclv-state-warn" : "enclv-state-muted";
  return `<div class="enclv-card-overlay" data-enclave-card-close role="presentation"><section class="enclv-card" role="dialog" aria-modal="true" aria-label="Profile: ${escapeHtml(name)}">`
    + `<div class="enclv-card-banner ${avatarClass(name)}-banner"></div>`
    + `<div class="enclv-card-body"><span class="enclv-card-avatar ${avatarClass(name)}">${escapeHtml(initialOf(name))}</span>`
    + `<div class="enclv-card-name">${escapeHtml(name)}</div>`
    + `<div class="enclv-card-handle">${escapeHtml(facts.handle)}</div>`
    + `<div class="enclv-card-state ${toneClass}">${escapeHtml(facts.state)}</div>`
    + `<div class="enclv-card-bio">${escapeHtml(facts.bio)}</div>`
    + `<div class="enclv-card-actions"><button class="enclv-card-btn" type="button" disabled title="Profiles do not have pages in this build yet; OSL has nowhere to take you">View page</button><button class="enclv-card-btn" type="button" data-enclave-card-message>Message</button></div>`
    + `</div></section></div>`;
}

function aboutMarkup(statusTag: (label: string) => string): string {
  return `<section class="enclv-about" aria-label="About OSL Enclaves" ${view.aboutOpen ? "" : "hidden"}>`
    + `<div class="enclv-about-inner"><h2>About Enclaves</h2>`
    + `<p>Enclaves are OSL's encrypted communities for the members you choose.</p>`
    + `<section class="settings-list" aria-label="OSL Enclaves"><div class="setting-line"><span><strong>Enclaves</strong><small>Create and use encrypted Enclaves with their own membership.</small></span>${statusTag("Available")}</div></section>`
    + `<p class="scope-approval-note">OSL Enclaves are separate from third-party communities. OSL does not claim access to provider communities or read provider pages.</p>`
    + `</div></section>`;
}

// ---------------------------------------------------------------------------
// Assembly + re-render
// ---------------------------------------------------------------------------

export interface OslEnclavesRegionModel {
  readonly stateNotices: string;
  readonly statusTag: (label: string) => string;
  readonly roleEditorMarkup?: string;
}

let lastModel: OslEnclavesRegionModel | null = null;

function dynamicMarkup(model: OslEnclavesRegionModel): string {
  const space = activeSpace();
  const channelIds = visibleSpaceChannels(filters, channelsFor(space).map((channel) => channel.channelId));
  const channel = activeChannel(space);
  const pane = view.aboutOpen ? aboutMarkup(model.statusTag) : channelPaneMarkup(space, channel, model.stateNotices) + aboutMarkup(model.statusTag);
  return enclaveRailMarkup()
    + sidebarMarkup(space, channel, channelIds)
    + pane
    + (view.aboutOpen ? "" : memberListMarkup(space))
    + profileCardMarkup();
}

/** The whole five-region surface. `main.ts` reaches this via osl-enclaves.ts. */
export function oslEnclavesFiveRegionSurface(model: OslEnclavesRegionModel): string {
  lastModel = model;
  return `<main class="content-viewport osl-enclaves-page"><h1 id="route-heading" class="sr-only" tabindex="-1">OSL Enclaves</h1>${appRailMarkup()}<div class="enclv-dynamic" data-enclave-root>${dynamicMarkup(model)}</div>${model.roleEditorMarkup ?? ""}</main>`;
}

function rerender(): void {
  const root = document.querySelector<HTMLElement>("[data-enclave-root]");
  if (root && lastModel) root.innerHTML = dynamicMarkup(lastModel);
}

function openChannel(channelId: string): void {
  const space = activeSpace();
  if (!channelsFor(space).some((channel) => channel.channelId === channelId)) return;
  view.channelBySpace.set(space.id, channelId);
  view.aboutOpen = false;
  view.draft = "";
  localState = markChannelRead(storedMessages, localState, scopedChannelId(space.id, channelId));
  rerender();
}

function sendDraft(): void {
  const body = view.draft.trim();
  if (!body) return;
  const space = activeSpace();
  const channel = activeChannel(space);
  if (!requestEnclaveMessagePost(snapshotFor(space), LOCAL_USER, channel.channelId).ok) return;
  const key = `${space.id}:${channel.channelId}`;
  const sent = view.sent.get(key) ?? [];
  sent.push({ body });
  view.sent.set(key, sent);
  storedMessages.push({
    messageId: `local-${nextLocalSequence}`,
    channelId: scopedChannelId(space.id, channel.channelId),
    localSequence: nextLocalSequence,
    incoming: false,
    mentionsLocalUser: false,
  });
  nextLocalSequence += 1;
  view.draft = "";
  rerender();
  document.querySelector<HTMLInputElement>("[data-enclave-draft]")?.focus();
}

function onSurfaceClick(event: Event): void {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const spaceTile = target.closest<HTMLElement>("[data-enclave-space]");
  if (spaceTile?.dataset.enclaveSpace) {
    const spaceId = spaceTile.dataset.enclaveSpace;
    if (SPACES.some((space) => space.id === spaceId)) {
      view.spaceId = spaceId;
      view.aboutOpen = false;
      view.draft = "";
      localState = markChannelRead(storedMessages, localState, scopedChannelId(activeSpace().id, activeChannel(activeSpace()).channelId));
      rerender();
    }
    return;
  }
  const channelButton = target.closest<HTMLElement>("[data-enclave-channel]");
  if (channelButton?.dataset.enclaveChannel) { openChannel(channelButton.dataset.enclaveChannel); return; }
  if (target.closest("[data-enclave-about]")) { view.aboutOpen = true; rerender(); return; }
  if (target.closest("[data-enclave-card-message]")) {
    view.card = null;
    rerender();
    // Navigation is owned by main.ts's [data-route] binding on the app rail.
    document.querySelector<HTMLButtonElement>(".enclv-apprail-btn[data-route]")?.click();
    return;
  }
  const member = target.closest<HTMLElement>("[data-enclave-member]");
  if (member?.dataset.enclaveMember) { view.card = member.dataset.enclaveMember; rerender(); return; }
  const overlay = target.closest("[data-enclave-card-close]");
  if (overlay && !target.closest(".enclv-card")) { view.card = null; rerender(); }
}

function onSurfaceInput(event: Event): void {
  const target = event.target;
  if (target instanceof HTMLInputElement && target.hasAttribute("data-enclave-draft")) view.draft = target.value;
}

function onSurfaceKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape" && view.card) { view.card = null; rerender(); return; }
  const target = event.target;
  if (event.key === "Enter" && target instanceof HTMLInputElement && target.hasAttribute("data-enclave-draft")) {
    event.preventDefault();
    sendDraft();
  }
}

// One document-level delegation, registered once. The surface region re-renders
// itself in place, so the listeners must not live on elements inside it.
if (typeof document !== "undefined" && typeof document.addEventListener === "function") {
  document.addEventListener("click", onSurfaceClick);
  document.addEventListener("input", onSurfaceInput);
  document.addEventListener("keydown", onSurfaceKeydown as EventListener);
}
