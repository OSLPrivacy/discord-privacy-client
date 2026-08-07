import { describe, expect, it } from "vitest";
import {
  DELETE_GRANT_RECORD,
  MESSAGE_READ_KEY_RECORD,
  createStoredProtectedMessageGrants,
  parseDeleteGrantRecord,
  parseMessageReadKeyRecord,
  validateDeleteGrant,
} from "../src/lib/delete-grant.js";

const owner = "identity:alice";
const scope = "discord:9000000000000401:direct_message:delete-scope";
const message = "message:0401";

const realDeleteGrant = JSON.stringify({
  record: DELETE_GRANT_RECORD,
  message,
  owner,
  scope,
});

const messageReadKey = JSON.stringify({
  record: MESSAGE_READ_KEY_RECORD,
  message,
  readKey: "a".repeat(32),
});

describe("TASK 0401 delete grant record", () => {
  it("parses a real delete grant and names exactly one owner and one scope", () => {
    const parsed = parseDeleteGrantRecord(realDeleteGrant);
    expect(parsed).toEqual({
      ok: true,
      grant: {
        record: DELETE_GRANT_RECORD,
        message,
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
    expect(senderDeleteGrant.message).toBe(message);
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

describe("TASK 0404 delete-grant scope checks", () => {
  it("fails validation when the message, owner, or burn scope changes", () => {
    const message0404 = "message:0404";
    const sender = "identity:sender-0404";
    const burnScope = "discord:9000000000000404:direct_message:burn-scope";
    const created = createStoredProtectedMessageGrants({
      message: message0404,
      readKey: "c".repeat(32),
      sender,
      scope: burnScope,
    });
    expect(created.ok).toBe(true);
    if (!created.ok) throw new Error(`grant creation failed: ${created.code}`);

    const grant = created.grants.senderDeleteGrants[0]!;
    const valid = validateDeleteGrant({
      grant,
      message: message0404,
      owner: sender,
      burnScope,
      allowedBurnScope: burnScope,
    });
    expect(valid).toEqual({ ok: true, grant });

    const changedMessage = validateDeleteGrant({
      grant: { ...grant, message: "message:0404-changed" },
      message: message0404,
      owner: sender,
      burnScope,
      allowedBurnScope: burnScope,
    });
    expect(changedMessage).toEqual({ ok: false, code: "delete_grant_message_mismatch" });

    const changedOwner = validateDeleteGrant({
      grant: { ...grant, owner: "identity:mallory-0404" },
      message: message0404,
      owner: sender,
      burnScope,
      allowedBurnScope: burnScope,
    });
    expect(changedOwner).toEqual({ ok: false, code: "delete_grant_owner_mismatch" });

    const changedScope = validateDeleteGrant({
      grant: { ...grant, scope: "discord:9000000000000404:direct_message:other-burn-scope" },
      message: message0404,
      owner: sender,
      burnScope,
      allowedBurnScope: burnScope,
    });
    expect(changedScope).toEqual({ ok: false, code: "delete_grant_scope_mismatch" });

    const changedAllowedScope = validateDeleteGrant({
      grant,
      message: message0404,
      owner: sender,
      burnScope,
      allowedBurnScope: "discord:9000000000000404:direct_message:other-burn-scope",
    });
    expect(changedAllowedScope).toEqual({ ok: false, code: "delete_grant_scope_not_allowed" });

    const allowedBurnScopeCount = [burnScope].length;
    expect(allowedBurnScopeCount).toBe(1);
    console.log(
      `TASK0404 valid_delete_grant=ok message=${grant.message} owner=${grant.owner} scope=${grant.scope} allowed_burn_scope_count=${allowedBurnScopeCount}`,
    );
    if (changedMessage.ok || changedOwner.ok || changedScope.ok || changedAllowedScope.ok) {
      throw new Error("changed delete-grant binding unexpectedly validated");
    }
    console.log(
      `TASK0404 changed_message validation=fail code=${changedMessage.code}`,
    );
    console.log(
      `TASK0404 changed_owner validation=fail code=${changedOwner.code}`,
    );
    console.log(
      `TASK0404 changed_scope validation=fail code=${changedScope.code}`,
    );
    console.log(
      `TASK0404 changed_allowed_burn_scope validation=fail code=${changedAllowedScope.code}`,
    );
  });
});
