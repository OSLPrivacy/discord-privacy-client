import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { senderReceiptStatus } from "./receipt-status";
import {
  oslChatsViewMarkup,
  senderReceiptStateFor,
  type OslChatFriend,
  type OslChatMessage,
  type OslChatsViewModel,
} from "./osl-chats-view";

/**
 * D-136 and D-135: two surfaces that stated something the product does not know.
 *
 * D-136 -- `Delivery receipt` mapped `delivered`/`opened`/`expired` and fell
 * through to `Prepared`. Nothing in the tree ever set any of the three on an
 * outgoing message, so `Not confirmed` was the only reachable value on every
 * message anyone would ever send. The state that IS set is `received`, from the
 * peer's signed `Received` acknowledgment; it was being discarded.
 *
 * D-135 -- the View once composer promised `Removed after it is opened`, while
 * `DeletionDrainReport::retained` (the only evidence a relay copy survives) was
 * computed and thrown away in Rust.
 *
 * The privacy property that outranks both: a MISSING receipt must stay
 * ambiguous. `receipt-status.ts` collapses every pre-receipt and unknown state
 * into one wording on purpose, so "they have receipts off" is not
 * distinguishable from "it has not arrived". Any change that makes those two
 * render differently is a regression even if every other test here passes.
 */

const friend: OslChatFriend = {
  personId: "friend-1",
  nickname: "Bob",
  verified: true,
  ready: true,
  preview: null,
  previewVisible: true,
  unreadCount: 0,
  handshakeConfirmed: true,
};

function outgoing(state: OslChatMessage["state"]): OslChatMessage {
  return { messageId: "m1", direction: "outgoing", body: "hello", state, timestampLabel: "Now" };
}

function model(overrides: Partial<OslChatsViewModel> = {}): OslChatsViewModel {
  return {
    friends: [friend],
    activePersonId: "friend-1",
    messages: [outgoing("sent")],
    draft: "",
    busy: false,
    viewOnce: false,
    ...overrides,
  };
}

/** The exact markup main.ts renders, so this file tests the shipped sentence. */
function receiptMarkup(messages: readonly OslChatMessage[]): string {
  const receipt = senderReceiptStatus(senderReceiptStateFor(messages));
  return `<p class="setting-line osl-chat-receipt-status" data-osl-chat-receipt-confirmed="${receipt.confirmed}"><span><strong>Delivery receipt</strong><small>${receipt.label}</small></span></p>`;
}

function viewOnceLabel(markup: string): string {
  const start = markup.indexOf('<label class="osl-chat-view-once"');
  expect(start).toBeGreaterThan(-1);
  return markup.slice(start, markup.indexOf("</label>", start) + "</label>".length);
}

describe("D-136 the delivery receipt reports a state the product actually learns", () => {
  it("confirms delivery from the acknowledgment the peer actually sends", () => {
    // `received` is what commitOslChatBatch writes from
    // NativeDiscordOverlayAcknowledgment.status, which the peer posts as a
    // signed Received acknowledgment (broker.rs:5659 / :4726 / :4943).
    const markup = receiptMarkup([outgoing("received")]);

    expect(senderReceiptStateFor([outgoing("received")])).toBe("Delivered");
    expect(markup).toContain('data-osl-chat-receipt-confirmed="true"');
    expect(markup).toContain("Their app reported it delivered");
  });

  it("has more than one reachable value, so the field is not decoration", () => {
    const reachable = new Set(
      (["queued", "sent", "received", "opened", "failed"] as const)
        .map((state) => receiptMarkup([outgoing(state)])),
    );

    expect(reachable.size).toBeGreaterThan(1);
  });

  it("reports only the latest outgoing message and ignores incoming ones", () => {
    const incoming: OslChatMessage = {
      messageId: "m0", direction: "incoming", body: "hi", state: "opened", timestampLabel: "Now",
    };

    expect(senderReceiptStateFor([incoming])).toBeNull();
    expect(senderReceiptStateFor([outgoing("received"), incoming])).toBe("Delivered");
    expect(senderReceiptStateFor([outgoing("received"), { ...outgoing("sent"), messageId: "m2" }])).toBeNull();
  });

  it("keeps a missing receipt indistinguishable from receipt opt-out", () => {
    // Every state that carries no receipt must render the SAME bytes. If a
    // future change lets a reader tell "no receipt yet" from "they turned
    // receipts off", opt-out becomes a signal and this fails.
    const unconfirmed = [
      receiptMarkup([]),
      receiptMarkup([outgoing("queued")]),
      receiptMarkup([outgoing("sent")]),
      receiptMarkup([outgoing("failed")]),
      receiptMarkup([outgoing("delivered")]),
      receiptMarkup([outgoing("expired")]),
      receiptMarkup([{ messageId: "m0", direction: "incoming", body: "hi", state: "received", timestampLabel: "Now" }]),
    ];

    expect(new Set(unconfirmed).size).toBe(1);
    expect(unconfirmed[0]).toContain('data-osl-chat-receipt-confirmed="false"');
    expect(unconfirmed[0]).toContain("Not confirmed");
    expect(unconfirmed[0]).not.toMatch(/read|opt|disabled|off\b/iu);
  });
});

describe("D-135 a requested relay deletion is never shown as a verified one", () => {
  it("reports copies OSL asked to delete and could not confirm gone", () => {
    const markup = oslChatsViewMarkup(model({ deletionUnconfirmed: 2 }));

    expect(markup).toContain('data-osl-deletion-unconfirmed="2"');
    expect(markup).toContain('data-deletion-status="not-confirmed"');
    expect(markup).toContain("has not been able to confirm it");
    expect(markup).toContain("assume the copy is still there");
  });

  it("keeps naming no transport internals, in the new copy too", () => {
    // osl-chats-view.test.ts bars this view from saying relay, server or
    // keyserver. The honest sentence had to be written inside that rule, not
    // by widening it.
    const markup = oslChatsViewMarkup(model({ deletionUnconfirmed: 4 }));

    expect(markup).not.toMatch(/relay|server|keyserver/iu);
  });

  it("never renders a retained copy as removed, deleted or gone", () => {
    // The prohibition itself. `retained > 0` means OSL asked and does not know;
    // any word here that reads as an accomplished removal is master 7.5 broken
    // in the same place it was just fixed.
    const start = oslChatsViewMarkup(model({ deletionUnconfirmed: 3 }))
      .indexOf('<p class="osl-chat-deletion-unconfirmed');
    const row = oslChatsViewMarkup(model({ deletionUnconfirmed: 3 })).slice(start, start + 600);

    expect(row).not.toMatch(/\b(removed|erased|destroyed|wiped)\b/iu);
    expect(row).not.toMatch(/\b(is|are|was|were)\s+(deleted|gone)\b/iu);
    expect(row).not.toMatch(/\bconfirmed\s+(deleted|removed|gone)\b/iu);
  });

  it("says nothing when nothing is known to be owed", () => {
    // Two states, not three: "nothing owed that OSL knows of" is silence, and
    // silence is not a claim that a copy is gone.
    expect(oslChatsViewMarkup(model({ deletionUnconfirmed: 0 }))).not.toContain("osl-chat-deletion-unconfirmed");
    expect(oslChatsViewMarkup(model())).not.toContain("osl-chat-deletion-unconfirmed");
    expect(oslChatsViewMarkup(model({ deletionUnconfirmed: -1 }))).not.toContain("osl-chat-deletion-unconfirmed");
  });

  it("does not promise removal in the View once composer label", () => {
    const label = viewOnceLabel(oslChatsViewMarkup(model()));

    expect(label).not.toMatch(/Removed after it is opened/iu);
    expect(label).toContain("asks for the sent copy to be deleted");
    expect(label).toContain("cannot confirm");
    expect(label).toContain("Kept out of OSL history");
  });
});

/**
 * Both halves of the emit/listen pair, checked from the side that runs.
 *
 * These assertions belong in `native_attachment_transport.rs`, and they cannot
 * live there. That file is compiled into the `osl-privacy-hub` BINARY, not the
 * `osl_privacy_hub` lib; the binary's test target does not compile on Linux at
 * all -- 27 errors from Windows-only symbols in `native_discord_overlay.rs`,
 * present on a pristine `integrate/first-usable` checkout -- and every CI job
 * that touches Rust runs `--features core --lib` plus the single
 * `windows_identity_lifecycle` integration test (`.github/workflows/rust-test.yml:51-53`,
 * `osl-hub-release.yml:60-63`). A `#[cfg(test)]` block in that module would
 * never execute anywhere, so putting the guard there would be decoration.
 * `npm test` does run in CI (`.github/workflows/ts-test.yml`), so the guard is
 * here.
 *
 * It also happens to be the right shape for this defect: the advisory this
 * replaces was deleted for being emitted-but-never-listened, and only a check
 * that reads BOTH files can catch that recurring.
 */
describe("D-135 the drain report crosses the boundary and something listens", () => {
  const transport = readFileSync(
    new URL("../../osl-hub/src/native_attachment_transport.rs", import.meta.url),
    "utf8",
  );
  const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  const production = transport.split("#[cfg(test)]\nmod tests {")[0];
  const EVENT = "osl://attachment-deletion-unconfirmed";

  it("does not discard the retained count again", () => {
    // Mutant [2]. Re-adding `.map(|_report| ())` also breaks the return type,
    // but a mutation that removes the reports alongside it would compile clean
    // and go silent, so the reports are counted rather than left to rustc.
    const signature =
      "pub(crate) fn retry_pending_deletions(\n    client: &ipc::cipher_store_client::CipherStoreClient,\n) -> Result<peer_attachment_io::DeletionDrainReport, String> {";
    expect(production).toContain(signature);
    // Code only. The doc comment above deliberately quotes the old
    // `.map(|_report| ())` to record what was wrong with it.
    const body = production.split(signature)[1].split("\n}\n")[0];
    expect(body).not.toContain(".map(|_report| ())");
    // The definition plus exactly four callers: attachment send, attachment
    // open, the detached pass and the periodic drain. A drain pass with no
    // report is a deletion OSL cannot confirm and never mentions.
    expect(production.match(/report_deletion_drain\(/gu)).toHaveLength(5);
  });

  it("sends only the unconfirmed count, and only to the window that subscribes", () => {
    // Mutant [3] at the boundary. `deleted` and `already_gone` are not
    // readbacks of the relay; handing them to the renderer invites a surface
    // that reads as a verified deletion (master 7.5).
    const report = production.split("fn report_deletion_drain(")[1].split("\n}\n")[0];

    expect(report).toContain('"retained": report.retained');
    expect(report).not.toContain("report.deleted");
    expect(report).not.toContain("report.already_gone");
    expect(report).toContain('emit_to(\n        "main",');
  });

  it("keeps the emit and the listener pointed at the same name", () => {
    expect(production).toContain(`"${EVENT}"`);
    expect(main).toContain(`listen<unknown>("${EVENT}"`);
    expect(main).toContain("parseAttachmentDeletionUnconfirmedEvent");
    expect(main).toMatch(/deletionUnconfirmed: oslChatDeletionUnconfirmed/u);
  });
});
