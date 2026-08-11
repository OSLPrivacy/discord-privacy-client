import { env, SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { PrivateContactLinkPageController } from "../../../apps/osl-hub-ui/src/onboarding-identity.js";

const DB = (env as unknown as { DB: D1Database }).DB;
const SERVICE = "http://task-0310a-service";

interface IssuedLink {
  link_value: string;
  revocation_secret: string;
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
  uses_allowed: 1;
  uses_recorded: 0;
}

interface Status {
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
  terminal_state: "live" | "redeemed" | "revoked" | "expired";
  terminal_at_unix_seconds: number | null;
}

function bundle(label: string): string {
  return btoa(`OSLFR1.${label}.identity-and-contact-bytes`);
}

function asWire(link: {
  linkValue: string;
  revocationSecret: string;
  issuedAtUnixSeconds: number;
  expiresAtUnixSeconds: number;
}): IssuedLink {
  return {
    link_value: link.linkValue,
    revocation_secret: link.revocationSecret,
    issued_at_unix_seconds: link.issuedAtUnixSeconds,
    expires_at_unix_seconds: link.expiresAtUnixSeconds,
    uses_allowed: 1,
    uses_recorded: 0,
  };
}

async function post(path: string, value: unknown, client: string): Promise<Response> {
  return SELF.fetch(`${SERVICE}${path}`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "cf-connecting-ip": client,
      // A client clock claim is deliberately irrelevant: service time is not
      // read from this header or from the request body.
      "x-client-unix-seconds": "4102444800",
    },
    body: JSON.stringify(value),
  });
}

async function issue(label: string): Promise<IssuedLink> {
  const response = await post(
    "/v1/private-contact-links/issue",
    { contact_bundle: bundle(label) },
    "192.0.2.31",
  );
  expect(response.status).toBe(201);
  return await response.json() as IssuedLink;
}

async function status(link: IssuedLink): Promise<Status> {
  const response = await post("/v1/private-contact-links/status", {
    link_value: link.link_value,
    revocation_secret: link.revocation_secret,
  }, "192.0.2.31");
  expect(response.status).toBe(200);
  return await response.json() as Status;
}

async function redeem(link: IssuedLink, client: string): Promise<Response> {
  return post("/v1/private-contact-links/redeem", { link_value: link.link_value }, client);
}

async function revoke(link: IssuedLink): Promise<Response> {
  return post("/v1/private-contact-links/revoke", {
    link_value: link.link_value,
    revocation_secret: link.revocation_secret,
  }, "192.0.2.31");
}

async function refusalHasZeroContactBytes(response: Response, forbidden: string): Promise<string> {
  const text = await response.text();
  expect(response.status).toBe(410);
  expect(text).toBe('{"error":"private contact link unavailable"}');
  expect(text).not.toContain(forbidden);
  expect(text).not.toContain("OSLFR1");
  expect(text).not.toContain("contact_bundle");
  return text;
}

async function waitPast(expiresAt: number): Promise<void> {
  const delay = Math.max(0, expiresAt * 1000 - Date.now() + 1100);
  await new Promise((resolve) => setTimeout(resolve, delay));
}

describe("TASK 0310a authoritative private contact links", () => {
  it("expires, revokes, and atomically arbitrates rendered-link service decisions", async () => {
    await DB.prepare("DELETE FROM private_contact_links").run();
    // The public directory is a separate production surface. The focused
    // service config intentionally applies only migration 0050 (the historical
    // full set currently has unrelated duplicate CREATEs), so create only the
    // readback table needed to prove this journey wrote no searchable name.
    await DB.prepare(
      `CREATE TABLE IF NOT EXISTS public_name_directory (
         name TEXT PRIMARY KEY,
         identity_fingerprint TEXT NOT NULL,
         claimed_at TEXT NOT NULL,
         updated_at TEXT NOT NULL
       )`,
    ).run();

    // One no-public-name shipping page creates three distinct service-issued
    // links through the real production controller and renders service expiry.
    const issueLabels = ["first-0310a", "second-0310a", "third-0310a"];
    const page = new PrivateContactLinkPageController({
      async create() {
        const issued = await issue(issueLabels.shift() ?? "unexpected-0310a");
        return {
          linkValue: issued.link_value,
          revocationSecret: issued.revocation_secret,
          issuedAtUnixSeconds: issued.issued_at_unix_seconds,
          expiresAtUnixSeconds: issued.expires_at_unix_seconds,
          usesAllowed: 1,
          usesRecorded: 0,
        };
      },
      async revoke(link) {
        return (await revoke(asWire(link))).status === 200;
      },
    });
    expect(await page.create()).toBe(true);
    expect(await page.create()).toBe(true);
    expect(await page.create()).toBe(true);
    const [first, second, third] = page.links.map(asWire) as [IssuedLink, IssuedLink, IssuedLink];
    expect(new Set([first.link_value, second.link_value, third.link_value]).size).toBe(3);
    for (const link of [first, second, third]) {
      expect(link.link_value).toMatch(/^OSLCL2\.[A-Za-z0-9_-]{43}$/u);
      expect(link.uses_allowed).toBe(1);
      expect(link.expires_at_unix_seconds - link.issued_at_unix_seconds).toBe(3);
      expect(link.expires_at_unix_seconds - link.issued_at_unix_seconds).toBeLessThanOrEqual(86_400);
    }
    const rendered = page.render();
    expect(rendered.match(/data-private-link-expiry/gu)).toHaveLength(3);
    expect(rendered.match(/Revoke now/gu)).toHaveLength(3);
    expect(rendered).toContain(`data-expires-at-unix-seconds="${second.expires_at_unix_seconds}"`);
    expect(rendered).toContain(new Date(second.expires_at_unix_seconds * 1000).toISOString());
    expect(rendered).toContain('data-searchable-identifier="none"');

    // A clean second client gets the first contact once, before the exact
    // service expiry; replay is refused without returning the bundle again.
    const firstPositive = await redeem(first, "198.51.100.32");
    expect(firstPositive.status).toBe(200);
    const firstBody = await firstPositive.json() as { accepted: boolean; contact_bundle: string };
    expect(firstBody.accepted).toBe(true);
    expect(firstBody.contact_bundle).toBe(bundle("first-0310a"));
    await refusalHasZeroContactBytes(
      await redeem(first, "198.51.100.33"),
      "first-0310a",
    );

    // The second link is independently known live and unredeemed before the
    // Worker clock reaches its displayed finite expiry.
    expect((await status(second)).terminal_state).toBe("live");

    // The rendered issuer control's production operation revokes the third
    // while it is live. A clean client is refused immediately.
    expect(await page.revoke(third.link_value)).toBe(true);
    expect(page.render()).toContain('data-private-link-state="revoked"');
    expect((await status(third)).terminal_state).toBe("revoked");
    const revokedAttempt = await redeem(third, "198.51.100.34");
    if (revokedAttempt.status === 200) {
      throw new Error(`TASK0310A revocation leaked link ${third.link_value} remained usable`);
    }
    await refusalHasZeroContactBytes(
      revokedAttempt,
      "third-0310a",
    );

    await waitPast(second.expires_at_unix_seconds);

    // No client time was consulted. The service clock makes the untouched
    // second bearer terminal and releases zero contact or identity bytes.
    const expiredAttempt = await redeem(second, "203.0.113.35");
    if (expiredAttempt.status === 200) {
      throw new Error(`TASK0310A expiry leaked link ${second.link_value} remained usable`);
    }
    await refusalHasZeroContactBytes(
      expiredAttempt,
      "second-0310a",
    );
    expect((await status(second)).terminal_state).toBe("expired");

    // A later Worker invocation represents an issuer restart: the D1 terminal
    // result, not a local row or process memory, still refuses the third link.
    await refusalHasZeroContactBytes(
      await redeem(third, "203.0.113.36"),
      "third-0310a",
    );
    expect((await status(third)).terminal_state).toBe("revoked");

    // A fresh fourth link races a clean redemption against issuer revocation.
    // Both operations use the same primary conditional transition.
    const fourth = await issue("fourth-race-0310a");
    const [raceRedeem, raceRevoke] = await Promise.all([
      redeem(fourth, "198.51.100.37"),
      revoke(fourth),
    ]);
    const raceWins = Number(raceRedeem.status === 200) + Number(raceRevoke.status === 200);
    expect(raceWins).toBe(1);
    const fourthFinal = await status(fourth);
    expect(["redeemed", "revoked"]).toContain(fourthFinal.terminal_state);

    // Two later Worker instances/replicas may have stale pre-race knowledge,
    // but neither can bypass the primary CAS or replay the terminal bearer.
    await refusalHasZeroContactBytes(
      await redeem(fourth, "203.0.113.38"),
      "fourth-race-0310a",
    );
    await refusalHasZeroContactBytes(
      await redeem(fourth, "203.0.113.39"),
      "fourth-race-0310a",
    );

    // Private-link issuance never writes a public name. The shipping lookup
    // endpoint still reports no directory entry for this account choice.
    const lookup = await DB.prepare(
      "SELECT COUNT(*) AS count FROM public_name_directory WHERE name = ?",
    ).bind("task0310a_private_issuer").first<{ count: number }>();
    expect(lookup?.count).toBe(0);

    const states = await DB.prepare(
      "SELECT terminal_state, COUNT(*) AS count FROM private_contact_links GROUP BY terminal_state",
    ).all<{ terminal_state: string; count: number }>();
    console.log(
      `TASK0310A links=3 distinct=3 rendered_expiries=3 rendered_revoke_controls=3 lifetime_seconds=3 displayed_expiry=${second.expires_at_unix_seconds} `
      + `first_clean_redemptions=1 first_replays=0 second_pre_expiry=live second_after_expiry=expired `
      + `expired_refusal_contact_bytes=0 revoked_immediate=1 revoked_after_restart=1 `
      + `race_winners=${raceWins} race_final=${fourthFinal.terminal_state} replica_replays=0 `
      + `public_name_lookup_rows=${lookup?.count ?? -1} client_clock_authority=false states=${JSON.stringify(states.results)}`,
    );
  });
});
