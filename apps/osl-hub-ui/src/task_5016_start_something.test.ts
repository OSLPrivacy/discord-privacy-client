import { describe, expect, it, vi } from "vitest";
import {
  START_SOMETHING_CHOICES,
  startDirectConversation,
  startEnclave,
  startGroupConversation,
  startSomethingSheetMarkup,
  type StartSomethingDependencies,
  type StartSomethingPerson,
} from "./start-something";

const people: StartSomethingPerson[] = [
  { personId: "ava", oslUserId: "osl-ava", name: "Ava" },
  { personId: "ben", oslUserId: "osl-ben", name: "Ben" },
  { personId: "cy", oslUserId: "osl-cy", name: "Cy" },
];

function dependencies(): StartSomethingDependencies {
  return {
    acceptDirectTarget: vi.fn(async (target: string) => target === "OSLFR1.fixture_invite_5016" ? people[0]! : null),
    createDirectConversation: vi.fn(async (_creator: string, memberIds: readonly string[]) => ({ conversationId: "dm-5016", memberIds })),
    createGroupConversation: vi.fn(async (_name: string, memberIds: readonly string[]) => ({ groupId: "group-5016", memberIds })),
    createEnclave: vi.fn(async (_name: string, memberIds: readonly string[], joiningRule) => ({ enclaveId: "enclave-5016", memberIds, joiningRule })),
  };
}

describe("TASK 5016 — START SOMETHING", () => {
  it("renders the pencil sheet with exactly its three choices and a copyable own invite", () => {
    const markup = startSomethingSheetMarkup(null, "OSLFR1.own_invite_5016", people);
    const choices = markup.match(/data-start-something-choice=/gu) ?? [];
    expect(START_SOMETHING_CHOICES).toHaveLength(3);
    expect(choices).toHaveLength(3);
    expect(markup).toContain("START SOMETHING");
    const direct = startSomethingSheetMarkup("direct", "OSLFR1.own_invite_5016", people);
    expect(direct).toContain("data-start-something-copy-invite");
    expect(direct).toContain("OSLFR1.own_invite_5016");
    console.info("TASK5016 choices=3 own_invite_copyable=true");
  });

  it("accepts the fixture invite and creates exactly one direct conversation", async () => {
    const deps = dependencies();
    const created = await startDirectConversation("OSLFR1.fixture_invite_5016", "osl-me", deps);
    expect(created.memberIds).toEqual(["osl-me", "osl-ava"]);
    expect(deps.createDirectConversation).toHaveBeenCalledTimes(1);
    console.info(`TASK5016 direct_fixture_accepted=true new_conversations=${(deps.createDirectConversation as ReturnType<typeof vi.fn>).mock.calls.length}`);
  });

  it("turns three ticks into a group of exactly four members", async () => {
    const deps = dependencies();
    const created = await startGroupConversation("Fixture group", "osl-me", people, ["ava", "ben", "cy"], deps);
    expect(created.memberIds).toHaveLength(4);
    expect(created.memberIds).toEqual(["osl-me", "osl-ava", "osl-ben", "osl-cy"]);
    console.info(`TASK5016 group_ticked=3 member_count=${created.memberIds.length}`);
  });

  it("saves the selected Enclave joining rule exactly", async () => {
    const deps = dependencies();
    const created = await startEnclave("Fixture enclave", "osl-me", people, ["ava"], "approval_required", deps);
    expect(created.joiningRule).toBe("approval_required");
    expect(deps.createEnclave).toHaveBeenCalledWith("Fixture enclave", ["osl-me", "osl-ava"], "approval_required");
    console.info(`TASK5016 enclave_joining_rule=${created.joiningRule}`);
  });
});
