/**
 * Task 0213: the add-friend name request box.
 *
 * The fixture backend below emulates the task 0211/0212 command contract
 * exactly: `osl_create_friend_request_by_osl_name` resolves for the one known
 * name and rejects every other name with "OSL: unknown OSL name". The
 * assertions check the rendered box, not internal state alone:
 * - 1 known-name submission -> exactly 1 pending entry naming that name,
 *   pending count 0 -> 1;
 * - an unknown-name submission -> 0 pending entries added, refusal text that
 *   names the refused name.
 */

import { describe, expect, it } from "vitest";
import {
  addFriendNameRequestMarkup,
  blankAddFriendNameRequestModel,
  submitAddFriendNameRequest,
  type AddFriendNameRequestBackend,
} from "./add-friend-name-request";

const KNOWN_NAME = "maple_0213";
const UNKNOWN_NAME = "violet_0213_missing";

interface RecordedInvoke {
  readonly command: string;
  readonly recipientName: string;
  readonly requestId: string;
}

function fixtureBackend(): {
  backend: AddFriendNameRequestBackend;
  invoked: RecordedInvoke[];
} {
  const invoked: RecordedInvoke[] = [];
  let counter = 0;
  const backend: AddFriendNameRequestBackend = {
    newRequestId: () => {
      counter += 1;
      return `task-0213-request-${counter}`;
    },
    invoke: (command, args) => {
      invoked.push({ command, recipientName: args.recipientName, requestId: args.requestId });
      if (args.recipientName !== KNOWN_NAME) {
        return Promise.reject(new Error("OSL: unknown OSL name"));
      }
      return Promise.resolve({
        request_id: args.requestId,
        recipient_name: args.recipientName,
        peer_osl_user_id: "task_0213_peer",
      });
    },
  };
  return { backend, invoked };
}

function pendingEntries(markup: string): string[] {
  return [...markup.matchAll(/data-pending-name-request="([^"]*)"/gu)].map((match) => match[1]);
}

function pendingCount(markup: string): number {
  const match = markup.match(/data-pending-count="(\d+)"/u);
  if (!match) throw new Error("pending count missing from markup");
  return Number(match[1]);
}

describe("task 0213: add-friend name request box", () => {
  it("draws the name box and the Send Request action with zero pending requests", () => {
    const markup = addFriendNameRequestMarkup(blankAddFriendNameRequestModel());
    expect(markup).toContain('<input id="add-friend-name-input" data-add-friend-name');
    expect(markup).toMatch(/<button[^>]*data-send-name-request[^>]*>Send Request<\/button>/u);
    expect(pendingCount(markup)).toBe(0);
    expect(pendingEntries(markup)).toEqual([]);
  });

  it("submitting 1 known name displays exactly 1 pending entry naming that name, count 0 -> 1", async () => {
    const { backend, invoked } = fixtureBackend();
    const before = blankAddFriendNameRequestModel();
    const beforeMarkup = addFriendNameRequestMarkup(before);
    expect(pendingCount(beforeMarkup)).toBe(0);

    const after = await submitAddFriendNameRequest(before, `  ${KNOWN_NAME}  `, backend);
    const afterMarkup = addFriendNameRequestMarkup(after);

    expect(pendingEntries(afterMarkup)).toEqual([KNOWN_NAME]);
    expect(pendingCount(afterMarkup)).toBe(1);
    expect(afterMarkup).toContain(`Request to ${KNOWN_NAME} is pending until they accept.`);
    expect(after.status).toEqual({ kind: "sent", recipientName: KNOWN_NAME });
    expect(invoked).toEqual([
      {
        command: "osl_create_friend_request_by_osl_name",
        recipientName: KNOWN_NAME,
        requestId: "task-0213-request-1",
      },
    ]);
    console.log(
      `TASK_0213_KNOWN_NAME name=${KNOWN_NAME} pending_before=${pendingCount(beforeMarkup)} pending_after=${pendingCount(afterMarkup)} entries=${pendingEntries(afterMarkup).join(",")}`,
    );
  });

  it("submitting an unknown name adds 0 pending entries and is refused by name", async () => {
    const { backend, invoked } = fixtureBackend();
    const before = blankAddFriendNameRequestModel();

    const after = await submitAddFriendNameRequest(before, UNKNOWN_NAME, backend);
    const markup = addFriendNameRequestMarkup(after);

    expect(pendingEntries(markup)).toEqual([]);
    expect(pendingCount(markup)).toBe(0);
    expect(after.status).toEqual({
      kind: "refused",
      recipientName: UNKNOWN_NAME,
      detail: "OSL: unknown OSL name",
    });
    expect(markup).toContain(`${UNKNOWN_NAME} is not a known OSL name. No request was created.`);
    expect(invoked).toHaveLength(1);
    console.log(
      `TASK_0213_UNKNOWN_NAME name=${UNKNOWN_NAME} pending_after=${pendingCount(markup)} refusal_names_name=${markup.includes(`${UNKNOWN_NAME} is not a known OSL name`)}`,
    );
  });

  it("an unknown name after a known one leaves the single pending entry untouched", async () => {
    const { backend } = fixtureBackend();
    let model = blankAddFriendNameRequestModel();
    model = await submitAddFriendNameRequest(model, KNOWN_NAME, backend);
    model = await submitAddFriendNameRequest(model, UNKNOWN_NAME, backend);
    const markup = addFriendNameRequestMarkup(model);
    expect(pendingEntries(markup)).toEqual([KNOWN_NAME]);
    expect(pendingCount(markup)).toBe(1);
    expect(markup).toContain(`${UNKNOWN_NAME} is not a known OSL name. No request was created.`);
  });
});
