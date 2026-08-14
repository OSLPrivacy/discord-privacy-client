import {
  drainTerminalRetentionReports,
  RETENTION_TERMINAL_CONFIRMATIONS,
} from "../../src/lib/retention-cleanup-recovery.js";

export default {
  fetch(): Response {
    return new Response(`task-6578-test-worker:${RETENTION_TERMINAL_CONFIRMATIONS}`);
  },
  async scheduled(_event: ScheduledEvent, env: import("../../src/env.js").Env): Promise<void> {
    await drainTerminalRetentionReports(env);
  },
};
