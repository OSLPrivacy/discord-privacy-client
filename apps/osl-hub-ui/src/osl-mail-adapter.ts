import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";

const ADDRESS = /^[a-z0-9](?:[a-z0-9._-]{0,30}[a-z0-9])?@oslprivacy\.com$/u;
const ID = /^[A-Za-z0-9_-]{12,160}$/u;
const RECEIPT = /^[a-f0-9]{64}$/u;
const MAX_BODY_BYTES = 256 * 1024;

export type OslMailTransit = "oslE2ee" | "externalSmtp";
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
export interface OslMailThreadSummary {
  threadId: string;
  subject: string;
  correspondent: string;
  latestAt: number;
  unread: boolean;
  transit: OslMailTransit;
}
export interface OslMailThreadMessage {
  messageId: string;
  from: string;
  to: string[];
  subject: string;
  body: string;
  receivedAt: number;
  transit: OslMailTransit;
}
export interface OslMailRetrievedThread {
  threadId: string;
  retrievalId: string;
  expiresAt: number;
  messages: OslMailThreadMessage[];
}
export interface OslMailDeleteReceipt {
  retrievalId: string;
  deletedMessageIds: string[];
  deletedAt: number;
  receiptSha256: string;
  serverDeleteConfirmed: true;
}
export interface OslMailSendReceipt {
  clientMessageId: string;
  acceptedAt: number;
  recipient: string;
  transit: OslMailTransit;
  receiptSha256: string;
}
export interface OslMailBurnReceipt {
  address: string;
  burnedAt: number;
  deletedMessages: number;
  receiptSha256: string;
  mailboxDisabled: true;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function exact(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}
function text(value: unknown, maximum: number, allowEmpty = false): value is string {
  if (typeof value !== "string" || (!allowEmpty && !value.trim()) || new TextEncoder().encode(value).length > maximum) return false;
  return !/[\u0000\u007f]/u.test(value);
}
function timestamp(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) > 0;
}
function transit(value: unknown): value is OslMailTransit {
  return value === "oslE2ee" || value === "externalSmtp";
}
function address(value: unknown): value is string {
  return typeof value === "string" && value.length <= 254 && /^[^\s@]+@[^\s@]+$/u.test(value);
}

export function parseOslMailStatus(value: unknown): OslMailStatus | null {
  if (!record(value) || !exact(value, ["available", "provisioned", "address", "unreadCount", "retentionSeconds"])
    || value.available !== true || typeof value.provisioned !== "boolean"
    || !(value.address === null || (typeof value.address === "string" && ADDRESS.test(value.address)))
    || !Number.isSafeInteger(value.unreadCount) || Number(value.unreadCount) < 0 || Number(value.unreadCount) > OSL_MAIL_STATUS_CONTRACT.maximumUnreadCount
    || !Number.isSafeInteger(value.retentionSeconds) || Number(value.retentionSeconds) < OSL_MAIL_STATUS_CONTRACT.minimumRetentionSeconds || Number(value.retentionSeconds) > OSL_MAIL_STATUS_CONTRACT.maximumRetentionSeconds
    || (value.provisioned ? value.address === null : value.address !== null || value.unreadCount !== 0)) return null;
  return value as unknown as OslMailStatus;
}

export function parseOslMailThreadSummary(value: unknown): OslMailThreadSummary | null {
  if (!record(value) || !exact(value, ["threadId", "subject", "correspondent", "latestAt", "unread", "transit"])
    || typeof value.threadId !== "string" || !ID.test(value.threadId) || !text(value.subject, 512, true)
    || !address(value.correspondent) || !timestamp(value.latestAt) || typeof value.unread !== "boolean" || !transit(value.transit)) return null;
  return value as unknown as OslMailThreadSummary;
}

function parseMessage(value: unknown): OslMailThreadMessage | null {
  if (!record(value) || !exact(value, ["messageId", "from", "to", "subject", "body", "receivedAt", "transit"])
    || typeof value.messageId !== "string" || !ID.test(value.messageId) || !address(value.from)
    || !Array.isArray(value.to) || value.to.length < 1 || value.to.length > 100 || !value.to.every(address)
    || !text(value.subject, 512, true) || !text(value.body, MAX_BODY_BYTES, true)
    || !timestamp(value.receivedAt) || !transit(value.transit)) return null;
  return value as unknown as OslMailThreadMessage;
}

export function parseOslMailRetrievedThread(value: unknown): OslMailRetrievedThread | null {
  if (!record(value) || !exact(value, ["threadId", "retrievalId", "expiresAt", "messages"])
    || typeof value.threadId !== "string" || !ID.test(value.threadId)
    || typeof value.retrievalId !== "string" || !ID.test(value.retrievalId)
    || !timestamp(value.expiresAt) || !Array.isArray(value.messages) || value.messages.length < 1 || value.messages.length > 200) return null;
  const messages = value.messages.map(parseMessage);
  return messages.some((message) => message === null) ? null : { ...value, messages } as OslMailRetrievedThread;
}

export function parseOslMailDeleteReceipt(value: unknown): OslMailDeleteReceipt | null {
  if (!record(value) || !exact(value, ["retrievalId", "deletedMessageIds", "deletedAt", "receiptSha256", "serverDeleteConfirmed"])
    || typeof value.retrievalId !== "string" || !ID.test(value.retrievalId) || !Array.isArray(value.deletedMessageIds)
    || value.deletedMessageIds.length > 200 || !value.deletedMessageIds.every((id) => typeof id === "string" && ID.test(id))
    || !timestamp(value.deletedAt) || typeof value.receiptSha256 !== "string" || !RECEIPT.test(value.receiptSha256)
    || value.serverDeleteConfirmed !== true) return null;
  return value as unknown as OslMailDeleteReceipt;
}

export function parseOslMailSendReceipt(value: unknown): OslMailSendReceipt | null {
  if (!record(value) || !exact(value, ["clientMessageId", "acceptedAt", "recipient", "transit", "receiptSha256"])
    || typeof value.clientMessageId !== "string" || !ID.test(value.clientMessageId) || !timestamp(value.acceptedAt)
    || typeof value.recipient !== "string" || !ADDRESS.test(value.recipient) || value.transit !== "oslE2ee"
    || typeof value.receiptSha256 !== "string" || !RECEIPT.test(value.receiptSha256)) return null;
  return value as unknown as OslMailSendReceipt;
}

export function parseOslMailBurnReceipt(value: unknown): OslMailBurnReceipt | null {
  if (!record(value) || !exact(value, ["address", "burnedAt", "deletedMessages", "receiptSha256", "mailboxDisabled"])
    || typeof value.address !== "string" || !ADDRESS.test(value.address) || !timestamp(value.burnedAt)
    || !Number.isSafeInteger(value.deletedMessages) || Number(value.deletedMessages) < 0
    || typeof value.receiptSha256 !== "string" || !RECEIPT.test(value.receiptSha256) || value.mailboxDisabled !== true) return null;
  return value as unknown as OslMailBurnReceipt;
}

async function call<T>(command: string, args: Record<string, unknown>, parser: (value: unknown) => T | null): Promise<T | null> {
  if (!isTauriRuntime()) return null;
  try { return parser(await invoke<unknown>(command, args)); } catch { return null; }
}

export const loadOslMailStatus = (): Promise<OslMailStatus | null> => call("osl_mail_get_status", {}, parseOslMailStatus);
export const provisionOslMail = (username: string): Promise<OslMailStatus | null> => /^[a-z0-9](?:[a-z0-9._-]{0,30}[a-z0-9])?$/u.test(username)
  ? call("osl_mail_provision", { username }, parseOslMailStatus) : Promise.resolve(null);
export async function listOslMailThreads(): Promise<OslMailThreadSummary[] | null> {
  if (!isTauriRuntime()) return null;
  try {
    const value = await invoke<unknown>("osl_mail_list_threads");
    if (!Array.isArray(value) || value.length > 500) return null;
    const rows = value.map(parseOslMailThreadSummary);
    return rows.some((row) => row === null) ? null : rows as OslMailThreadSummary[];
  } catch { return null; }
}
export const retrieveOslMailThread = (threadId: string): Promise<OslMailRetrievedThread | null> => ID.test(threadId)
  ? call("osl_mail_retrieve_thread", { threadId }, parseOslMailRetrievedThread) : Promise.resolve(null);
export const acknowledgeOslMailRetrieval = (retrievalId: string, messageIds: string[]): Promise<OslMailDeleteReceipt | null> => ID.test(retrievalId)
  && messageIds.length > 0 && messageIds.length <= 200 && messageIds.every((id) => ID.test(id))
  ? call("osl_mail_acknowledge_retrieval", { retrievalId, messageIds }, parseOslMailDeleteReceipt) : Promise.resolve(null);
export async function sendOslMail(recipient: string, subject: string, body: string): Promise<OslMailSendReceipt | null> {
  if (!ADDRESS.test(recipient) || !text(subject, 512, true) || !text(body, MAX_BODY_BYTES)) return null;
  const receipt = await call("osl_mail_send", { recipient, subject, body }, parseOslMailSendReceipt);
  return receipt?.transit === "oslE2ee" ? receipt : null;
}
export const burnOslMailbox = (addressValue: string, confirmation: string): Promise<OslMailBurnReceipt | null> => ADDRESS.test(addressValue)
  && confirmation === addressValue ? call("osl_mail_burn", { address: addressValue, confirmation }, parseOslMailBurnReceipt) : Promise.resolve(null);
