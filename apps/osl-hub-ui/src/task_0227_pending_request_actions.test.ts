import { describe, expect, it } from "vitest";

interface ReceivedRequestRow {
  requestId: string;
  senderAlias: string;
}

interface RequestAction {
  action: string;
  label: string;
}

function escapeAttribute(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

function receivedRequestRowMarkup(row: ReceivedRequestRow): string {
  const nickname = row.senderAlias || "Unnamed friend";
  return `<article class="received-request-row" data-received-request="${escapeAttribute(row.requestId)}">
    <div>
      <strong>${nickname}</strong>
      <small>Wants to be friends</small>
    </div>
    <div class="request-actions" role="group" aria-label="Actions for request from ${escapeAttribute(nickname)}">
      <button class="button compact" type="button" data-request-action="accept" data-request-id="${escapeAttribute(row.requestId)}" aria-label="Accept friend request from ${escapeAttribute(nickname)}">Accept</button>
      <button class="button compact" type="button" data-request-action="decline" data-request-id="${escapeAttribute(row.requestId)}" aria-label="Decline friend request from ${escapeAttribute(nickname)}">Decline</button>
    </div>
  </article>`;
}

function extractRequestActions(html: string): RequestAction[] {
  return [...html.matchAll(/<button[^>]*data-request-action="([^"]*)"\s[^>]*>([^<]*)<\/button>/gu)]
    .map((match) => ({
      action: match[1],
      label: match[2].trim(),
    }));
}

describe("TASK 0227 pending request actions fixture", () => {
  it("renders Accept and Decline actions for one incoming request", () => {
    const fixture = receivedRequestRowMarkup({
      requestId: "REQ-0227",
      senderAlias: "Pat",
    });

    const actions = extractRequestActions(fixture);

    console.log(`TASK0227_REQUEST_ID=REQ-0227`);
    for (const action of actions) {
      console.log(`TASK0227_ACTION action="${action.action}" label="${action.label}"`);
    }
    console.log(`TASK0227_DONE count=${actions.length}`);

    expect(fixture).toContain('data-received-request="REQ-0227"');
    expect(fixture).toContain("Pat");
    expect(actions).toHaveLength(2);
    expect(actions.map((a) => a.action)).toEqual(["accept", "decline"]);
    expect(actions.map((a) => a.label)).toEqual(["Accept", "Decline"]);
  });
});
