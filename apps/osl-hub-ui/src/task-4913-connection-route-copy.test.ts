import { describe, expect, it } from "vitest";

import { applyTorBootstrapStatus, initialTorOnboardingState, onboardingTorMarkup } from "./onboarding-tor";
import { applyTorSidecarEvent, initialTorBootStatus } from "./tor-boot-orchestrator";

const MULLVAD_COPY = "You can use both. Neither replaces the other.";
const TOR_SUCCESS_COPY = "Use Tor covers OSL's own traffic and nothing else. Discord, Telegram and the browser are separate processes with their own sockets.";

describe("TASK 4913 connection route copy", () => {
  it("keeps exactly Tor and Direct as routes and states the separate Mullvad and Tor scopes", () => {
    const choiceMarkup = onboardingTorMarkup(initialTorOnboardingState());
    const routeRadios = choiceMarkup.match(/<input\b[^>]*\btype="radio"[^>]*>/gu) ?? [];
    const routeNames = routeRadios.map((radio) => /\baria-label="([^"]+)"/u.exec(radio)?.[1]);

    expect(routeRadios).toHaveLength(2);
    expect(routeNames).toEqual(["Tor", "Direct"]);
    expect(routeRadios.join("\n")).not.toContain("Mullvad");
    expect(choiceMarkup).toContain(`<div class="tor-mullvad-status" aria-label="Mullvad status"><strong>Mullvad</strong><span>${MULLVAD_COPY}</span></div>`);

    const ready = applyTorSidecarEvent(initialTorBootStatus(), { event: "ready" });
    const successMarkup = onboardingTorMarkup(applyTorBootstrapStatus(initialTorOnboardingState(), ready));
    expect(successMarkup).toContain(TOR_SUCCESS_COPY);

    console.info(`TASK4913_ROUTE_RADIO_COUNT=${routeRadios.length}`);
    console.info(`TASK4913_ROUTE_RADIO_NAMES=${routeNames.join("|")}`);
    console.info(`TASK4913_MULLVAD_COPY=${MULLVAD_COPY}`);
    console.info(`TASK4913_TOR_SUCCESS_COPY=${TOR_SUCCESS_COPY}`);
  });
});
