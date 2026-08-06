import { describe, expect, it } from "vitest";
import {
  DELETE_GRANT_RECORD,
  MESSAGE_READ_KEY_RECORD,
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
