//! Authoritative private-contact link issuance, redemption, and revocation.
//!
//! Service time is the only clock used. Redemption and revocation compete for
//! one conditional D1 transition, so exactly one can move a live row into a
//! terminal state. No contact bytes are selected until redemption wins.

import type { Env } from "../env.js";
import { badRequest, conflict, gone, json, notFound, serverError } from "../lib/http.js";

const LINK_PREFIX = "OSLCL2.";
const CAPABILITY_BYTES = 32;
const CAPABILITY_CHARS = 43;
const MAX_CONTACT_BUNDLE_CHARS = 12_288;
export const PRIVATE_CONTACT_LINK_MAX_LIFETIME_SECONDS = 24 * 60 * 60;

type TerminalState = "live" | "redeemed" | "revoked" | "expired";

interface LinkRow {
  contact_bundle: string;
  issued_at_unix_seconds: number;
  expires_at_unix_seconds: number;
  terminal_state: TerminalState;
  terminal_at_unix_seconds: number | null;
}

function exactObject(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  return actual.length === keys.length && actual.every((key, index) => key === [...keys].sort()[index]);
}

async function body(request: Request, keys: readonly string[]): Promise<Record<string, unknown> | null> {
  try {
    const value = await request.json();
    return exactObject(value, keys) ? value : null;
  } catch {
    return null;
  }
}

function base64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/u, "");
}

function capability(): string {
  return base64Url(crypto.getRandomValues(new Uint8Array(CAPABILITY_BYTES)));
}

function parseLink(value: unknown): string | null {
  if (typeof value !== "string" || !value.startsWith(LINK_PREFIX)) return null;
  const token = value.slice(LINK_PREFIX.length);
  return token.length === CAPABILITY_CHARS && /^[A-Za-z0-9_-]+$/u.test(token) ? value : null;
}

function parseRevocationSecret(value: unknown): string | null {
  return typeof value === "string"
    && value.length === CAPABILITY_CHARS
    && /^[A-Za-z0-9_-]+$/u.test(value)
    ? value
    : null;
}

function parseContactBundle(value: unknown): string | null {
  if (typeof value !== "string" || value.length < 1 || value.length > MAX_CONTACT_BUNDLE_CHARS) return null;
  if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u.test(value)) return null;
  return value;
}

async function sha256(value: string): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value)));
  return Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function serviceNow(): number {
  return Math.floor(Date.now() / 1000);
}

function lifetime(env: Env): number {
  const configured = Number(env.PRIVATE_CONTACT_LINK_TTL_SECONDS ?? PRIVATE_CONTACT_LINK_MAX_LIFETIME_SECONDS);
  return Number.isSafeInteger(configured)
    && configured >= 1
    && configured <= PRIVATE_CONTACT_LINK_MAX_LIFETIME_SECONDS
    ? configured
    : PRIVATE_CONTACT_LINK_MAX_LIFETIME_SECONDS;
}

async function expireIfDue(env: Env, linkDigest: string, now: number): Promise<void> {
  await env.DB.prepare(
    `UPDATE private_contact_links
        SET terminal_state = 'expired', terminal_at_unix_seconds = ?
      WHERE link_sha256 = ?
        AND terminal_state = 'live'
        AND expires_at_unix_seconds <= ?`,
  ).bind(now, linkDigest, now).run();
}

function unavailable(): Response {
  // One body for unknown, used, expired, and revoked bearers. It contains no
  // contact bundle, person ID, public name, or terminal-state oracle.
  return gone("private contact link unavailable");
}

export async function handlePrivateContactLinkIssue(request: Request, env: Env): Promise<Response> {
  const parsed = await body(request, ["contact_bundle"]);
  const contactBundle = parsed && parseContactBundle(parsed.contact_bundle);
  if (!contactBundle) return badRequest("invalid private contact link issue request");

  const now = serviceNow();
  const expiresAt = now + lifetime(env);
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const linkValue = `${LINK_PREFIX}${capability()}`;
    const revocationSecret = capability();
    try {
      await env.DB.prepare(
        `INSERT INTO private_contact_links
           (link_sha256, revoke_sha256, contact_bundle,
            issued_at_unix_seconds, expires_at_unix_seconds,
            terminal_state, terminal_at_unix_seconds)
         VALUES (?, ?, ?, ?, ?, 'live', NULL)`,
      ).bind(
        await sha256(linkValue),
        await sha256(revocationSecret),
        contactBundle,
        now,
        expiresAt,
      ).run();
      return json({
        link_value: linkValue,
        revocation_secret: revocationSecret,
        issued_at_unix_seconds: now,
        expires_at_unix_seconds: expiresAt,
        uses_allowed: 1,
        uses_recorded: 0,
      }, { status: 201 });
    } catch {
      // A random capability collision retries; persistent DB failures fall
      // through to a service error without issuing a bearer the DB cannot see.
    }
  }
  return serverError("private contact link could not be issued");
}

export async function handlePrivateContactLinkRedeem(request: Request, env: Env): Promise<Response> {
  const parsed = await body(request, ["link_value"]);
  const linkValue = parsed && parseLink(parsed.link_value);
  if (!linkValue) return badRequest("invalid private contact link redemption");
  const digest = await sha256(linkValue);
  const now = serviceNow();

  const won = await env.DB.prepare(
    `UPDATE private_contact_links
        SET terminal_state = 'redeemed', terminal_at_unix_seconds = ?
      WHERE link_sha256 = ?
        AND terminal_state <> 'redeemed'
        AND terminal_state <> 'expired'
        AND terminal_state <> 'revoked'
        AND expires_at_unix_seconds > ?`,
  ).bind(now, digest, now).run();
  if ((won.meta?.changes ?? 0) !== 1) {
    await expireIfDue(env, digest, now);
    return unavailable();
  }

  const row = await env.DB.prepare(
    `SELECT contact_bundle, issued_at_unix_seconds, expires_at_unix_seconds,
            terminal_state, terminal_at_unix_seconds
       FROM private_contact_links WHERE link_sha256 = ?`,
  ).bind(digest).first<LinkRow>();
  if (!row || row.terminal_state !== "redeemed") {
    return serverError("private contact link terminal state is unavailable");
  }
  return json({
    accepted: true,
    contact_bundle: row.contact_bundle,
    issued_at_unix_seconds: row.issued_at_unix_seconds,
    expires_at_unix_seconds: row.expires_at_unix_seconds,
    uses_allowed: 1,
    uses_recorded: 1,
  });
}

export async function handlePrivateContactLinkRevoke(request: Request, env: Env): Promise<Response> {
  const parsed = await body(request, ["link_value", "revocation_secret"]);
  const linkValue = parsed && parseLink(parsed.link_value);
  const revocationSecret = parsed && parseRevocationSecret(parsed.revocation_secret);
  if (!linkValue || !revocationSecret) return badRequest("invalid private contact link revocation");
  const linkDigest = await sha256(linkValue);
  const revokeDigest = await sha256(revocationSecret);
  const now = serviceNow();

  const won = await env.DB.prepare(
    `UPDATE private_contact_links
        SET terminal_state = 'revoked', terminal_at_unix_seconds = ?
      WHERE link_sha256 = ?
        AND revoke_sha256 = ?
        AND terminal_state = 'live'
        AND expires_at_unix_seconds > ?`,
  ).bind(now, linkDigest, revokeDigest, now).run();
  if ((won.meta?.changes ?? 0) === 1) {
    return json({ revoked: true, terminal_state: "revoked", terminal_at_unix_seconds: now });
  }
  await expireIfDue(env, linkDigest, now);
  const row = await env.DB.prepare(
    `SELECT terminal_state FROM private_contact_links
      WHERE link_sha256 = ? AND revoke_sha256 = ?`,
  ).bind(linkDigest, revokeDigest).first<{ terminal_state: TerminalState }>();
  if (!row) return notFound("private contact link control unavailable");
  return conflict(`private contact link is terminal: ${row.terminal_state}`);
}

export async function handlePrivateContactLinkStatus(request: Request, env: Env): Promise<Response> {
  const parsed = await body(request, ["link_value", "revocation_secret"]);
  const linkValue = parsed && parseLink(parsed.link_value);
  const revocationSecret = parsed && parseRevocationSecret(parsed.revocation_secret);
  if (!linkValue || !revocationSecret) return badRequest("invalid private contact link status request");
  const linkDigest = await sha256(linkValue);
  const revokeDigest = await sha256(revocationSecret);
  const now = serviceNow();
  await expireIfDue(env, linkDigest, now);
  const row = await env.DB.prepare(
    `SELECT issued_at_unix_seconds, expires_at_unix_seconds,
            terminal_state, terminal_at_unix_seconds
       FROM private_contact_links
      WHERE link_sha256 = ? AND revoke_sha256 = ?`,
  ).bind(linkDigest, revokeDigest).first<Omit<LinkRow, "contact_bundle">>();
  if (!row) return notFound("private contact link control unavailable");
  return json({
    issued_at_unix_seconds: row.issued_at_unix_seconds,
    expires_at_unix_seconds: row.expires_at_unix_seconds,
    terminal_state: row.terminal_state,
    terminal_at_unix_seconds: row.terminal_at_unix_seconds,
  });
}
