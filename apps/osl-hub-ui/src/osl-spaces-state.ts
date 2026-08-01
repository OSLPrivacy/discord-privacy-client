/**
 * Per-device Space preferences. The identifiers live in this module's private
 * store rather than on the handle, so serializing application state for a
 * relay or another member cannot accidentally disclose a hide/block/mute list.
 */
class SpaceLocalFilterHandle {}

export type SpaceLocalFilters = SpaceLocalFilterHandle;

export interface SpaceMessage {
  readonly id: string;
  readonly channelId: string;
  readonly senderId: string;
}

interface FilterState {
  readonly hiddenChannelIds: readonly string[];
  readonly blockedMemberIds: readonly string[];
  readonly mutedChannelIds: readonly string[];
}

const EMPTY_STATE: FilterState = Object.freeze({
  hiddenChannelIds: Object.freeze([]),
  blockedMemberIds: Object.freeze([]),
  mutedChannelIds: Object.freeze([]),
});
const states = new WeakMap<SpaceLocalFilters, FilterState>();

function filtersWith(state: FilterState): SpaceLocalFilters {
  const filters = new SpaceLocalFilterHandle();
  states.set(filters, state);
  return filters;
}

const EMPTY_FILTERS = filtersWith(EMPTY_STATE);

export function emptySpaceLocalFilters(): SpaceLocalFilters {
  return EMPTY_FILTERS;
}

function stateOf(filters: SpaceLocalFilters): FilterState {
  const state = states.get(filters);
  if (!state) throw new Error("Unknown Space local filters handle");
  return state;
}

function requireLocalId(id: string): string {
  if (id.trim().length === 0) throw new Error("Space filter identifiers must not be blank");
  return id;
}

function include(ids: readonly string[], id: string): readonly string[] {
  return ids.includes(id) ? ids : [...ids, id];
}

export function hideSpaceChannel(filters: SpaceLocalFilters, channelId: string): SpaceLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(channelId);
  return filtersWith({ ...state, hiddenChannelIds: include(state.hiddenChannelIds, id) });
}

export function unhideSpaceChannel(filters: SpaceLocalFilters, channelId: string): SpaceLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(channelId);
  return filtersWith({ ...state, hiddenChannelIds: state.hiddenChannelIds.filter((candidate) => candidate !== id) });
}

/** Blocking changes only this client's display and notification decisions. */
export function blockSpaceMember(filters: SpaceLocalFilters, memberId: string): SpaceLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(memberId);
  return filtersWith({ ...state, blockedMemberIds: include(state.blockedMemberIds, id) });
}

export function unblockSpaceMember(filters: SpaceLocalFilters, memberId: string): SpaceLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(memberId);
  return filtersWith({ ...state, blockedMemberIds: state.blockedMemberIds.filter((candidate) => candidate !== id) });
}

/** Muting leaves content available but stops this device from notifying about it. */
export function muteSpaceChannel(filters: SpaceLocalFilters, channelId: string): SpaceLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(channelId);
  return filtersWith({ ...state, mutedChannelIds: include(state.mutedChannelIds, id) });
}

export function unmuteSpaceChannel(filters: SpaceLocalFilters, channelId: string): SpaceLocalFilters {
  const state = stateOf(filters);
  const id = requireLocalId(channelId);
  return filtersWith({ ...state, mutedChannelIds: state.mutedChannelIds.filter((candidate) => candidate !== id) });
}

export function visibleSpaceChannels(filters: SpaceLocalFilters, channelIds: readonly string[]): string[] {
  const hidden = new Set(stateOf(filters).hiddenChannelIds);
  return channelIds.filter((channelId) => !hidden.has(channelId));
}

export function visibleSpaceMessages<T extends SpaceMessage>(filters: SpaceLocalFilters, messages: readonly T[]): T[] {
  const blocked = new Set(stateOf(filters).blockedMemberIds);
  return messages.filter((message) => !blocked.has(message.senderId));
}

export function shouldNotifyForSpaceMessage(filters: SpaceLocalFilters, message: SpaceMessage): boolean {
  const state = stateOf(filters);
  return !state.blockedMemberIds.includes(message.senderId)
    && !state.hiddenChannelIds.includes(message.channelId)
    && !state.mutedChannelIds.includes(message.channelId);
}
