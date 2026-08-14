import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  AGREE_MESSAGING_RISK_COMMAND,
  READ_MESSAGING_RISK_AGREEMENT_COMMAND,
  createMessagingRiskGate,
  messagingRiskBackend,
  type MessagingRiskAgreementRead,
  type MessagingRiskGateInvoke,
  type ServiceActionRequest,
} from "./messaging-risk-gate";
import { MESSAGING_RISK_FACTS } from "./messaging-risk-page";

const DISCORD = "discord";
const DISCORD_NAME = "Discord";
const FIRST_ACCOUNT = "acct-discord-1";
const SECOND_ACCOUNT = "acct-discord-2";

/**
 * Stands in for the hub's stored agreements (TASK 3111): keyed by service *and*
 * service account, written only by `agree_messaging_service_risk`, and it
 * outlives any one gate the way the encrypted file outlives a window.
 */
class StoredAgreements {
  readonly calls: Array<{ command: string; serviceId: string; accountId: string }> = [];
  private readonly agreed = new Map<string, number>();

  readonly invoke: MessagingRiskGateInvoke = async (command, payload) => {
    this.calls.push({ command, serviceId: payload.serviceId, accountId: payload.accountId });
    const key = `${payload.serviceId} ${payload.accountId}`;
    if (command === AGREE_MESSAGING_RISK_COMMAND) {
      this.agreed.set(key, 1_786_019_246);
      return undefined;
    }
    const agreedAt = this.agreed.get(key) ?? null;
    const read: MessagingRiskAgreementRead = {
      serviceId: payload.serviceId,
      accountId: payload.accountId,
      agreed: agreedAt !== null,
      agreedAt,
      wording: agreedAt === null ? [] : [...MESSAGING_RISK_FACTS],
    };
    return read;
  };

  commandCalls(command: string): number {
    return this.calls.filter((call) => call.command === command).length;
  }
}

function sendIn(serviceId: string, serviceName: string, accountId: string): ServiceActionRequest {
  return { serviceId, serviceName, accountId, action: "send" };
}

function gateOver(store: StoredAgreements, done: ServiceActionRequest[]) {
  return createMessagingRiskGate({
    backend: messagingRiskBackend(store.invoke),
    act: async (request) => {
      done.push(request);
    },
  });
}

describe("TASK 3113 the risk page in front of the first action in a service", () => {
  it("opens 1 time for the first action, 0 times for the second, and 1 time for a different account", async () => {
    const store = new StoredAgreements();
    const done: ServiceActionRequest[] = [];
    const gate = gateOver(store, done);

    // 1. The first thing OSL is asked to do for this account.
    const first = await gate.requestServiceAction(sendIn(DISCORD, DISCORD_NAME, FIRST_ACCOUNT));
    expect(first).toEqual({
      outcome: "risk-page-opened",
      step: "risk",
      serviceId: DISCORD,
      accountId: FIRST_ACCOUNT,
      action: "send",
    });
    expect(done).toHaveLength(0);
    const markup = gate.openRiskPageMarkup() ?? "";
    expect(markup).toContain(">Messaging risk<");
    for (const fact of MESSAGING_RISK_FACTS) {
      expect(markup).toContain(fact.replace(/'/g, "&#39;"));
    }
    const firstActionOpens = gate.riskPageOpenCount(DISCORD, FIRST_ACCOUNT);

    // Ticking and continuing saves the agreement and then does what was asked.
    gate.toggleRiskAgreement();
    const continued = await gate.continueFromRiskPage();
    expect(continued).toEqual({
      outcome: "acted",
      serviceId: DISCORD,
      accountId: FIRST_ACCOUNT,
      action: "send",
    });
    expect(done).toHaveLength(1);
    expect(store.commandCalls(AGREE_MESSAGING_RISK_COMMAND)).toBe(1);
    expect(gate.openRiskPageMarkup()).toBeNull();

    // 2. The second action for the same account, from a gate built after a
    //    restart, so only the stored agreement can answer it.
    const afterRestart = gateOver(store, done);
    const second = await afterRestart.requestServiceAction(
      sendIn(DISCORD, DISCORD_NAME, FIRST_ACCOUNT),
    );
    expect(second).toEqual({
      outcome: "acted",
      serviceId: DISCORD,
      accountId: FIRST_ACCOUNT,
      action: "send",
    });
    expect(afterRestart.openRiskPageMarkup()).toBeNull();
    expect(done).toHaveLength(2);
    const secondActionOpens = afterRestart.riskPageOpenCount(DISCORD, FIRST_ACCOUNT);

    // 3. A different account in the same service is asked for itself.
    const other = await afterRestart.requestServiceAction(
      sendIn(DISCORD, DISCORD_NAME, SECOND_ACCOUNT),
    );
    expect(other).toEqual({
      outcome: "risk-page-opened",
      step: "risk",
      serviceId: DISCORD,
      accountId: SECOND_ACCOUNT,
      action: "send",
    });
    expect(done).toHaveLength(2);
    const otherAccountOpens = afterRestart.riskPageOpenCount(DISCORD, SECOND_ACCOUNT);

    // eslint-disable-next-line no-console
    console.log(
      `TASK3113_FIRST_ACTION_OPENS=${firstActionOpens}\n`
        + `TASK3113_SECOND_ACTION_OPENS=${secondActionOpens}\n`
        + `TASK3113_OTHER_ACCOUNT_OPENS=${otherAccountOpens}\n`
        + `TASK3113_TOTAL_OPENS=${afterRestart.totalRiskPageOpens() + firstActionOpens}\n`
        + `TASK3113_READ_COMMAND=${READ_MESSAGING_RISK_AGREEMENT_COMMAND}\n`
        + `TASK3113_AGREE_COMMAND=${AGREE_MESSAGING_RISK_COMMAND}\n`
        + `TASK3113_AGREE_CALLS=${store.commandCalls(AGREE_MESSAGING_RISK_COMMAND)}`,
    );

    expect(firstActionOpens).toBe(1);
    expect(secondActionOpens).toBe(0);
    expect(otherAccountOpens).toBe(1);
  });

  it("asks the stored agreement for the exact service and account the action is for", async () => {
    const store = new StoredAgreements();
    const done: ServiceActionRequest[] = [];
    const gate = gateOver(store, done);

    await gate.requestServiceAction(sendIn(DISCORD, DISCORD_NAME, FIRST_ACCOUNT));
    gate.toggleRiskAgreement();
    await gate.continueFromRiskPage();

    expect(store.calls).toEqual([
      { command: READ_MESSAGING_RISK_AGREEMENT_COMMAND, serviceId: DISCORD, accountId: FIRST_ACCOUNT },
      { command: AGREE_MESSAGING_RISK_COMMAND, serviceId: DISCORD, accountId: FIRST_ACCOUNT },
    ]);
  });

  it("agreeing in one service does not answer for another service", async () => {
    const store = new StoredAgreements();
    const done: ServiceActionRequest[] = [];
    const gate = gateOver(store, done);

    await gate.requestServiceAction(sendIn(DISCORD, DISCORD_NAME, FIRST_ACCOUNT));
    gate.toggleRiskAgreement();
    await gate.continueFromRiskPage();

    const telegram = await gate.requestServiceAction(sendIn("telegram", "Telegram", FIRST_ACCOUNT));
    expect(telegram.outcome).toBe("risk-page-opened");
    expect(gate.riskPageOpenCount("telegram", FIRST_ACCOUNT)).toBe(1);
    expect(done).toHaveLength(1);
  });

  it("holds every kind of action, not only sending, and does it after the tick", async () => {
    const store = new StoredAgreements();
    const done: ServiceActionRequest[] = [];
    const gate = gateOver(store, done);

    const connect = await gate.requestServiceAction({
      serviceId: DISCORD,
      serviceName: DISCORD_NAME,
      accountId: FIRST_ACCOUNT,
      action: "connect",
    });
    expect(connect.outcome).toBe("risk-page-opened");
    expect(gate.heldAction()?.action).toBe("connect");
    expect(done).toHaveLength(0);

    gate.toggleRiskAgreement();
    await gate.continueFromRiskPage();
    expect(done.map((request) => request.action)).toEqual(["connect"]);
  });

  it("refuses to act and saves nothing while the box is unticked", async () => {
    const store = new StoredAgreements();
    const done: ServiceActionRequest[] = [];
    const gate = gateOver(store, done);

    await gate.requestServiceAction(sendIn(DISCORD, DISCORD_NAME, FIRST_ACCOUNT));
    const refused = await gate.continueFromRiskPage();

    expect(refused).toEqual({ outcome: "refused", step: "risk", reason: "risk-not-agreed" });
    expect(done).toHaveLength(0);
    expect(store.commandCalls(AGREE_MESSAGING_RISK_COMMAND)).toBe(0);
    expect(gate.openRiskPageMarkup()).not.toBeNull();
  });

  it("Back closes the page, agrees to nothing, and leaves the next action still gated", async () => {
    const store = new StoredAgreements();
    const done: ServiceActionRequest[] = [];
    const gate = gateOver(store, done);

    await gate.requestServiceAction(sendIn(DISCORD, DISCORD_NAME, FIRST_ACCOUNT));
    expect(gate.backFromRiskPage()).toBe("account");
    expect(gate.openRiskPageMarkup()).toBeNull();
    expect(store.commandCalls(AGREE_MESSAGING_RISK_COMMAND)).toBe(0);

    const again = await gate.requestServiceAction(sendIn(DISCORD, DISCORD_NAME, FIRST_ACCOUNT));
    expect(again.outcome).toBe("risk-page-opened");
    expect(gate.riskPageOpenCount(DISCORD, FIRST_ACCOUNT)).toBe(2);
    expect(done).toHaveLength(0);
  });

  it("does not count a second ask while the same page is still open", async () => {
    const store = new StoredAgreements();
    const done: ServiceActionRequest[] = [];
    const gate = gateOver(store, done);

    await gate.requestServiceAction(sendIn(DISCORD, DISCORD_NAME, FIRST_ACCOUNT));
    const again = await gate.requestServiceAction(sendIn(DISCORD, DISCORD_NAME, FIRST_ACCOUNT));

    expect(again.outcome).toBe("risk-page-already-open");
    expect(gate.riskPageOpenCount(DISCORD, FIRST_ACCOUNT)).toBe(1);
  });
});

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..", "..");

/**
 * The gate is only connected if the two commands it names are commands the
 * desktop build actually registers *and* grants the webview: a registered
 * command with no `permissions/hub.toml` entry and no `capabilities/hub.json`
 * grant is rejected by Tauri's ACL before it ever runs.
 */
describe("TASK 3113 both gate commands are registered and granted to the window", () => {
  const commandSurface = readFileSync(
    path.join(REPO_ROOT, "apps/osl-hub/src/hub_command_surface.rs"),
    "utf8",
  );
  const permissions = readFileSync(path.join(REPO_ROOT, "apps/osl-hub/permissions/hub.toml"), "utf8");
  const capabilities = JSON.parse(
    readFileSync(path.join(REPO_ROOT, "apps/osl-hub/capabilities/hub.json"), "utf8"),
  ) as { permissions: string[] };
  const main = readFileSync(path.join(REPO_ROOT, "apps/osl-hub/src/main.rs"), "utf8");

  function permissionIdentifier(command: string): string {
    return `allow-${command.replace(/_/g, "-")}`;
  }

  for (const command of [READ_MESSAGING_RISK_AGREEMENT_COMMAND, AGREE_MESSAGING_RISK_COMMAND]) {
    it(`registers ${command} in the hub command list`, () => {
      expect(commandSurface).toMatch(new RegExp(`^\\s+${command},$`, "m"));
    });

    it(`declares ${command} in permissions/hub.toml`, () => {
      expect(permissions).toContain(`identifier = "${permissionIdentifier(command)}"`);
      expect(permissions).toContain(`commands.allow = ["${command}"]`);
    });

    it(`grants ${permissionIdentifier(command)} to the hub window in capabilities/hub.json`, () => {
      expect(capabilities.permissions).toContain(permissionIdentifier(command));
    });

    it(`implements ${command} as a tauri command`, () => {
      expect(main).toMatch(new RegExp(`async fn ${command}\\(`));
    });
  }

  it("reads the agreement through the same per-account store TASK 3111 writes", () => {
    const start = main.indexOf(`async fn ${READ_MESSAGING_RISK_AGREEMENT_COMMAND}(`);
    expect(start).toBeGreaterThan(-1);
    const body = main.slice(start, start + 2_000);
    expect(body).toMatch(
      /services::read_messaging_risk_agreement\(\s*&owner,\s*&service_id,\s*&account_id,?\s*\)/,
    );
    expect(body).toContain("registry.require_owned(&owner, service_kind, &account_id)?;");
  });
});
