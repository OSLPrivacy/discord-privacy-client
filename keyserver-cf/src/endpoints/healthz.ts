import type { Env } from "../env.js";
import {
  CONTROL_INBOX_EVICTION_SIGNAL_CAPABILITY,
  controlInboxDispositionSchemaReady,
} from "../lib/control-inbox-sweep.js";
import { json } from "../lib/http.js";

function nonEmpty(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

export async function handleHealthz(env: Env): Promise<Response> {
  const controlInboxSenderDisposition =
    await controlInboxDispositionSchemaReady(env.DB);
  const revision =
    nonEmpty(env.OSL_SERVER_REVISION) ??
    nonEmpty(env.CF_VERSION_METADATA?.tag) ??
    nonEmpty(env.CF_VERSION_METADATA?.id) ??
    "unknown";
  const buildTime =
    nonEmpty(env.OSL_SERVER_BUILD_TIME) ??
    nonEmpty(env.CF_VERSION_METADATA?.timestamp) ??
    "unknown";
  const configurationName =
    nonEmpty(env.OSL_SERVER_CONFIGURATION_NAME) ??
    nonEmpty(env.DEPLOYMENT_ENV) ??
    "unknown";
  return json(
    {
      ok: controlInboxSenderDisposition,
      revision,
      build_time: buildTime,
      configuration_name: configurationName,
      capabilities: {
        control_inbox_sender_disposition: controlInboxSenderDisposition ? 1 : 0,
        [CONTROL_INBOX_EVICTION_SIGNAL_CAPABILITY]:
          controlInboxSenderDisposition ? 1 : 0,
      },
    },
    controlInboxSenderDisposition ? undefined : { status: 503 },
  );
}
