import { describe, expect, it } from "vitest";
import {
  enclaveServerScreen,
  requestEnclaveServerChannels,
  requestEnclaveServerMembers,
  requestEnclaveServerMessages,
  type EnclaveServerSnapshot,
} from "./osl-enclaves-server-access";

const namedMember = "Ada Namedmember";
const neverMember = "Nia Nevermember";
const markedMessage = "TASK1384_MARKED_SERVER_MESSAGE";

const server: EnclaveServerSnapshot = {
  serverId: "task-1384-server",
  messages: [
    {
      messageId: "msg-task-1384",
      channelId: "chan-general",
      senderId: "member-ada",
      plaintext: markedMessage,
      marked: true,
    },
  ],
  members: [
    { memberId: "member-ada", displayName: namedMember },
    { memberId: "member-ben", displayName: "Ben Member" },
    { memberId: "member-cy", displayName: "Cy Member" },
  ],
  channels: [
    { channelId: "chan-general", name: "general-marked" },
    { channelId: "chan-private", name: "private-marked" },
  ],
};

function names(rows: readonly { readonly displayName?: string; readonly name?: string }[]): string {
  return rows.map((row) => row.displayName ?? row.name).join(",");
}

describe("TASK 1384 Enclave server access", () => {
  it("refuses messages, members and channels to an account that was never a member", () => {
    const firstScreen = enclaveServerScreen(server, namedMember);
    expect(firstScreen.messageRows.map((row) => row.plaintext)).toEqual([markedMessage]);
    expect(firstScreen.memberRows.map((row) => row.displayName)).toEqual([
      namedMember,
      "Ben Member",
      "Cy Member",
    ]);
    expect(firstScreen.channelRows.map((row) => row.name)).toEqual([
      "general-marked",
      "private-marked",
    ]);
    console.log(
      `TASK1384 first_screen requester=${namedMember} messages=${firstScreen.messageRows.length} marked=${firstScreen.messageRows[0]?.plaintext} members=${firstScreen.memberRows.length} member_names=${names(firstScreen.memberRows)} channels=${firstScreen.channelRows.length} channel_names=${names(firstScreen.channelRows)}`,
    );

    const neverScreen = enclaveServerScreen(server, neverMember);
    expect(neverScreen.messageRows).toHaveLength(0);
    expect(neverScreen.memberRows).toHaveLength(0);
    expect(neverScreen.channelRows).toHaveLength(0);
    expect(neverScreen.refusals).toEqual([
      `${neverMember} was never a member of task-1384-server; message list refused`,
      `${neverMember} was never a member of task-1384-server; member list refused`,
      `${neverMember} was never a member of task-1384-server; channel list refused`,
    ]);
    expect(JSON.stringify(neverScreen)).not.toContain(namedMember);
    console.log(
      `TASK1384 never_screen requester=${neverMember} messages=${neverScreen.messageRows.length} members=${neverScreen.memberRows.length} channels=${neverScreen.channelRows.length} refusals=${neverScreen.refusals.join(" | ")} contains_named_member=${JSON.stringify(neverScreen).includes(namedMember)}`,
    );

    const directMessages = requestEnclaveServerMessages(server, neverMember);
    const directMembers = requestEnclaveServerMembers(server, neverMember);
    const directChannels = requestEnclaveServerChannels(server, neverMember);
    expect(directMessages).toMatchObject({
      requesterName: neverMember,
      refused: true,
      rows: [],
      refusal: `${neverMember} was never a member of task-1384-server; message list refused`,
    });
    expect(directMembers).toMatchObject({
      requesterName: neverMember,
      refused: true,
      rows: [],
      refusal: `${neverMember} was never a member of task-1384-server; member list refused`,
    });
    expect(directChannels).toMatchObject({
      requesterName: neverMember,
      refused: true,
      rows: [],
      refusal: `${neverMember} was never a member of task-1384-server; channel list refused`,
    });
    const directMemberNames = directMembers.rows.map((row) => row.displayName);
    expect(directMemberNames).not.toContain(namedMember);
    console.log(
      `TASK1384 direct_never_member requester=${neverMember} message_refusal="${directMessages.refusal}" member_refusal="${directMembers.refusal}" channel_refusal="${directChannels.refusal}" counts=${directMessages.rows.length}/${directMembers.rows.length}/${directChannels.rows.length} contains_named_member=${directMemberNames.includes(namedMember)}`,
    );

    const afterScreen = enclaveServerScreen(server, namedMember);
    expect(afterScreen.messageRows.map((row) => row.plaintext)).toEqual([markedMessage]);
    expect(afterScreen.memberRows.map((row) => row.displayName)).toEqual([
      namedMember,
      "Ben Member",
      "Cy Member",
    ]);
    expect(afterScreen.channelRows.map((row) => row.name)).toEqual([
      "general-marked",
      "private-marked",
    ]);
    console.log(
      `TASK1384 after_screen requester=${namedMember} messages=${afterScreen.messageRows.length} marked=${afterScreen.messageRows[0]?.plaintext} members=${afterScreen.memberRows.length} member_names=${names(afterScreen.memberRows)} channels=${afterScreen.channelRows.length} channel_names=${names(afterScreen.channelRows)}`,
    );
  });
});
