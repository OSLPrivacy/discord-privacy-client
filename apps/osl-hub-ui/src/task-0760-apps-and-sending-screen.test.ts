import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  activeSendStyle,
  appsAndSendingScreenMarkup,
  connectedApps,
  nextGenerationState,
  nextGenerationStateLabel,
  selectedAccount,
  type AppsAndSendingModel,
} from "./apps-and-sending-screen";

const FIXTURE = path.join(
  import.meta.dirname,
  "..",
  "screenshots",
  "fixtures",
  "task-0760-apps-and-sending.json",
);

function savedModel(): AppsAndSendingModel {
  return JSON.parse(readFileSync(FIXTURE, "utf8")) as AppsAndSendingModel;
}

/** The one `<input …>` tag carrying the given id, attributes and all. */
function inputTag(markup: string, id: string): string {
  const start = markup.indexOf("<input", 0);
  for (let at = start; at !== -1; at = markup.indexOf("<input", at + 1)) {
    const tag = markup.slice(at, markup.indexOf(">", at) + 1);
    if (tag.includes(`id="${id}"`)) return tag;
  }
  throw new Error(`no input with id ${id}`);
}

/** Just the one app's card, so an assertion cannot be answered by a neighbour. */
function appCard(markup: string, appId: string): string {
  const start = markup.indexOf(`<li class="apps-sending-app" data-app-id="${appId}"`);
  if (start === -1) throw new Error(`no card for ${appId}`);
  const end = markup.indexOf("</li>", start);
  return markup.slice(start, end);
}

describe("TASK 0760 Apps and sending screen", () => {
  it("shows one connected app with its account, actions, sending style and switch", () => {
    const model = savedModel();
    const markup = appsAndSendingScreenMarkup(model);
    const connected = connectedApps(model);

    expect(connected).toHaveLength(1);
    expect(connected[0].name).toBe("Discord");
    expect(markup).toContain('data-connected-count="1"');
    expect(markup).toContain("Apps and sending");
    expect(markup).toContain("Connected apps");
    expect(markup).toContain("Send messages");
    expect(markup).toContain("Open Discord");
    expect(markup).toContain(">Set up<");
    expect(markup).toContain("Remove app");
    expect(markup).toContain("Next-generation messages");
    expect(markup).toContain(">Save<");
  });

  it("names the one chosen account and the one active sending style", () => {
    const model = savedModel();
    const discord = connectedApps(model)[0];

    expect(selectedAccount(discord)?.label).toBe("work@example.test");
    expect(activeSendStyle(discord)?.name).toBe("Clipboard");
    expect(appsAndSendingScreenMarkup(model)).toContain('data-active-send-style="clipboard"');
  });

  it("refuses to call anything active when the model marks two styles at once", () => {
    const model = savedModel();
    const discord = model.apps[0];
    discord.sendStyles[0].active = true;
    discord.sendStyles[1].active = true;

    expect(activeSendStyle(discord)).toBeNull();
    expect(appsAndSendingScreenMarkup(model)).toContain('data-active-send-style="none"');
    expect(appsAndSendingScreenMarkup(model)).not.toContain('data-send-style-active="true"');
  });

  it("gives a not-connected app no sending choices even when the model marks one active", () => {
    const model = savedModel();
    const telegram = model.apps[1];
    telegram.sendStyles = [
      { id: "single", name: "Single Enter", detail: "one press", tag: "", risk: "", active: true },
    ];

    expect(activeSendStyle(telegram)).toBeNull();
    const card = appCard(appsAndSendingScreenMarkup(model), "telegram");
    expect(card).not.toContain("data-send-style=");
    expect(card).not.toContain("data-next-generation=");
    expect(card).toContain("Set this app up to choose how OSL sends into it.");
    // Removing an app that was never added is not an action the screen offers.
    expect(card).not.toContain('id="apps-sending-remove-telegram"');
    expect(card).toContain('id="apps-sending-set-up-telegram"');
  });

  it("only reports the next-generation switch on when the build and the setting agree", () => {
    expect(nextGenerationState({ requested: true, buildEnabled: true, detail: "" })).toBe("on");
    expect(nextGenerationState({ requested: false, buildEnabled: true, detail: "" })).toBe("off");
    expect(nextGenerationState({ requested: true, buildEnabled: false, detail: "" })).toBe("unavailable");
    expect(nextGenerationStateLabel("on")).toBe("On");
    expect(nextGenerationStateLabel("unavailable")).toBe("Unavailable");
  });

  it("draws the saved-on switch as checked and a build-blocked one as disabled", () => {
    const on = appsAndSendingScreenMarkup(savedModel());
    expect(on).toContain('data-next-generation="on"');
    expect(on).toContain('data-next-generation-state="on"');
    expect(inputTag(on, "apps-sending-next-generation-discord")).toContain("checked");
    expect(inputTag(on, "apps-sending-next-generation-discord")).not.toContain("disabled");

    const blocked = savedModel();
    blocked.apps[0].nextGeneration.buildEnabled = false;
    const markup = appsAndSendingScreenMarkup(blocked);
    expect(markup).toContain('data-next-generation="unavailable"');
    expect(markup).not.toContain('data-next-generation-state="on"');
    expect(inputTag(markup, "apps-sending-next-generation-discord")).toContain("disabled");
    expect(inputTag(markup, "apps-sending-next-generation-discord")).not.toContain("checked");
  });

  it("counts a disconnected app out of the connected group", () => {
    const model = savedModel();
    model.apps[0].state = "notConnected";

    expect(connectedApps(model)).toHaveLength(0);
    expect(appsAndSendingScreenMarkup(model)).toContain('data-connected-count="0"');
  });

  it("escapes model text instead of injecting it as markup", () => {
    const model = savedModel();
    model.apps[0].accounts[0].label = '<img src=x onerror="boom">';
    const markup = appsAndSendingScreenMarkup(model);

    expect(markup).not.toContain("<img src=x");
    expect(markup).toContain("&lt;img src=x onerror=&quot;boom&quot;&gt;");
  });
});
