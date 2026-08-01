# OSL Mail IPC contract (v1)

**Status:** FROZEN for the Rust client bridge.  This document records the
renderer-facing interface that already exists in
`apps/osl-hub-ui/src/osl-mail-adapter.ts`.  It does not change the wire
protocol implemented by `keyserver-cf`.

## Boundary

The renderer calls the seven Tauri commands below.  Commands receive the
shown camelCase arguments and return the exact camelCase DTO shown; the
renderer rejects extra or malformed response keys.  An invocation error or a
response that fails the renderer parser is surfaced to the renderer as `null`.

| command | arguments | successful result |
| --- | --- | --- |
| `osl_mail_get_status` | `{}` | `OslMailStatus` |
| `osl_mail_provision` | `{ username: string }` | `OslMailStatus` |
| `osl_mail_list_threads` | none | `OslMailThreadSummary[]` (at most 500) |
| `osl_mail_retrieve_thread` | `{ threadId: string }` | `OslMailRetrievedThread` |
| `osl_mail_acknowledge_retrieval` | `{ retrievalId: string, messageIds: string[] }` | `OslMailDeleteReceipt` |
| `osl_mail_send` | `{ recipient: string, subject: string, body: string }` | `OslMailSendReceipt` |
| `osl_mail_burn` | `{ address: string, confirmation: string }` | `OslMailBurnReceipt` |

The renderer does not invoke malformed requests: addresses and usernames are
lowercase `oslprivacy.com` values, IDs match `[A-Za-z0-9_-]{12,160}`, receipt
digests are 64 lowercase hex characters, acknowledgement arrays contain 1 to
200 IDs, subject is at most 512 bytes, and body is at most 256 KiB.

## Exact response DTOs

`OslMailStatus` is exactly one of:

```ts
{ available: true; provisioned: true; address: `${string}@oslprivacy.com`;
  unreadCount: number; retentionSeconds: number }
{ available: true; provisioned: false; address: null;
  unreadCount: 0; retentionSeconds: number }
```

`unreadCount` is an integer in `0..100000`; `retentionSeconds` is an integer
in `60..604800`.

```ts
type OslMailThreadSummary = {
  threadId: string; subject: string; correspondent: string; latestAt: number;
  unread: boolean; transit: "oslE2ee" | "externalSmtp";
};
type OslMailThreadMessage = {
  messageId: string; from: string; to: string[]; subject: string; body: string;
  receivedAt: number; transit: "oslE2ee" | "externalSmtp";
};
type OslMailRetrievedThread = {
  threadId: string; retrievalId: string; expiresAt: number;
  messages: OslMailThreadMessage[];
};
type OslMailDeleteReceipt = {
  retrievalId: string; deletedMessageIds: string[]; deletedAt: number;
  receiptSha256: string; serverDeleteConfirmed: true;
};
type OslMailSendReceipt = {
  clientMessageId: string; acceptedAt: number; recipient: string;
  transit: "oslE2ee"; receiptSha256: string;
};
type OslMailBurnReceipt = {
  address: string; burnedAt: number; deletedMessages: number;
  receiptSha256: string; mailboxDisabled: true;
};
```

All timestamps are positive safe-integer epoch milliseconds.  Retrieved
threads contain 1 to 200 messages; each recipient list contains 1 to 100
email addresses.  The renderer enforces exact object keys for every DTO.

## Existing server routes and translation ownership

The Rust bridge, not the renderer, owns signing, request IDs, timestamps,
identity lookup, encryption/decryption, and translation to/from the server's
snake_case protocol.  The existing routes are:

| route | operation |
| --- | --- |
| `GET /v1/mail/capabilities` | service capability advertisement |
| `POST /v1/mail/address` | `PROVISION` |
| `POST /v1/mail/consent` | `CONSENT` |
| `POST /v1/mail/send/osl` | `SEND-OSL` |
| `POST /v1/mail/send/external` | always unavailable |
| `POST /v1/mail/list` | `LIST` |
| `POST /v1/mail/fetch` | `FETCH` |
| `POST /v1/mail/ack` | `ACK` |
| `POST /v1/mail/delete` | `DELETE` |
| `POST /v1/mail/burn` | `BURN` |

There is deliberately no one-to-one route mapping.  In particular, the server
has no status route and its list/fetch responses contain ciphertext,
`opaque_thread_token`, and snake_case IDs rather than renderer thread DTOs.
The bridge must derive status from local identity/address state plus signed
mail operations, and it must retain or derive the local presentation metadata
needed to decrypt and group messages.  It must not expose the server response
objects directly to the renderer.

Every mutating/read route except capabilities requires the signed common
fields `user_id`, `request_id`, `timestamp_ms`, and `signature_b64`; the
signature covers `OSL-MAIL-<OPERATION>-v1` and canonical JSON excluding the
signature.  The renderer IPC contract intentionally never accepts these
fields.

## Non-goals

`/v1/mail/consent`, `/v1/mail/delete`, and external outbound are server
capabilities with no renderer IPC command in v1.  Adding a command, changing
an argument name, or relaxing an exact response DTO is a new contract version,
not an implementation detail.
