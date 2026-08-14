import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  GET_SERVICE_TERMS_ADDRESS_COMMAND,
  MESSAGING_RISK_BACK_STEP,
  MESSAGING_RISK_FACTS,
  MESSAGING_RISK_NEXT_STEP,
  MESSAGING_RISK_STEP,
  backFromMessagingRisk,
  canContinueFromMessagingRisk,
  continueFromMessagingRisk,
  initialMessagingRiskState,
  messagingRiskPageMarkup,
  readServiceTerms,
  toggleMessagingRiskAgreement,
  type MessagingRiskInvoke,
} from "./messaging-risk-page";

const SERVICE_ID = "discord";
const SERVICE_NAME = "Discord";

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe("TASK 3112 messaging risk page", () => {
  it("titles the page Messaging risk and lists every fact, Read service terms, tick box, Back and Continue", () => {
    const state = initialMessagingRiskState();
    const markup = messagingRiskPageMarkup(SERVICE_ID, SERVICE_NAME, state);

    expect(occurrences(markup, ">Messaging risk<")).toBe(1);
    expect(MESSAGING_RISK_FACTS).toHaveLength(5);
    for (const fact of MESSAGING_RISK_FACTS) {
      expect(markup).toContain(fact.replace(/'/g, "&#39;"));
    }
    expect(occurrences(markup, ">Read service terms<")).toBe(1);
    expect(occurrences(markup, 'class="sr-only mr-agree-tick"')).toBe(1);
    expect(occurrences(markup, ">Back<")).toBe(1);
    expect(occurrences(markup, ">Continue<")).toBe(1);
  });

  it("starts unticked with Continue unavailable", () => {
    const state = initialMessagingRiskState();
    const markup = messagingRiskPageMarkup(SERVICE_ID, SERVICE_NAME, state);

    expect(state.agreed).toBe(false);
    expect(canContinueFromMessagingRisk(state)).toBe(false);
    expect(markup).toContain(`data-messaging-continue="${MESSAGING_RISK_NEXT_STEP}" disabled aria-disabled="true"`);
    expect(continueFromMessagingRisk(state)).toEqual({
      outcome: "refused",
      step: MESSAGING_RISK_STEP,
      reason: "risk-not-agreed",
    });
  });

  it("ticking makes Continue available and opens the named next page", () => {
    const ticked = toggleMessagingRiskAgreement(initialMessagingRiskState());
    const markup = messagingRiskPageMarkup(SERVICE_ID, SERVICE_NAME, ticked);

    expect(ticked.agreed).toBe(true);
    expect(canContinueFromMessagingRisk(ticked)).toBe(true);
    expect(markup).not.toContain("aria-disabled");
    expect(markup).toContain(`data-messaging-continue="${MESSAGING_RISK_NEXT_STEP}"`);

    const result = continueFromMessagingRisk(ticked);
    expect(result).toEqual({ outcome: "advanced", step: MESSAGING_RISK_NEXT_STEP });

    // eslint-disable-next-line no-console
    console.log(
      `TASK3112_TICK agreed=${ticked.agreed} continue_available=${canContinueFromMessagingRisk(ticked)} next_step=${result.outcome === "advanced" ? result.step : ""}`,
    );
  });

  it("unticking makes Continue unavailable again", () => {
    const ticked = toggleMessagingRiskAgreement(initialMessagingRiskState());
    const unticked = toggleMessagingRiskAgreement(ticked);
    const markup = messagingRiskPageMarkup(SERVICE_ID, SERVICE_NAME, unticked);

    expect(unticked.agreed).toBe(false);
    expect(canContinueFromMessagingRisk(unticked)).toBe(false);
    expect(markup).toContain(`data-messaging-continue="${MESSAGING_RISK_NEXT_STEP}" disabled aria-disabled="true"`);
    expect(continueFromMessagingRisk(unticked)).toEqual({
      outcome: "refused",
      step: MESSAGING_RISK_STEP,
      reason: "risk-not-agreed",
    });

    // eslint-disable-next-line no-console
    console.log(
      `TASK3112_UNTICK agreed=${unticked.agreed} continue_available=${canContinueFromMessagingRisk(unticked)}`,
    );
  });

  it("ticking a second time opens the same named next page", () => {
    let state = initialMessagingRiskState();
    state = toggleMessagingRiskAgreement(state); // tick
    state = toggleMessagingRiskAgreement(state); // untick
    state = toggleMessagingRiskAgreement(state); // tick again

    expect(state.agreed).toBe(true);
    const result = continueFromMessagingRisk(state);
    expect(result).toEqual({ outcome: "advanced", step: MESSAGING_RISK_NEXT_STEP });

    // eslint-disable-next-line no-console
    console.log(
      `TASK3112_RETICK agreed=${state.agreed} next_step=${result.outcome === "advanced" ? result.step : ""}`,
    );
  });

  it("Back leaves the tick alone and names the previous step", () => {
    expect(backFromMessagingRisk()).toBe(MESSAGING_RISK_BACK_STEP);
  });

  it("Read service terms calls the real terms command for the service on screen", async () => {
    const calls: Array<{ command: string; serviceId: string }> = [];
    const invoke: MessagingRiskInvoke = async (command, payload) => {
      calls.push({ command, serviceId: payload.serviceId });
      return { serviceId: payload.serviceId, termsAddress: "https://discord.com/terms" };
    };

    const address = await readServiceTerms(SERVICE_ID, invoke);

    expect(calls).toEqual([{ command: GET_SERVICE_TERMS_ADDRESS_COMMAND, serviceId: SERVICE_ID }]);
    expect(address.termsAddress).toBe("https://discord.com/terms");
  });
});

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..", "..");

/**
 * The five facts and the terms command have to match the real Rust source,
 * not a copy that has drifted: TASK 3109's MESSAGING_RISK_FACTS and TASK
 * 1406's get_service_terms_address command.
 */
describe("TASK 3112 the page matches the real backend facts and terms command", () => {
  const services = readFileSync(path.join(REPO_ROOT, "apps/osl-hub/src/services.rs"), "utf8");
  const commandSurface = readFileSync(
    path.join(REPO_ROOT, "apps/osl-hub/src/hub_command_surface.rs"),
    "utf8",
  );

  it("lists exactly the five facts services.rs writes into every agreement", () => {
    const block = services.match(
      /pub const MESSAGING_RISK_FACTS: \[&str; MESSAGING_RISK_FACT_COUNT\] = \[([^\]]*)\];/,
    );
    expect(block, "MESSAGING_RISK_FACTS must exist in services.rs").not.toBeNull();
    const rustFacts = [...(block as RegExpMatchArray)[1].matchAll(/"([^"]*)"/g)].map((match) => match[1]);

    expect(rustFacts).toHaveLength(5);
    expect([...MESSAGING_RISK_FACTS]).toEqual(rustFacts);
  });

  it("names a terms command the desktop build registers", () => {
    expect(commandSurface).toContain(`pub struct ServiceTermsAddress`);
    expect(commandSurface).toMatch(
      new RegExp(`^\\s+${GET_SERVICE_TERMS_ADDRESS_COMMAND},$`, "m"),
    );
  });
});
