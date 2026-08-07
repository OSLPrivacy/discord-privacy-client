import { badRequest, json, notFound } from "./http.js";

const DRAWER_DOMAIN = "OSL-DISCOVERY-DRAWER-v1";
const LABEL_DOMAIN = "OSL-DISCOVERY-LABEL-v1";
const WEEK_MS = 7 * 24 * 60 * 60 * 1000;
const DRAWER_RE = /^[0-9a-f]{3}$/;
const LABEL_RE = /^[A-Za-z0-9_-]{22,64}$/;
const EPOCH_RE = /^(\d{4})-W(\d{2})$/;
const SEALED_NOTE_MAX_CHARS = 8192;

export interface DiscoveryCard {
  drawer_name: string;
  label: string;
  sealed_note: string;
  discovery_epoch: string;
}

export interface DiscoveryCardDerivationInput {
  app_id: string;
  account_handle: string;
  setting_material: string;
  sealed_note: string;
  discovery_epoch?: string;
}

interface DiscoveryCardRow {
  drawer_name: string;
  label: string;
  sealed_note: string;
  discovery_epoch: string;
}

function utf8(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function base64Url(bytes: Uint8Array): string {
  let raw = "";
  for (const b of bytes) raw += String.fromCharCode(b);
  return btoa(raw).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/u, "");
}

async function sha256Text(value: string): Promise<Uint8Array> {
  const bytes = utf8(value);
  const input = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  return new Uint8Array(await crypto.subtle.digest("SHA-256", input));
}

function hashInput(parts: readonly string[]): string {
  return JSON.stringify(parts);
}

export async function discoveryDrawerName(
  appId: string,
  accountHandle: string,
): Promise<string> {
  const digest = await sha256Text(hashInput([DRAWER_DOMAIN, appId, accountHandle]));
  return hex(digest).slice(0, 3);
}

export async function discoveryLabel(
  settingMaterial: string,
  discoveryEpoch: string,
): Promise<string> {
  const digest = await sha256Text(hashInput([LABEL_DOMAIN, settingMaterial, discoveryEpoch]));
  return base64Url(digest.slice(0, 16));
}

export async function buildDiscoveryCard(
  input: DiscoveryCardDerivationInput,
): Promise<DiscoveryCard> {
  const discovery_epoch = input.discovery_epoch ?? currentDiscoveryEpochStamp();
  return {
    drawer_name: await discoveryDrawerName(input.app_id, input.account_handle),
    label: await discoveryLabel(input.setting_material, discovery_epoch),
    sealed_note: input.sealed_note,
    discovery_epoch,
  };
}

function isoWeekParts(date: Date): { year: number; week: number } {
  const d = new Date(Date.UTC(
    date.getUTCFullYear(),
    date.getUTCMonth(),
    date.getUTCDate(),
  ));
  const day = d.getUTCDay() || 7;
  d.setUTCDate(d.getUTCDate() + 4 - day);
  const year = d.getUTCFullYear();
  const yearStart = new Date(Date.UTC(year, 0, 1));
  const week = Math.ceil((((d.getTime() - yearStart.getTime()) / 86_400_000) + 1) / 7);
  return { year, week };
}

export function currentDiscoveryEpochStamp(now = new Date()): string {
  const { year, week } = isoWeekParts(now);
  return `${year}-W${String(week).padStart(2, "0")}`;
}

function isoWeekStartMs(year: number, week: number): number | null {
  if (!Number.isSafeInteger(year) || !Number.isSafeInteger(week) || week < 1 || week > 53) {
    return null;
  }
  const jan4 = new Date(Date.UTC(year, 0, 4));
  const jan4Day = jan4.getUTCDay() || 7;
  const monday = new Date(jan4);
  monday.setUTCDate(jan4.getUTCDate() - jan4Day + 1 + ((week - 1) * 7));
  if (currentDiscoveryEpochStamp(monday) !== `${year}-W${String(week).padStart(2, "0")}`) {
    return null;
  }
  return monday.getTime();
}

export function discoveryEpochIndex(stamp: string): number | null {
  const match = EPOCH_RE.exec(stamp);
  if (!match) return null;
  const start = isoWeekStartMs(Number(match[1]), Number(match[2]));
  return start === null ? null : Math.floor(start / WEEK_MS);
}

export function addDiscoveryEpochWeeks(stamp: string, weeks: number): string {
  const match = EPOCH_RE.exec(stamp);
  if (!match || !Number.isSafeInteger(weeks)) {
    throw new Error("invalid discovery epoch");
  }
  const start = isoWeekStartMs(Number(match[1]), Number(match[2]));
  if (start === null) throw new Error("invalid discovery epoch");
  return currentDiscoveryEpochStamp(new Date(start + (weeks * WEEK_MS)));
}

function missingField(body: Record<string, unknown>): keyof DiscoveryCard | null {
  for (const field of ["drawer_name", "label", "sealed_note", "discovery_epoch"] as const) {
    if (!(field in body) || body[field] == null) return field;
  }
  return null;
}

function parseDiscoveryCard(body: unknown): DiscoveryCard | Response {
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    return badRequest("missing drawer_name");
  }
  const record = body as Record<string, unknown>;
  const missing = missingField(record);
  if (missing) return badRequest(`missing ${missing}`);

  const { drawer_name, label, sealed_note, discovery_epoch } = record;
  if (typeof drawer_name !== "string" || !DRAWER_RE.test(drawer_name)) {
    return badRequest("drawer_name invalid");
  }
  if (typeof label !== "string" || !LABEL_RE.test(label)) {
    return badRequest("label invalid");
  }
  if (
    typeof sealed_note !== "string" ||
    sealed_note.length === 0 ||
    sealed_note.length > SEALED_NOTE_MAX_CHARS
  ) {
    return badRequest("sealed_note invalid");
  }
  if (typeof discovery_epoch !== "string" || discoveryEpochIndex(discovery_epoch) === null) {
    return badRequest("discovery_epoch invalid");
  }
  return { drawer_name, label, sealed_note, discovery_epoch };
}

function validateDiscoveryEpoch(stamp: string, now = new Date()): Response | null {
  const current = discoveryEpochIndex(currentDiscoveryEpochStamp(now));
  const supplied = discoveryEpochIndex(stamp);
  if (current === null || supplied === null) return badRequest("discovery_epoch invalid");
  if (supplied < current - 1) return badRequest("stale discovery epoch");
  if (supplied > current) return badRequest("discovery_epoch is not current or previous");
  return null;
}

export async function sweepStaleDiscoveryCards(
  db: D1Database,
  now = new Date(),
): Promise<number> {
  const current = discoveryEpochIndex(currentDiscoveryEpochStamp(now));
  if (current === null) throw new Error("current discovery epoch invalid");
  const result = await db
    .prepare("DELETE FROM discovery_cards WHERE discovery_epoch_index < ?")
    .bind(current - 1)
    .run();
  return result.meta.changes ?? 0;
}

export async function handleDiscoveryCardPost(
  request: Request,
  db: D1Database,
  now = new Date(),
): Promise<Response> {
  const parsed = parseDiscoveryCard(await request.json().catch(() => null));
  if (parsed instanceof Response) return parsed;
  const epochError = validateDiscoveryEpoch(parsed.discovery_epoch, now);
  if (epochError) return epochError;
  const epochIndex = discoveryEpochIndex(parsed.discovery_epoch);
  if (epochIndex === null) return badRequest("discovery_epoch invalid");

  await sweepStaleDiscoveryCards(db, now);
  await db
    .prepare(
      `INSERT INTO discovery_cards
         (drawer_name, label, sealed_note, discovery_epoch, discovery_epoch_index, updated_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6)
       ON CONFLICT(drawer_name, label) DO UPDATE SET
         sealed_note = excluded.sealed_note,
         discovery_epoch = excluded.discovery_epoch,
         discovery_epoch_index = excluded.discovery_epoch_index,
         updated_at = excluded.updated_at`,
    )
    .bind(
      parsed.drawer_name,
      parsed.label,
      parsed.sealed_note,
      parsed.discovery_epoch,
      epochIndex,
      Math.floor(now.getTime() / 1000),
    )
    .run();
  return json(parsed, { status: 201 });
}

export async function handleDiscoveryCardRead(
  request: Request,
  db: D1Database,
  now = new Date(),
): Promise<Response> {
  const body = await request.json().catch(() => null);
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    return badRequest("missing drawer_name");
  }
  const record = body as Record<string, unknown>;
  if (!("drawer_name" in record) || record.drawer_name == null) return badRequest("missing drawer_name");
  if (!("label" in record) || record.label == null) return badRequest("missing label");
  if (typeof record.drawer_name !== "string" || !DRAWER_RE.test(record.drawer_name)) {
    return badRequest("drawer_name invalid");
  }
  if (typeof record.label !== "string" || !LABEL_RE.test(record.label)) {
    return badRequest("label invalid");
  }

  await sweepStaleDiscoveryCards(db, now);
  const row = await db
    .prepare(
      `SELECT drawer_name, label, sealed_note, discovery_epoch
         FROM discovery_cards
        WHERE drawer_name = ? AND label = ?`,
    )
    .bind(record.drawer_name, record.label)
    .first<DiscoveryCardRow>();
  if (!row) return notFound("discovery card not found");
  return json(row);
}
