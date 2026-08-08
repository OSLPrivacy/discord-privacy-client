import { describe, expect, it } from "vitest";

import {
  createEnclaveChannelList,
  openEnclaveChannel,
  type EnclaveChannel,
} from "./osl-enclave-channel-list";
import { oslEnclavesSurfaceMarkup } from "./osl-enclaves";

describe("TASK 5006 Enclave channel permission overrides", () => {
  it("keeps all five fixture channels rendered while denying two with their deciding rules", () => {
    const memberId = "fixture-member";
    const deniedAnnouncements = "Channel override: fixture-member cannot open announcements.";
    const deniedStaff = "Channel override: fixture-member cannot open staff-room.";
    const channels: readonly EnclaveChannel[] = [
      { channelId: "general", name: "general" },
      { channelId: "announcements", name: "announcements", permissionOverrides: [{ memberId, decision: "deny", reason: deniedAnnouncements }] },
      { channelId: "projects", name: "projects" },
      { channelId: "staff-room", name: "staff-room", permissionOverrides: [{ memberId, decision: "deny", reason: deniedStaff }] },
      { channelId: "off-topic", name: "off-topic" },
    ];

    const list = createEnclaveChannelList(memberId, channels);
    const markup = oslEnclavesSurfaceMarkup({
      channelList: list,
      statusTag: (label) => `<span>${label}</span>`,
    });
    const deniedRows = list.rows.filter((row) => row.denied);
    const renderedChannelIds = [...markup.matchAll(/data-enclave-channel-id="([^"]+)"/gu)].map((match) => match[1]);
    const greyedRows = [...markup.matchAll(/class="osl-enclave-channel-row is-denied"/gu)];
    const missingChannels = channels.filter((channel) => !renderedChannelIds.includes(channel.channelId));
    const announcementsOpen = openEnclaveChannel(list, "announcements");
    const staffOpen = openEnclaveChannel(list, "staff-room");

    expect(list.rows).toHaveLength(5);
    expect(renderedChannelIds).toHaveLength(5);
    expect(renderedChannelIds).toEqual(channels.map((channel) => channel.channelId));
    expect(missingChannels).toHaveLength(0);
    expect(deniedRows).toHaveLength(2);
    expect(greyedRows).toHaveLength(2);
    expect(deniedRows.map((row) => row.channelId)).toEqual(["announcements", "staff-room"]);
    expect(deniedRows.map((row) => row.reason)).toEqual([deniedAnnouncements, deniedStaff]);
    expect(markup).toContain(`title="${deniedAnnouncements}"`);
    expect(markup).toContain(`title="${deniedStaff}"`);
    expect(announcementsOpen).toEqual({ opened: false, channelId: "announcements", refusal: deniedAnnouncements });
    expect(staffOpen).toEqual({ opened: false, channelId: "staff-room", refusal: deniedStaff });
    console.log(
      `TASK5006 total_rows=${renderedChannelIds.length} greyed_rows=${greyedRows.length} missing_channels=${missingChannels.length}`,
    );
    console.log(`TASK5006 hover_reason announcements="${deniedAnnouncements}" staff-room="${deniedStaff}"`);
    console.log(
      `TASK5006 open_refused announcements=${!announcementsOpen.opened} reason="${announcementsOpen.refusal}" staff-room=${!staffOpen.opened} reason="${staffOpen.refusal}"`,
    );
  });
});
