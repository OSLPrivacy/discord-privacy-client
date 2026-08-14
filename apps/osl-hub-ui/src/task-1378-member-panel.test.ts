import { describe, expect, it } from "vitest";

import { oslEnclavesSurfaceMarkup } from "./osl-enclaves";
import { SERVER_CHANNEL_SIDEBAR_FIXTURE, serverChannelSidebarMarkup } from "./server-channel-sidebar";

describe("TASK 1378 member panel", () => {
  it("prints the default fixture finish-line evidence", () => {
    const markup = oslEnclavesSurfaceMarkup({ statusTag: (label) => `<span>${label}</span>` });
    const selectedServer = SERVER_CHANNEL_SIDEBAR_FIXTURE.servers[0];
    const viewer = selectedServer.members.find((member) => member.id === SERVER_CHANNEL_SIDEBAR_FIXTURE.viewerMemberId);
    const evidence = [
      `member rows=${markup.match(/data-enclave-member-id=/gu)?.length ?? 0}`,
      `permission strings for all three=${selectedServer.members.map((member) => `${member.name}: ${member.serverActions.join(", ")}`).join(" | ")}`,
      `selected server=${SERVER_CHANNEL_SIDEBAR_FIXTURE.selectedServerId}`,
      `selected channel=${SERVER_CHANNEL_SIDEBAR_FIXTURE.selectedChannelId}`,
      `channel member labels=${markup.match(/Member of #general/gu)?.length ?? 0}`,
      `viewer permissions=${viewer?.serverActions.join(", ") ?? "missing"}`,
      `invite buttons=${markup.match(/data-enclave-invite(?:[\s>])/gu)?.length ?? 0}`,
      `remove buttons=${markup.match(/data-enclave-remove-member=/gu)?.length ?? 0}`,
      `disabled buttons=${markup.match(/\sdisabled(?:[\s=>])/gu)?.length ?? 0}`,
    ].join("; ");

    console.log(evidence);
    expect(evidence).toBe("member rows=3; permission strings for all three=Avery Chen: read, send, invite, make channels, remove messages, remove people, change server | Morgan Reyes: read, send, invite, remove messages | Liam: read, send; selected server=osl-community; selected channel=general; channel member labels=3; viewer permissions=read, send; invite buttons=0; remove buttons=0; disabled buttons=0");
    expect(selectedServer.members).toHaveLength(3);
    for (const member of selectedServer.members) {
      expect(markup).toContain(`data-enclave-member-id="${member.id}"`);
      expect(markup).toContain(member.serverActions.join(" · "));
      expect(markup).toContain(`aria-label="Allowed server actions for ${member.name}"`);
    }
    expect(markup).toContain("Member of #general");
  });

  it("renders gated owner controls only for an authorized viewer", () => {
    const markup = serverChannelSidebarMarkup({ ...SERVER_CHANNEL_SIDEBAR_FIXTURE, viewerMemberId: "avery" });
    const inviteButtons = markup.match(/data-enclave-invite(?:[\s>])/gu)?.length ?? 0;
    const removeButtons = markup.match(/data-enclave-remove-member=/gu)?.length ?? 0;

    console.log(`authorized invite buttons=${inviteButtons}; authorized remove buttons=${removeButtons}`);
    expect(inviteButtons).toBe(1);
    expect(removeButtons).toBe(2);
    expect(markup).not.toContain("disabled");
  });

  it("keeps each server's roster scoped to that server", () => {
    const markup = serverChannelSidebarMarkup({
      ...SERVER_CHANNEL_SIDEBAR_FIXTURE,
      selectedServerId: "design-circle",
    });

    expect(markup.match(/data-enclave-member-id=/gu) ?? []).toHaveLength(0);
    expect(markup).not.toContain("Avery Chen");
    expect(markup).not.toContain("Morgan Reyes");
    expect(markup).not.toContain("Liam");
  });

  it("escapes names and safely falls back from an invalid selection", () => {
    const markup = serverChannelSidebarMarkup({
      ...SERVER_CHANNEL_SIDEBAR_FIXTURE,
      selectedServerId: "missing-server",
      selectedChannelId: "missing-channel",
      servers: [{
        ...SERVER_CHANNEL_SIDEBAR_FIXTURE.servers[0],
        name: '<img src=x onerror="bad">',
        channels: [{ id: "safe", name: '<script>alert(1)</script>', memberIds: ["liam"] }],
      }],
    });

    expect(markup).toContain("&lt;img src=x onerror=&quot;bad&quot;&gt;");
    expect(markup).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(markup).not.toContain("<script>");
    expect(markup).toContain('data-selected-channel-id="safe"');
  });
});
