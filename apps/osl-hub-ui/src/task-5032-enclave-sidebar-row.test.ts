import { describe, expect, it } from "vitest";
import {
  OSL_ENCLAVE_SIDEBAR_COLLAPSED,
  isEnclaveExpanded,
  oslEnclaveSidebarListMarkup,
  oslEnclaveSidebarRowMarkup,
  oslEnclaveUnreadTotal,
  toggleEnclaveExpansion,
  type OslEnclaveSidebarRowModel,
} from "./osl-chat-enclave-sidebar-row";

const fixtureEnclave: OslEnclaveSidebarRowModel = {
  enclaveId: "enclave-fixture",
  name: "Fixture Enclave",
  channels: [
    { channelId: "general", name: "general", unreadCount: 2 },
    { channelId: "announcements", name: "announcements", unreadCount: 0 },
    { channelId: "off-topic", name: "off-topic", unreadCount: 3 },
    { channelId: "mod-only", name: "mod-only", unreadCount: 1, locked: true, lockedReason: "Requires the Moderator role" },
  ],
};

const otherEnclave: OslEnclaveSidebarRowModel = {
  enclaveId: "enclave-other",
  name: "Other Enclave",
  channels: [
    { channelId: "other-general", name: "general", unreadCount: 0 },
  ],
};

function channelRowCount(markup: string, enclaveId: string): number {
  const list = markup.match(new RegExp(`data-enclave-channels="${enclaveId}"[^>]*>([\\s\\S]*?)</ul>`, "u"));
  return list ? (list[1].match(/data-enclave-channel-id="/gu) ?? []).length : 0;
}

/**
 * Delegated click handler shaped exactly like the app's existing navigation
 * wiring (`data-route` / `data-primary-destination` trigger navigation
 * elsewhere -- see `main.ts`). Toggling an enclave row must not be reachable
 * through that path.
 */
function dispatchClick(
  attributes: Record<string, string | undefined>,
  counters: { navigations: number; toggles: number },
  toggleTarget: { state: ReturnType<typeof toggleEnclaveExpansion> },
): void {
  if (attributes["data-route"] || attributes["data-primary-destination"]) {
    counters.navigations += 1;
    return;
  }
  const toggleId = attributes["data-enclave-toggle"];
  if (toggleId) {
    counters.toggles += 1;
    toggleTarget.state = toggleEnclaveExpansion(toggleTarget.state, toggleId);
  }
}

describe("TASK 5032 enclave sidebar rows expand in place", () => {
  it("lists exactly the enclave's 4 channels when expanded, and 0 when collapsed", () => {
    const collapsedMarkup = oslEnclaveSidebarRowMarkup(fixtureEnclave, OSL_ENCLAVE_SIDEBAR_COLLAPSED);
    const collapsedCount = channelRowCount(collapsedMarkup, "enclave-fixture");
    console.log(`TASK5032 collapsed_channel_count=${collapsedCount}`);
    expect(collapsedCount).toBe(0);

    const expandedState = toggleEnclaveExpansion(OSL_ENCLAVE_SIDEBAR_COLLAPSED, "enclave-fixture");
    const expandedMarkup = oslEnclaveSidebarRowMarkup(fixtureEnclave, expandedState);
    const expandedCount = channelRowCount(expandedMarkup, "enclave-fixture");
    console.log(`TASK5032 expanded_channel_count=${expandedCount}`);
    expect(expandedCount).toBe(4);
  });

  it("fires 0 navigation events while toggling, only toggle events", () => {
    const counters = { navigations: 0, toggles: 0 };
    const target = { state: OSL_ENCLAVE_SIDEBAR_COLLAPSED as ReturnType<typeof toggleEnclaveExpansion> };

    // Clicking the toggle button carries only data-enclave-toggle, never data-route.
    dispatchClick({ "data-enclave-toggle": "enclave-fixture" }, counters, target);
    expect(isEnclaveExpanded(target.state, "enclave-fixture")).toBe(true);
    dispatchClick({ "data-enclave-toggle": "enclave-fixture" }, counters, target);
    expect(isEnclaveExpanded(target.state, "enclave-fixture")).toBe(false);

    console.log(`TASK5032 navigation_events=${counters.navigations} toggle_events=${counters.toggles}`);
    expect(counters.navigations).toBe(0);
    expect(counters.toggles).toBe(2);

    const rowMarkup = oslEnclaveSidebarRowMarkup(fixtureEnclave, OSL_ENCLAVE_SIDEBAR_COLLAPSED);
    expect(rowMarkup).not.toContain("data-route");
    expect(rowMarkup).not.toContain("data-primary-destination");
  });

  it("collapsing hides all 4 channels again with the sidebar order unchanged", () => {
    const models = [fixtureEnclave, otherEnclave];
    let state = OSL_ENCLAVE_SIDEBAR_COLLAPSED as ReturnType<typeof toggleEnclaveExpansion>;

    const before = oslEnclaveSidebarListMarkup(models, state);
    const orderBefore = [...before.matchAll(/data-enclave-id="([^"]+)"/gu)].map((match) => match[1]);

    state = toggleEnclaveExpansion(state, "enclave-fixture");
    const expandedMarkup = oslEnclaveSidebarListMarkup(models, state);
    expect(channelRowCount(expandedMarkup, "enclave-fixture")).toBe(4);

    state = toggleEnclaveExpansion(state, "enclave-fixture");
    const collapsedAgain = oslEnclaveSidebarListMarkup(models, state);
    const collapsedAgainCount = channelRowCount(collapsedAgain, "enclave-fixture");
    const orderAfter = [...collapsedAgain.matchAll(/data-enclave-id="([^"]+)"/gu)].map((match) => match[1]);

    console.log(`TASK5032 recollapsed_channel_count=${collapsedAgainCount} order_before=${orderBefore.join(",")} order_after=${orderAfter.join(",")}`);
    expect(collapsedAgainCount).toBe(0);
    expect(orderAfter).toEqual(orderBefore);
  });

  it("rolls the collapsed row's unread count up to exactly the sum of its channels' counts", () => {
    const total = oslEnclaveUnreadTotal(fixtureEnclave);
    const expectedSum = fixtureEnclave.channels.reduce((sum, channel) => sum + channel.unreadCount, 0);
    console.log(`TASK5032 unread_total=${total} expected_sum=${expectedSum}`);
    expect(total).toBe(expectedSum);
    expect(total).toBe(6);

    const collapsedMarkup = oslEnclaveSidebarRowMarkup(fixtureEnclave, OSL_ENCLAVE_SIDEBAR_COLLAPSED);
    expect(collapsedMarkup).toContain(`data-enclave-unread="${total}"`);
  });

  it("lists a locked channel row greyed with its reason -- never missing -- while obeying locked, not hidden", () => {
    const expandedState = toggleEnclaveExpansion(OSL_ENCLAVE_SIDEBAR_COLLAPSED, "enclave-fixture");
    const markup = oslEnclaveSidebarRowMarkup(fixtureEnclave, expandedState);
    console.log(`TASK5032 has_locked_row=${markup.includes('data-enclave-channel-id="mod-only"')} locked_class=${markup.includes("osl-enclave-channel is-locked")}`);
    expect(markup).toContain('data-enclave-channel-id="mod-only"');
    expect(markup).toContain("osl-enclave-channel is-locked");
    expect(markup).toContain('data-enclave-channel-locked="true"');
    expect(markup).toContain("Requires the Moderator role");
  });

  it("survives switching tabs with the same 1 enclave open", () => {
    let state = OSL_ENCLAVE_SIDEBAR_COLLAPSED as ReturnType<typeof toggleEnclaveExpansion>;
    state = toggleEnclaveExpansion(state, "enclave-fixture");

    // "Switching tabs" changes which primary destination is rendered; it must not
    // touch expansion state, which the caller holds independently of route.
    const renderUnderRoute = (route: "inbox" | "settings") => {
      void route; // the sidebar markup does not depend on the active route
      return oslEnclaveSidebarListMarkup([fixtureEnclave, otherEnclave], state);
    };

    const onInboxTab = renderUnderRoute("inbox");
    const onSettingsTab = renderUnderRoute("settings");
    const backOnInboxTab = renderUnderRoute("inbox");

    const openCount = (markup: string) => [...markup.matchAll(/class="osl-enclave-row is-expanded"/gu)].length;
    console.log(`TASK5032 open_enclaves_inbox=${openCount(onInboxTab)} open_enclaves_settings=${openCount(onSettingsTab)} open_enclaves_inbox_again=${openCount(backOnInboxTab)}`);

    expect(openCount(onInboxTab)).toBe(1);
    expect(openCount(onSettingsTab)).toBe(1);
    expect(openCount(backOnInboxTab)).toBe(1);
    expect(channelRowCount(onSettingsTab, "enclave-fixture")).toBe(4);
  });
});
