export const OSL_CHAT_NOTIFICATION_SETTINGS_KEY = "osl-chat-per-chat-notifications-v1";

export type OslChatNotificationSwitch =
  | "notifications"
  | "previews"
  | "typingIndicator"
  | "readReceipts"
  | "dontShowImTyping"
  | "dontShowWhenOnline";

export interface OslChatNotificationSettings {
  readonly notifications: boolean;
  readonly previews: boolean;
  readonly typingIndicator: boolean;
  readonly readReceipts: boolean;
  readonly dontShowImTyping: boolean;
  readonly dontShowWhenOnline: boolean;
}

export interface OslChatReadReceiptFixture {
  readonly send: boolean;
  readonly receive: boolean;
}

export interface OslChatNotificationPreview {
  readonly senderName: string;
  readonly messagePreview: string;
  readonly messageWordCount: number;
  readonly title: string;
}

export interface OslChatSwitchBehaviour {
  readonly notificationCreated: boolean;
  readonly previewWordCount: number;
  readonly incomingTypingVisible: boolean;
  readonly readReceipts: { readonly send: boolean; readonly receive: boolean };
  readonly outgoingTypingVisible: boolean;
  readonly outgoingOnlineVisible: boolean;
}

type SettingsStorage = Pick<Storage, "getItem" | "setItem">;

const defaults: OslChatNotificationSettings = Object.freeze({
  notifications: true,
  previews: true,
  typingIndicator: true,
  // Receipts stay off unless both directions are explicitly enabled together.
  readReceipts: false,
  dontShowImTyping: false,
  dontShowWhenOnline: false,
});

const switches: readonly OslChatNotificationSwitch[] = [
  "notifications",
  "previews",
  "typingIndicator",
  "readReceipts",
  "dontShowImTyping",
  "dontShowWhenOnline",
];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isSettings(value: unknown): value is OslChatNotificationSettings {
  if (!isRecord(value)) return false;
  return switches.every((key) => typeof value[key] === "boolean");
}

function parseSettingsMap(raw: string | null): Record<string, OslChatNotificationSettings> {
  if (raw === null) return {};
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!isRecord(parsed)) return {};
    return Object.fromEntries(Object.entries(parsed)
      .filter(([personId, value]) => personId.length > 0 && personId.length <= 180 && isSettings(value))
      .slice(0, 512)) as Record<string, OslChatNotificationSettings>;
  } catch {
    return {};
  }
}

export function defaultOslChatNotificationSettings(): OslChatNotificationSettings {
  return { ...defaults };
}

export function readOslChatNotificationSettings(
  storage: Pick<Storage, "getItem">,
  personId: string,
): OslChatNotificationSettings {
  const value = parseSettingsMap(storage.getItem(OSL_CHAT_NOTIFICATION_SETTINGS_KEY))[personId];
  return value ? { ...value } : defaultOslChatNotificationSettings();
}

export function saveOslChatNotificationSettings(
  storage: SettingsStorage,
  personId: string,
  settings: OslChatNotificationSettings,
): boolean {
  if (personId.length === 0 || personId.length > 180 || !isSettings(settings)) return false;
  const map = parseSettingsMap(storage.getItem(OSL_CHAT_NOTIFICATION_SETTINGS_KEY));
  map[personId] = { ...settings };
  storage.setItem(OSL_CHAT_NOTIFICATION_SETTINGS_KEY, JSON.stringify(map));
  return true;
}

export function setOslChatNotificationSwitch(
  storage: SettingsStorage,
  personId: string,
  key: OslChatNotificationSwitch,
  enabled: boolean,
): OslChatNotificationSettings {
  const current = readOslChatNotificationSettings(storage, personId);
  const next = { ...current, [key]: enabled };
  saveOslChatNotificationSettings(storage, personId, next);
  return next;
}

/**
 * A wire/import fixture may describe the two receipt directions separately,
 * but the UI deliberately cannot. Refuse asymmetric input instead of silently
 * turning on one-way receipts.
 */
export function settingsFromReadReceiptFixture(
  fixture: OslChatReadReceiptFixture,
  base: OslChatNotificationSettings = defaults,
): OslChatNotificationSettings | null {
  if (fixture.send !== fixture.receive) return null;
  return { ...base, readReceipts: fixture.send };
}

function countWords(value: string): number {
  return value.trim() === "" ? 0 : value.trim().split(/\s+/u).length;
}

export function oslChatNotificationPreview(
  settings: OslChatNotificationSettings,
  senderName: string,
  message: string,
): OslChatNotificationPreview | null {
  if (!settings.notifications) return null;
  const messagePreview = settings.previews ? message : "";
  return {
    senderName,
    messagePreview,
    messageWordCount: countWords(messagePreview),
    title: senderName,
  };
}

/** One independently comparable outcome for each of the six switches. */
export function oslChatSwitchBehaviour(
  settings: OslChatNotificationSettings,
  previewFixture = "one two three",
): OslChatSwitchBehaviour {
  return {
    notificationCreated: settings.notifications,
    previewWordCount: settings.previews ? countWords(previewFixture) : 0,
    incomingTypingVisible: settings.typingIndicator,
    readReceipts: { send: settings.readReceipts, receive: settings.readReceipts },
    outgoingTypingVisible: !settings.dontShowImTyping,
    outgoingOnlineVisible: !settings.dontShowWhenOnline,
  };
}

const labels: ReadonlyArray<readonly [OslChatNotificationSwitch, string, string]> = [
  ["notifications", "Notifications", "Show alerts for new messages from this chat."],
  ["previews", "Previews", "Include message words in this chat's alerts."],
  ["typingIndicator", "Typing indicator", "Show when this person is typing."],
  ["readReceipts", "Read receipts", "Send and receive read receipts together, or neither."],
  ["dontShowImTyping", "Don't show I'm typing", "Do not send my typing state to this person."],
  ["dontShowWhenOnline", "Don't show when I'm online", "Do not send my online state to this person."],
];

export function oslChatNotificationSettingsMarkup(settings: OslChatNotificationSettings): string {
  const rows = labels.map(([key, label, help]) => `<label class="setting-line interactive"><span><strong>${label}</strong><small>${help}</small></span><input data-osl-chat-notification-switch="${key}" type="checkbox" ${settings[key] ? "checked" : ""}/></label>`).join("");
  return `<section class="osl-chat-notifications-block" aria-label="NOTIFICATIONS"><h3>NOTIFICATIONS</h3>${rows}</section>`;
}
