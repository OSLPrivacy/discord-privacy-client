import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";

import { attachmentProgressMarkup, type AttachmentProgressEvent } from "./attachment-progress";
import {
  directVoiceCallChoice,
  TOR_KEYSERVER_WAIT_DELAY_MS,
  TOR_KEYSERVER_WAIT_LABEL,
  TOR_SLOW_ATTACHMENT_LABEL,
  VOICE_CLIENT_SHIPS_THIS_RELEASE,
  withTorKeyserverPolling,
} from "./tor-breakage-honesty";

describe("TASK 4912 Tor breakage honesty", () => {
  it("wires both Tor honesty states into their shipping app paths", () => {
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    expect(main).toContain("attachmentProgressMarkup(event, torOnboarding.choice)");
    expect(main).toContain("await withTorKeyserverPolling(");
    expect(main).toMatch(/withTorKeyserverPolling\([\s\S]*?addOslFriendByUsername\([\s\S]*?torOnboarding\.choice/u);
    expect(main).toContain('id="friend-form-status" role="status"');
    console.info("TASK4912_SHIPPING_WIRING attachment_tor_route=1 keyserver_poll_wrapper=1 live_status=1");
  });

  it("shows bytes sent and the slow label for a large active Tor attachment before failure", () => {
    const event: AttachmentProgressEvent = {
      contextId: "chat:task-4912",
      job: {
        jobId: "Task4912LargeAttachment",
        metadata: {
          filename: "archive.osl",
          mediaType: "application/octet-stream",
          size: 16 * 1024 * 1024,
        },
        caption: "",
        viewOnce: false,
        stage: "uploading",
        progress: 25,
        retryFrom: null,
        failure: null,
      },
    };

    const markup = attachmentProgressMarkup(event, "tor");
    expect(event.job.failure).toBeNull();
    expect(markup).toContain(TOR_SLOW_ATTACHMENT_LABEL);
    expect(markup).toContain("4,194,304 of 16,777,216 bytes sent");
    expect(markup).toContain('data-network-route="tor"');
    expect(attachmentProgressMarkup(event, "direct")).not.toContain(TOR_SLOW_ATTACHMENT_LABEL);
    console.info(`TASK4912_ATTACHMENT bytes_sent=4194304 total_bytes=16777216 label="${TOR_SLOW_ATTACHMENT_LABEL}" failure_before_label=0`);
  });

  it("shows the Tor keyserver wait at exactly ten seconds and clears it on completion", async () => {
    vi.useFakeTimers();
    try {
      let finish: ((value: string) => void) | null = null;
      const pending = new Promise<string>((resolve) => { finish = resolve; });
      const changes: Array<string | null> = [];
      const poll = withTorKeyserverPolling(
        () => pending,
        "tor",
        (status) => changes.push(status),
      );

      await vi.advanceTimersByTimeAsync(TOR_KEYSERVER_WAIT_DELAY_MS - 1);
      expect(changes).toEqual([]);
      await vi.advanceTimersByTimeAsync(1);
      expect(changes).toEqual([TOR_KEYSERVER_WAIT_LABEL]);
      if (!finish) throw new Error("keyserver fixture did not expose its resolver");
      finish("keys-ready");
      await expect(poll).resolves.toBe("keys-ready");
      expect(changes).toEqual([TOR_KEYSERVER_WAIT_LABEL, null]);
      console.info(`TASK4912_KEY_POLL threshold_ms=${TOR_KEYSERVER_WAIT_DELAY_MS} shown="${TOR_KEYSERVER_WAIT_LABEL}" cleared=1`);
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps voice absent while retaining exactly one future per-call Direct disclosure", () => {
    const choices = [directVoiceCallChoice()];
    expect(VOICE_CLIENT_SHIPS_THIS_RELEASE).toBe(false);
    expect(choices).toHaveLength(1);
    expect(choices[0]).toEqual({
      id: "direct",
      label: "Direct",
      disclosure: "This call will connect directly and may reveal your network path to the voice server.",
    });
    console.info(`TASK4912_DIRECT_CHOICE count=${choices.length} label=${choices[0]?.label} disclosure="${choices[0]?.disclosure}" voice_client_ships=0`);
  });
});
