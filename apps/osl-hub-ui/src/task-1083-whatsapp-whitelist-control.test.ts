import { describe, expect, it } from "vitest";
import {
  parseWhatsAppDirectMessageWhitelistState,
  whatsappDirectMessageControlMarkup,
  whatsappDirectMessageIsAllowed,
} from "./whatsapp-whitelist-control";

const twoWay = {
  app: "whatsapp",
  kind: "direct_message",
  firstAccount: "qa-owner",
  secondAccount: "qa-peer",
  firstToSecondStableId: "whatsapp:qa-owner:direct_message:qa-peer",
  secondToFirstStableId: "whatsapp:qa-peer:direct_message:qa-owner",
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  state: "two-way",
} as const;

describe("TASK1083 WhatsApp whitelist control", () => {
  it("gives an allowed two-way direct message one OSL control with one verification tick", () => {
    const state = parseWhatsAppDirectMessageWhitelistState(twoWay);
    const markup = whatsappDirectMessageControlMarkup(state);
    const tickCount = markup.split('data-osl-verification-tick="visible"').length - 1;
    const controlCount = markup.split('data-osl-whatsapp-control="protected-direct-message"').length - 1;

    console.log(`TASK1083 direct=allowed-two-way controls=${controlCount} ticks=${tickCount}`);
    expect(whatsappDirectMessageIsAllowed(state)).toBe(true);
    expect(controlCount).toBe(1);
    expect(tickCount).toBe(1);
  });

  it("renders no OSL control for an unallowed one-way direct message", () => {
    const state = parseWhatsAppDirectMessageWhitelistState({
      ...twoWay,
      secondToFirstAllowed: false,
      state: "one-way",
    });
    const markup = whatsappDirectMessageControlMarkup(state);

    console.log(`TASK1083 direct=unallowed-one-way controls=${markup.includes("data-osl-whatsapp-control")} ticks=${markup.includes("data-osl-verification-tick")}`);
    expect(whatsappDirectMessageIsAllowed(state)).toBe(false);
    expect(markup).toBe("");
    expect(markup).not.toContain("data-osl-whatsapp-control");
    expect(markup).not.toContain("data-osl-verification-tick");
  });

  it("fails closed if the backend's directional facts disagree with its state", () => {
    const malformed = { ...twoWay, secondToFirstAllowed: false };
    expect(parseWhatsAppDirectMessageWhitelistState(malformed)).toBeNull();
    expect(whatsappDirectMessageControlMarkup(parseWhatsAppDirectMessageWhitelistState(malformed))).toBe("");
  });
});
