import { describe, expect, it } from "vitest";
import proof from "./task-5019-default-flip-proof.json";
import { SHIPPED_CONNECTION_DEFAULT, initialTorOnboardingState } from "./onboarding-tor";
import { applyTorSidecarEvent, initialTorBootStatus, parseTorSidecarLine, torRouteStatusLabel } from "./tor-boot-orchestrator";

const success = "Connected — Tor covers OSL's own traffic, not Discord or your browser.";

describe("TASK 5019 product-sentence acceptance", () => {
  it("uses one honest success sentence for OSL's traffic and the external-app boundary", () => {
    const ready = applyTorSidecarEvent(initialTorBootStatus(), { event: "ready" });
    const message = torRouteStatusLabel(ready);
    expect(message).toBe(success);
    expect(message.match(/[.!?](?=\s|$)/gu)).toHaveLength(1);
    expect(message).toContain("OSL's own traffic");
    expect(message).toContain("not Discord or your browser");
  });

  it("maps the packaged sidecar's real error object to the plain failure state", () => {
    const event = parseTorSidecarLine('{"event":"error","scope":"bootstrap","detail":"directory unavailable"}');
    expect(event).toEqual({ event: "error", message: "directory unavailable" });
    expect(torRouteStatusLabel(applyTorSidecarEvent(initialTorBootStatus(), event!))).toBe("Failed -- Tor could not connect");
  });

  it("refuses a default flip until a packaged-build 4900 Tor send is green", () => {
    const packagedGreen = proof.packagedBuild === true
      && proof.route === "tor"
      && proof.status === "green"
      && proof.messagesArrived === 1
      && proof.offTunnelBytes === 0;
    expect(SHIPPED_CONNECTION_DEFAULT === "direct" || packagedGreen).toBe(true);
    expect(initialTorOnboardingState().choice).toBe(SHIPPED_CONNECTION_DEFAULT);
    console.info(`TASK5019_DEFAULT=${SHIPPED_CONNECTION_DEFAULT} TASK4900_PACKAGED_GREEN=${packagedGreen}`);
  });
});
