import type { ScopePolicy } from "./scrub-delete-engine";

export interface ProtectedSets {
  channelIds: readonly string[];
  correspondentIds: readonly string[];
}

const emptyProtectedSets: ProtectedSets = Object.freeze({ channelIds: Object.freeze([]), correspondentIds: Object.freeze([]) });

function canonical(values: readonly string[]): readonly string[] {
  return Object.freeze([...new Set(values)].sort());
}

function snapshot(channelIds: Iterable<string>, correspondentIds: Iterable<string>): ProtectedSets {
  return Object.freeze({ channelIds: canonical([...channelIds]), correspondentIds: canonical([...correspondentIds]) });
}

/**
 * Per-account protection history for Scrub. A run can add protection, but cannot
 * remove it: explicit unprotection is intentionally outside this batch API.
 */
export class ScrubProtectionHistory {
  private readonly channelIds: Set<string>;
  private readonly correspondentIds: Set<string>;

  constructor(initial: ProtectedSets = emptyProtectedSets) {
    this.channelIds = new Set(initial.channelIds);
    this.correspondentIds = new Set(initial.correspondentIds);
  }

  current(): ProtectedSets {
    return snapshot(this.channelIds, this.correspondentIds);
  }

  protect(additions: ProtectedSets): ProtectedSets {
    for (const id of additions.channelIds) this.channelIds.add(id);
    for (const id of additions.correspondentIds) this.correspondentIds.add(id);
    return this.current();
  }

  /** Records protections named during this run, then returns a scope with all inherited protections. */
  inherit(scope: ScopePolicy): ScopePolicy {
    const protections = this.protect({
      channelIds: scope.protectedChannelIds,
      correspondentIds: scope.protectedCorrespondentIds,
    });
    return {
      ...scope,
      protectedChannelIds: protections.channelIds,
      protectedCorrespondentIds: protections.correspondentIds,
    };
  }
}
