import type { Env } from "../env.js";
import {
  CONTROL_INBOX_EVICTION_SIGNAL_CAPABILITY,
  controlInboxDispositionSchemaReady,
} from "../lib/control-inbox-sweep.js";
import { json } from "../lib/http.js";

export async function handleHealthz(env: Env): Promise<Response> {
  const controlInboxSenderDisposition =
    await controlInboxDispositionSchemaReady(env.DB);
  return json(
    {
      ok: controlInboxSenderDisposition,
      capabilities: {
        control_inbox_sender_disposition: controlInboxSenderDisposition ? 1 : 0,
        [CONTROL_INBOX_EVICTION_SIGNAL_CAPABILITY]:
          controlInboxSenderDisposition ? 1 : 0,
      },
    },
    controlInboxSenderDisposition ? undefined : { status: 503 },
  );
}
