/// D2 — head-of-line starvation on the inbox drain, and the signed
/// `?sender=` filter that fixes it.
///
/// The first test is a *characterization* of the bug: it proves an
/// unfiltered drain cannot see a message that sits past the 64-row page
/// boundary, using nothing but rows other peers legitimately posted. The
/// rest prove the filter fixes it and that the filter cannot be
/// stripped, forged, or silently ignored.

import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { canonicalControlInboxGetBytes } from "../../src/lib/canonical.js";
import { canonicalControlInboxPostBytes } from "../../src/lib/canonical.js";
import { base64Encode, registerTestUser, signEd25519 } from "./helpers.js";

let seq = 0;
const uid = (prefix: string) => `${prefix}-${Date.now().toString(36)}-${seq++}`;

/** The route's own page size and per-pair admission cap. */
const MAX_DRAIN_ROWS = 64;

async function postOne(
  senderId: string,
  recipientId: string,
  signingKey: CryptoKey,
  marker: string,
): Promise<Response> {
  const bundle = new TextEncoder().encode(marker);
  const bundleHash = new Uint8Array(
    await crypto.subtle.digest("SHA-256", bundle),
  );
  const timestamp_ms = Date.now();
  const scope_id = `scope-${seq++}`;
  const signature_b64 = await signEd25519(
    signingKey,
    canonicalControlInboxPostBytes({
      sender_id: senderId,
      recipient_id: recipientId,
      scope_id,
      timestamp_ms,
      bundle_sha256: bundleHash,
    }),
  );
  return SELF.fetch("http://test/v1/control-inbox", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      sender_id: senderId,
      recipient_id: recipientId,
      scope_id,
      timestamp_ms,
      bundle_b64: base64Encode(bundle),
      signature_b64,
    }),
  });
}

async function drain(
  recipientId: string,
  signingKey: CryptoKey,
  opts: {
    /** Value put in the URL. */
    urlSender?: string | null;
    /** Value put in the SIGNATURE. Defaults to `urlSender`. */
    signedSender?: string | null;
  } = {},
): Promise<Response> {
  const ts = Date.now();
  const urlSender = opts.urlSender ?? null;
  const signedSender =
    opts.signedSender === undefined ? urlSender : opts.signedSender;
  const sig = await signEd25519(
    signingKey,
    canonicalControlInboxGetBytes({
      user_id: recipientId,
      timestamp_ms: ts,
      sender_id: signedSender,
    }),
  );
  let path = `http://test/v1/control-inbox/${encodeURIComponent(recipientId)}?ts=${ts}&sig=${encodeURIComponent(sig)}`;
  if (urlSender !== null) {
    path += `&sender=${encodeURIComponent(urlSender)}`;
  }
  return SELF.fetch(path);
}

async function items(res: Response): Promise<Array<Record<string, unknown>>> {
  expect(res.status).toBe(200);
  const j = (await res.json()) as { items: Array<Record<string, unknown>> };
  return j.items;
}

function bundleText(item: Record<string, unknown>): string {
  return atob(item.bundle_b64 as string);
}

describe("inbox drain — head-of-line starvation", () => {
  // 65 signed POSTs plus three registrations. Comfortably under the
  // route's own 1200/min limiter, but well over vitest's 5s default
  // when the whole suite is running in parallel.
  it("an unfiltered page hides the active peer behind unrelated backlog, and the filter reveals it", async () => {
    const recipientId = uid("recip");
    const recipient = await registerTestUser(SELF, recipientId);

    // Two friends whose conversations are not open. 32 rows each is the
    // per-pair admission cap, so this is entirely legitimate traffic:
    // 64 rows, exactly filling the page.
    const blockers: string[] = [];
    for (let b = 0; b < 2; b++) {
      const blockerId = uid(`blocker${b}`);
      blockers.push(blockerId);
      const blocker = await registerTestUser(SELF, blockerId);
      for (let i = 0; i < MAX_DRAIN_ROWS / 2; i++) {
        const res = await postOne(
          blockerId,
          recipientId,
          blocker.signingKey,
          `blocker-${b}-${i}`,
        );
        expect(res.status).toBe(201);
      }
    }

    // Now the peer we are actually talking to posts.
    const activeId = uid("active");
    const active = await registerTestUser(SELF, activeId);
    expect(
      (await postOne(activeId, recipientId, active.signingKey, "THE-MESSAGE"))
        .status,
    ).toBe(201);

    // Unfiltered: a full page of somebody else's backlog. The message we
    // want is not in it, and nothing reports that.
    const unfiltered = await items(await drain(recipientId, recipient.signingKey));
    expect(unfiltered.length).toBe(MAX_DRAIN_ROWS);
    expect(unfiltered.some((i) => bundleText(i) === "THE-MESSAGE")).toBe(false);
    expect(unfiltered.every((i) => blockers.includes(i.sender_id as string))).toBe(
      true,
    );

    // Filtered: immediately reachable, and delivery no longer depends on
    // anybody else's backlog.
    const res = await drain(recipientId, recipient.signingKey, {
      urlSender: activeId,
    });
    const filtered = await items(res);
    expect(filtered.length).toBe(1);
    expect(bundleText(filtered[0]!)).toBe("THE-MESSAGE");
    expect(filtered[0]!.sender_id).toBe(activeId);
  }, 60_000);

  it("echoes the filter it honoured", async () => {
    const recipientId = uid("recip");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = uid("sender");
    const sender = await registerTestUser(SELF, senderId);
    await postOne(senderId, recipientId, sender.signingKey, "hi");

    const res = await drain(recipientId, recipient.signingKey, {
      urlSender: senderId,
    });
    const j = (await res.json()) as Record<string, unknown>;
    expect(j.filtered_sender_id).toBe(senderId);

    // The unfiltered form does not gain the field.
    const plain = await drain(recipientId, recipient.signingKey);
    expect((await plain.json() as Record<string, unknown>).filtered_sender_id)
      .toBeUndefined();
  });
});

describe("inbox drain — the filter is signed", () => {
  it("stripping ?sender= from a filtered request is refused, not served unfiltered", async () => {
    const recipientId = uid("recip");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = uid("sender");
    const sender = await registerTestUser(SELF, senderId);
    await postOne(senderId, recipientId, sender.signingKey, "hi");

    // Signed WITH the filter, sent WITHOUT it: the reconstruction no
    // longer matches. Must be 401, never a 200 with an unfiltered page.
    const res = await drain(recipientId, recipient.signingKey, {
      urlSender: null,
      signedSender: senderId,
    });
    expect(res.status).toBe(401);
  });

  it("adding ?sender= to an unfiltered signature is refused", async () => {
    const recipientId = uid("recip");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = uid("sender");
    const sender = await registerTestUser(SELF, senderId);
    await postOne(senderId, recipientId, sender.signingKey, "hi");

    const res = await drain(recipientId, recipient.signingKey, {
      urlSender: senderId,
      signedSender: null,
    });
    expect(res.status).toBe(401);
  });

  it("substituting a different sender in the URL is refused", async () => {
    const recipientId = uid("recip");
    const recipient = await registerTestUser(SELF, recipientId);
    const aId = uid("a");
    const a = await registerTestUser(SELF, aId);
    const bId = uid("b");
    await registerTestUser(SELF, bId);
    await postOne(aId, recipientId, a.signingKey, "from-a");

    const res = await drain(recipientId, recipient.signingKey, {
      urlSender: aId,
      signedSender: bId,
    });
    expect(res.status).toBe(401);
  });

  it("a malformed ?sender= is a 400, never a silent unfiltered page", async () => {
    const recipientId = uid("recip");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = uid("sender");
    const sender = await registerTestUser(SELF, senderId);
    await postOne(senderId, recipientId, sender.signingKey, "hi");

    for (const bad of ["", "x".repeat(257), "has\nnewline"]) {
      const res = await drain(recipientId, recipient.signingKey, {
        urlSender: bad,
      });
      expect(res.status, `sender=${JSON.stringify(bad)}`).toBe(400);
    }
  });

  it("the filter is not a probe: it only ever narrows the caller's own rows", async () => {
    const victimId = uid("victim");
    const victim = await registerTestUser(SELF, victimId);
    const attackerId = uid("attacker");
    const attacker = await registerTestUser(SELF, attackerId);
    const senderId = uid("sender");
    const sender = await registerTestUser(SELF, senderId);

    // A row addressed to the victim.
    await postOne(senderId, victimId, sender.signingKey, "for-victim");

    // The attacker drains their OWN inbox filtered by the same sender.
    // `recipient_id` is bound from the authenticated user, so this
    // cannot reach the victim's row.
    const mine = await items(
      await drain(attackerId, attacker.signingKey, { urlSender: senderId }),
    );
    expect(mine.length).toBe(0);

    // And the attacker cannot sign for the victim's user_id.
    const forged = await drain(victimId, attacker.signingKey, {
      urlSender: senderId,
    });
    expect(forged.status).toBe(401);

    // The victim, of course, sees it.
    const theirs = await items(
      await drain(victimId, victim.signingKey, { urlSender: senderId }),
    );
    expect(theirs.length).toBe(1);
  });
});
