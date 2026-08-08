import { describe, expect, it } from "vitest";
import {
  startDirectConversation,
  startEnclave,
  startGroupConversation,
  type EnclaveJoiningRule,
  type StartSomethingDependencies,
  type StartSomethingPerson,
} from "./start-something";

const CREATOR = "osl-5035-me";
const PEOPLE: readonly StartSomethingPerson[] = [
  { personId: "ava", oslUserId: "osl-5035-ava", name: "Ava" },
  { personId: "ben", oslUserId: "osl-5035-ben", name: "Ben" },
  { personId: "cy", oslUserId: "osl-5035-cy", name: "Cy" },
];

type CreatedDirect = { readonly id: string; readonly members: readonly string[]; readonly messages: string[] };
type CreatedGroup = { readonly id: string; readonly members: readonly string[] };
type CreatedEnclave = { readonly id: string; readonly members: readonly string[]; readonly joiningRule: EnclaveJoiningRule };

class PencilAcceptanceRun {
  readonly directs: CreatedDirect[] = [];
  readonly groups: CreatedGroup[] = [];
  readonly enclaves: CreatedEnclave[] = [];
  route: "osl-chat" | "home" = "osl-chat";

  readonly dependencies: StartSomethingDependencies = {
    acceptDirectTarget: async (target) => target === "OSLFR1.accepted_5035" ? PEOPLE[0]! : null,
    createDirectConversation: async (_creator, members) => {
      const id = `direct-${this.directs.length + 1}`;
      this.directs.push({ id, members: [...members], messages: [] });
      return { conversationId: id, memberIds: members };
    },
    createGroupConversation: async (_name, members) => {
      const id = `group-${this.groups.length + 1}`;
      this.groups.push({ id, members: [...members] });
      return { groupId: id, memberIds: members };
    },
    createEnclave: async (_name, members, joiningRule) => {
      const id = `enclave-${this.enclaves.length + 1}`;
      this.enclaves.push({ id, members: [...members], joiningRule });
      return { enclaveId: id, memberIds: members, joiningRule };
    },
  };

  async sendDirect(id: string, body: string): Promise<void> {
    const direct = this.directs.find((candidate) => candidate.id === id);
    if (!direct) throw new Error("Direct conversation is not open");
    direct.messages.push(body);
  }

  returnHome(): void {
    this.route = "home";
  }

  assertWorkingResults(): void {
    if (this.route === "home" && !this.directs.length && !this.groups.length && !this.enclaves.length) {
      throw new Error("START SOMETHING returned to Home without creating anything");
    }
    if (this.directs.length !== 1 || this.groups.length !== 1 || this.enclaves.length !== 1) {
      throw new Error("Each START SOMETHING path must create exactly one result");
    }
    if (this.directs[0]?.messages.length !== 1) throw new Error("Created direct conversation cannot send");
    if (this.groups[0]?.members.length !== 4) throw new Error("Created group does not have four members");
    if (this.enclaves[0]?.joiningRule !== "approval_required") throw new Error("Created Enclave did not save its joining rule");
  }
}

describe("TASK 5035 — START SOMETHING acceptance", () => {
  it("creates and reads back one working direct, four-member group, and rule-preserving Enclave", async () => {
    const run = new PencilAcceptanceRun();

    const direct = await startDirectConversation("OSLFR1.accepted_5035", CREATOR, run.dependencies);
    await run.sendDirect(direct.conversationId, "5035 direct delivery");
    const group = await startGroupConversation("5035 group", CREATOR, PEOPLE, ["ava", "ben", "cy"], run.dependencies);
    const enclave = await startEnclave("5035 enclave", CREATOR, PEOPLE, ["ava", "ben"], "approval_required", run.dependencies);

    run.assertWorkingResults();
    expect(run.directs).toEqual([{ id: direct.conversationId, members: [CREATOR, "osl-5035-ava"], messages: ["5035 direct delivery"] }]);
    expect(run.groups).toEqual([{ id: group.groupId, members: [CREATOR, "osl-5035-ava", "osl-5035-ben", "osl-5035-cy"] }]);
    expect(run.enclaves).toEqual([{ id: enclave.enclaveId, members: [CREATOR, "osl-5035-ava", "osl-5035-ben"], joiningRule: "approval_required" }]);
    console.info(`TASK5035 direct_results=${run.directs.length} direct_messages=${run.directs[0]?.messages.length} group_results=${run.groups.length} group_members=${run.groups[0]?.members.length} enclave_results=${run.enclaves.length} enclave_joining_rule=${run.enclaves[0]?.joiningRule}`);
  });

  it("goes red when the pencil path returns Home without creating anything", () => {
    const throwaway = new PencilAcceptanceRun();
    throwaway.returnHome();
    expect(() => throwaway.assertWorkingResults()).toThrow("START SOMETHING returned to Home without creating anything");
    console.info("TASK5035 home_without_creation=red");
  });
});
