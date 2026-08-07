import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { bothSidesBurnCommand } from "../src/lib/both-sides-burn-command.js";
import {
  DELETE_GRANT_RECORD,
  MESSAGE_READ_KEY_RECORD,
  parseDeleteGrantRecord,
  parseMessageReadKeyRecord,
} from "../src/lib/delete-grant.js";
import { sha256Hex } from "../src/lib/digest.js";
import { d1Count, d1Run } from "./helpers/workerd.js";

const ORIGIN = "https://cipher.test";

interface ProtectedStoredCopyFixture {
  id: string;
  fetchCap: string;
  ackCap: string;
  manageCap: string;
  message: string;
  owner: string;
  burnScope: string;
  deliveryTag: string;
}

async function seedProtectedStoredCopyFixture(fixture: ProtectedStoredCopyFixture) {
  const now = Math.floor(Date.now() / 1000);
  const fetchDigest = await sha256Hex(fixture.fetchCap);
  await d1Run(
    `INSERT INTO blob_capability_index (
       blob_id, fetch_digest_sha256_hex, ack_digest_sha256_hex,
       manage_digest_sha256_hex, object_class, pool, delivery_tag,
       size_bytes, expires_at, created_at,
       delete_grant_message, delete_grant_owner, burn_scope
     ) VALUES (?, ?, ?, ?, 'single-ack', 'undelivered', ?, ?, ?, ?, ?, ?, ?)`,
    fixture.id,
    fetchDigest,
    await sha256Hex(fixture.ackCap),
    await sha256Hex(fixture.manageCap),
    fixture.deliveryTag,
    1,
    now + 3600,
    now,
    fixture.message,
    fixture.owner,
    fixture.burnScope,
  );
  await env.PAYLOADS.put(fetchDigest, new Uint8Array([7]));
  return fetchDigest;
}

function deleteGrant(fixture: ProtectedStoredCopyFixture): string {
  return JSON.stringify({
    record: DELETE_GRANT_RECORD,
    message: fixture.message,
    owner: fixture.owner,
    scope: fixture.burnScope,
  });
}

describe("TASK 0408 both-sides burn command", () => {
  it("submits two separately authorized deletion requests without sharing a read key", async () => {
    const message = "message:0408";
    const burnScope = "discord:9000000000000408:direct_message:stored-protected-message";
    const sender = {
      id: "04080000000000000000000000000001",
      fetchCap: "f4080000000000000000000000000001",
      ackCap: "a4080000000000000000000000000001",
      manageCap: "d4080000000000000000000000000001",
      message,
      owner: "identity:sender-0408",
      burnScope,
      deliveryTag: "1".repeat(32),
    };
    const recipient = {
      id: "04080000000000000000000000000002",
      fetchCap: "f4080000000000000000000000000002",
      ackCap: "a4080000000000000000000000000002",
      manageCap: "d4080000000000000000000000000002",
      message,
      owner: "identity:recipient-0408",
      burnScope,
      deliveryTag: "2".repeat(32),
    };
    const sharedReadKey = JSON.stringify({
      record: MESSAGE_READ_KEY_RECORD,
      message,
      readKey: "8".repeat(32),
    });
    const senderFetchDigest = await seedProtectedStoredCopyFixture(sender);
    const recipientFetchDigest = await seedProtectedStoredCopyFixture(recipient);

    const observedRequests: { url: string; method: string; headers: Headers }[] = [];
    const observedFetch: typeof fetch = async (input, init) => {
      const request = new Request(input, init);
      observedRequests.push({
        url: request.url,
        method: request.method,
        headers: new Headers(request.headers),
      });
      return SELF.fetch(request);
    };

    const report = await bothSidesBurnCommand({
      fetch: observedFetch,
      origin: ORIGIN,
      sender: {
        blobId: sender.id,
        manageCap: sender.manageCap,
        deleteGrant: deleteGrant(sender),
      },
      recipient: {
        blobId: recipient.id,
        manageCap: recipient.manageCap,
        deleteGrant: deleteGrant(recipient),
      },
    });

    const grantHeaders = observedRequests
      .map((request) => request.headers.get("x-osl-delete-grant"))
      .filter((grant): grant is string => grant !== null);
    const requestOwners = grantHeaders.map((grant) => {
      const parsed = parseDeleteGrantRecord(grant);
      if (!parsed.ok) throw new Error(`delete request carried invalid grant: ${parsed.code}`);
      return parsed.grant.owner;
    });
    const requestReadKeyHeaderCount = observedRequests.filter((request) =>
      request.headers.has("x-osl-read-key") || request.headers.has("x-osl-message-read-key")
    ).length;
    const readKeyGrantCount = grantHeaders.filter((grant) => parseMessageReadKeyRecord(grant).ok).length;
    const sharedReadKeyCount = grantHeaders.filter((grant) => grant === sharedReadKey).length;

    expect(report.sender).toEqual({
      deleted: true,
      status: 204,
      owner: sender.owner,
      affected_scope: burnScope,
    });
    expect(report.recipient).toEqual({
      deleted: true,
      status: 204,
      owner: recipient.owner,
      affected_scope: burnScope,
    });
    expect(observedRequests.map((request) => request.method)).toEqual(["DELETE", "DELETE"]);
    expect(requestOwners).toEqual([sender.owner, recipient.owner]);
    expect(new Set(requestOwners).size).toBe(2);
    expect(grantHeaders).toHaveLength(2);
    expect(requestReadKeyHeaderCount).toBe(0);
    expect(readKeyGrantCount).toBe(0);
    expect(sharedReadKeyCount).toBe(0);
    expect(await d1Count("SELECT COUNT(*) FROM blob_capability_index WHERE blob_id IN (?, ?)", sender.id, recipient.id)).toBe(0);
    await expect(env.PAYLOADS.head(senderFetchDigest)).resolves.toBeNull();
    await expect(env.PAYLOADS.head(recipientFetchDigest)).resolves.toBeNull();

    console.log(
      `TASK0408 both_sides_burn request_count=${observedRequests.length} delete_grant_header_count=${grantHeaders.length} sender_status=${report.sender.status} recipient_status=${report.recipient.status} sender_request_owner=${report.sender.owner} recipient_request_owner=${report.recipient.owner} distinct_owner_count=${new Set(requestOwners).size} sender_remaining=0 recipient_remaining=0 read_key_header_count=${requestReadKeyHeaderCount} read_key_grant_count=${readKeyGrantCount} shared_read_key_count=${sharedReadKeyCount} shared_read_key=no sender_scope=${report.sender.affected_scope} recipient_scope=${report.recipient.affected_scope}`,
    );
  });
});
