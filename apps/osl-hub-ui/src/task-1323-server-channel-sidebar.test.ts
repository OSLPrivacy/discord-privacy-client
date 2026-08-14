import { describe, expect, it } from "vitest";

import { SERVER_CHANNEL_SIDEBAR_FIXTURE, serverChannelSidebarMarkup } from "./server-channel-sidebar";

describe("TASK 1323 server and channel sidebar", () => {
  it("shows two servers and three channels beneath the selected server in the fixture screen", () => {
    const markup = serverChannelSidebarMarkup();

    expect(SERVER_CHANNEL_SIDEBAR_FIXTURE.servers).toHaveLength(2);
    expect(markup.match(/data-enclave-server-id=/gu)).toHaveLength(2);
    expect(markup).toContain('data-enclave-server-id="osl-community"');
    expect(markup).toContain('aria-pressed="true"');
    expect(markup.match(/data-enclave-channel-id=/gu)).toHaveLength(3);
    expect(markup).toContain('aria-label="Channels in OSL Community"');
    expect(markup).toContain('data-fixture-screen="server-channel-sidebar"');
  });
});
