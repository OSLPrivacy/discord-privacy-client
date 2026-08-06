import { describe, expect, it } from "vitest";
import {
  DELETE_GRANT_RECORD,
  MESSAGE_READ_KEY_RECORD,
  createStoredProtectedMessageGrants,
  parseDeleteGrantRecord,
  parseMessageReadKeyRecord,
} from "../src/lib/delete-grant.js";

const owner = "identity:alice";
const scope = "discord:9000000000000401:direct_message:delete-scope";

const realDeleteGrant = JSON.stringify({
  record: DELETE_GRANT_RECORD,
  owner,
  scope,
});

const messageReadKey = JSON.stringify({
  record: MESSAGE_READ_KEY_RECORD,
  message: "message:0401",
  readKey: "a".repeat(32),
});

describe("TASK 0401 delete grant record", () => {
  it("parses a real delete grant and names exactly one owner and one scope", () => {
    const parsed = parseDeleteGrantRecord(realDeleteGrant);
    expect(parsed).toEqual({
      ok: true,
      grant: {
        record: DELETE_GRANT_RECORD,
        owner,
        scope,
      },
    });
    if (!parsed.ok) throw new Error("delete grant did not parse");

    const names = Object.keys(parsed.grant).filter((key) => key === "owner" || key === "scope");
    const ownerCount = names.filter((key) => key === "owner").length;
    const scopeCount = names.filter((key) => key === "scope").length;
    expect(ownerCount).toBe(1);
    expect(scopeCount).toBe(1);
    console.log(
      `TASK0401 real delete grant parsed owner=${parsed.grant.owner} owner_count=${ownerCount} scope=${parsed.grant.scope} scope_count=${scopeCount}`,
    );
  });

  it("refuses a message read key offered as a delete grant", () => {
    const refused = parseDeleteGrantRecord(messageReadKey);
    expect(refused).toEqual({ ok: false, code: "not_delete_grant" });
    if (refused.ok) throw new Error("message read key parsed as delete grant");
    console.log(`TASK0401 read key offered as delete grant refused=${refused.code}`);
  });

  it("refuses a delete grant offered as a message read key", () => {
    const refused = parseMessageReadKeyRecord(realDeleteGrant);
    expect(refused).toEqual({ ok: false, code: "not_message_read_key" });
    if (refused.ok) throw new Error("delete grant parsed as message read key");
    console.log(`TASK0401 delete grant offered as read key refused=${refused.code}`);
  });
});

describe("TASK 0402 sender delete-grant creation", () => {
  it("creates one read key and one different sender delete grant for a stored protected message", () => {
    const message = "message:0402";
    const sender = "identity:sender-0402";
    const messageScope = "discord:9000000000000402:direct_message:stored-protected-message";
    const created = createStoredProtectedMessageGrants({
      message,
      readKey: "b".repeat(32),
      sender,
      scope: messageScope,
    });

    expect(created.ok).toBe(true);
    if (!created.ok) throw new Error(`grant creation failed: ${created.code}`);

    const { grants } = created;
    expect(grants.message).toBe(message);
    expect(grants.readKeys).toHaveLength(1);
    expect(grants.senderDeleteGrants).toHaveLength(1);

    const readKey = grants.readKeys[0]!;
    const senderDeleteGrant = grants.senderDeleteGrants[0]!;
    expect(readKey.record).toBe(MESSAGE_READ_KEY_RECORD);
    expect(senderDeleteGrant.record).toBe(DELETE_GRANT_RECORD);
    expect(senderDeleteGrant.owner).toBe(sender);
    expect(senderDeleteGrant.scope).toBe(messageScope);
    expect(senderDeleteGrant).not.toEqual(readKey);

    const different = JSON.stringify(senderDeleteGrant) !== JSON.stringify(readKey);
    expect(different).toBe(true);
    console.log(
      `TASK0402 stored protected message=${grants.message} read_key_count=${grants.readKeys.length} sender_delete_grant_count=${grants.senderDeleteGrants.length} different=${different ? "yes" : "no"} read_record=${readKey.record} delete_record=${senderDeleteGrant.record} owner=${senderDeleteGrant.owner} scope=${senderDeleteGrant.scope}`,
    );
  });
});
