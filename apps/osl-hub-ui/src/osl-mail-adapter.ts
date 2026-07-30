import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";

const OSL_MAIL_ADDRESS = /^[a-z0-9](?:[a-z0-9._-]{0,30}[a-z0-9])?@oslprivacy\.com$/u;
const OSL_MAIL_USERNAME = /^[a-z0-9](?:[a-z0-9._-]{0,30}[a-z0-9])?$/u;

export type OslMailAddress = `${string}@oslprivacy.com`;
export type OslMailUnreadCount = number;
export type OslMailRetentionSeconds = number;

export const OSL_MAIL_STATUS_CONTRACT = {
  maximumUnreadCount: 100_000,
  minimumRetentionSeconds: 60,
  maximumRetentionSeconds: 7 * 24 * 60 * 60,
} as const;

export type OslMailStatus = OslMailProvisionedStatus | OslMailUnprovisionedStatus;

export interface OslMailProvisionedStatus {
  readonly available: true;
  readonly provisioned: true;
  readonly address: OslMailAddress;
  readonly unreadCount: OslMailUnreadCount;
  readonly retentionSeconds: OslMailRetentionSeconds;
}

export interface OslMailUnprovisionedStatus {
  readonly available: true;
  readonly provisioned: false;
  readonly address: null;
  readonly unreadCount: 0;
  readonly retentionSeconds: OslMailRetentionSeconds;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exact(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function retentionSeconds(value: unknown): value is OslMailRetentionSeconds {
  return Number.isSafeInteger(value)
    && Number(value) >= OSL_MAIL_STATUS_CONTRACT.minimumRetentionSeconds
    && Number(value) <= OSL_MAIL_STATUS_CONTRACT.maximumRetentionSeconds;
}

function unreadCount(value: unknown): value is OslMailUnreadCount {
  return Number.isSafeInteger(value)
    && Number(value) >= 0
    && Number(value) <= OSL_MAIL_STATUS_CONTRACT.maximumUnreadCount;
}

export function parseOslMailStatus(value: unknown): OslMailStatus | null {
  if (!record(value)
    || !exact(value, ["available", "provisioned", "address", "unreadCount", "retentionSeconds"])
    || value.available !== true
    || typeof value.provisioned !== "boolean"
    || !(value.address === null || (typeof value.address === "string" && OSL_MAIL_ADDRESS.test(value.address)))
    || !unreadCount(value.unreadCount)
    || !retentionSeconds(value.retentionSeconds)) return null;

  if (value.provisioned) {
    return value.address === null ? null : value as unknown as OslMailProvisionedStatus;
  }
  return value.address === null && value.unreadCount === 0
    ? value as unknown as OslMailUnprovisionedStatus
    : null;
}

async function statusCall(command: string, args: Record<string, unknown>): Promise<OslMailStatus | null> {
  if (!isTauriRuntime()) return null;
  try {
    return parseOslMailStatus(await invoke<unknown>(command, args));
  } catch {
    return null;
  }
}

export const loadOslMailStatus = (): Promise<OslMailStatus | null> => statusCall("osl_mail_get_status", {});

export const provisionOslMail = (username: string): Promise<OslMailStatus | null> => OSL_MAIL_USERNAME.test(username)
  ? statusCall("osl_mail_provision", { username })
  : Promise.resolve(null);

type OslMailStatusTestApi = {
  describe: typeof import("vitest").describe;
  expect: typeof import("vitest").expect;
  expectTypeOf: typeof import("vitest").expectTypeOf;
  it: typeof import("vitest").it;
};

export function registerOslMailStatusAdapterTests({ describe, expect, expectTypeOf, it }: OslMailStatusTestApi): void {
  describe("OSL Mail status adapter", () => {
    it("publishes the strict status DTO contract", () => {
      expect(OSL_MAIL_STATUS_CONTRACT).toEqual({
        maximumUnreadCount: 100_000,
        minimumRetentionSeconds: 60,
        maximumRetentionSeconds: 604_800,
      });
      expectTypeOf<OslMailStatus>().toEqualTypeOf<OslMailProvisionedStatus | OslMailUnprovisionedStatus>();
      expectTypeOf<OslMailProvisionedStatus["address"]>().toEqualTypeOf<OslMailAddress>();
      expectTypeOf<OslMailUnprovisionedStatus["address"]>().toEqualTypeOf<null>();
      expectTypeOf<OslMailUnprovisionedStatus["unreadCount"]>().toEqualTypeOf<0>();
    });

    it("accepts only exact provisioned status responses", () => {
      expect(parseOslMailStatus({
        available: true,
        provisioned: true,
        address: "member@oslprivacy.com",
        unreadCount: 2,
        retentionSeconds: 3_600,
      })).toMatchObject({ provisioned: true, address: "member@oslprivacy.com" });
      expect(parseOslMailStatus({
        available: true,
        provisioned: true,
        address: null,
        unreadCount: 0,
        retentionSeconds: 3_600,
      })).toBeNull();
      expect(parseOslMailStatus({
        available: true,
        provisioned: true,
        address: "member@oslprivacy.com",
        unreadCount: OSL_MAIL_STATUS_CONTRACT.maximumUnreadCount,
        retentionSeconds: OSL_MAIL_STATUS_CONTRACT.maximumRetentionSeconds,
      })).not.toBeNull();
      expect(parseOslMailStatus({
        available: true,
        provisioned: true,
        address: "member@oslprivacy.com",
        unreadCount: OSL_MAIL_STATUS_CONTRACT.maximumUnreadCount + 1,
        retentionSeconds: 3_600,
      })).toBeNull();
      expect(parseOslMailStatus({
        available: true,
        provisioned: true,
        address: "member@oslprivacy.com",
        unreadCount: 0,
        retentionSeconds: OSL_MAIL_STATUS_CONTRACT.minimumRetentionSeconds - 1,
      })).toBeNull();
      expect(parseOslMailStatus({
        available: true,
        provisioned: true,
        address: "member@oslprivacy.com",
        unreadCount: 0,
        retentionSeconds: 3_600,
        extra: true,
      })).toBeNull();
    });

    it("models unprovisioned status as no mailbox and no unread mail", () => {
      expect(parseOslMailStatus({
        available: true,
        provisioned: false,
        address: null,
        unreadCount: 0,
        retentionSeconds: 3_600,
      })).toMatchObject({ provisioned: false, address: null, unreadCount: 0 });
      expect(parseOslMailStatus({
        available: true,
        provisioned: false,
        address: "member@oslprivacy.com",
        unreadCount: 0,
        retentionSeconds: 3_600,
      })).toBeNull();
      expect(parseOslMailStatus({
        available: true,
        provisioned: false,
        address: null,
        unreadCount: 1,
        retentionSeconds: 3_600,
      })).toBeNull();
    });

    it("refuses unavailable, malformed, or renderer-invented status", async () => {
      expect(parseOslMailStatus({
        available: false,
        provisioned: false,
        address: null,
        unreadCount: 0,
        retentionSeconds: 3_600,
      })).toBeNull();
      expect(parseOslMailStatus({
        available: true,
        provisioned: true,
        address: "Liam@oslprivacy.com",
        unreadCount: 0,
        retentionSeconds: 3_600,
      })).toBeNull();

      await expect(provisionOslMail("../member")).resolves.toBeNull();
    });
  });
}
