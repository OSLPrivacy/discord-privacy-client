import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

// T21-T11: the relay route has only opaque tag/ciphertext storage.  This is a
// source-contract test because no relay-side identity fixture may exist.
const source = readFileSync(new URL("../src/endpoints/space-events.ts", import.meta.url), "utf8");
const migration = readFileSync(new URL("../migrations/0041_space_event_queue_reserved.sql", import.meta.url), "utf8");
assert.match(source, /recipientTag\.length !== TAG_BYTES/);
assert.match(source, /INSERT INTO space_event_queue/);
assert.match(source, /WHERE recipient_tag = \?/);
assert.doesNotMatch(migration.replace(/^--.*$/gm, ""), /user_id|space_id|member|roster/i);
