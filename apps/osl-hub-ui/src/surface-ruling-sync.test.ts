import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type Ruling = {
  chat_carriers: string[];
  email_carriers: string[];
  native_email_carriers: string[];
  first_party_surfaces: string[];
  mailbox_reading_ruling: MailboxReadingRuling;
  owner_rulings?: OwnerRuling[];
  cut_surfaces: string[];
};

type MailboxReadingRuling = {
  task: string;
  ruled_on: string;
  ruled_by: string;
  owner_words: string;
  outside_services_correction: string;
};

type OwnerRuling = {
  ruled_on: string;
  ruled_by: string;
  words: string;
  surfaces: string[];
};

const root = new URL("../../..", import.meta.url);
const ruling = JSON.parse(readFileSync(new URL("data/surface-ruling-2026-08-05.json", root), "utf8")) as Ruling;

const sorted = (values: readonly string[]) => [...values].sort();
const unique = (values: readonly string[]) => sorted([...new Set(values)]);
const serviceIds = unique([...ruling.chat_carriers, "email"]);
const homeAppIds = unique([...ruling.chat_carriers, ...ruling.email_carriers]);
const nativeAppIds = unique([...ruling.chat_carriers, ...ruling.native_email_carriers]);
const ownerRestoredChatApps = unique(ruling.owner_rulings?.flatMap((entry) => entry.surfaces) ?? []);
const nonSignalMessagingRiskServiceIds = unique([...ruling.chat_carriers.filter((id) => id !== "signal"), "email"]);
const legacyTimerSurfaceIds = unique([...ruling.cut_surfaces, ...ownerRestoredChatApps]);
const adapterProfileSurfaceIds = unique([
  ...ruling.chat_carriers,
  ...ownerRestoredChatApps,
  ...ruling.email_carriers.filter((id) => id !== "icloud"),
  ...ruling.cut_surfaces.filter((id) => id === "snapchat"),
]);
const tileColourSurfaceIds = unique([
  ...serviceIds,
  ...ownerRestoredChatApps,
  ...ruling.email_carriers.filter((id) => id !== "tuta"),
  ...ruling.cut_surfaces.filter((id) => id === "snapchat"),
]);

type CheckedList = {
  name: string;
  actual: readonly string[];
  expected: readonly string[];
};

const REQUIRED_TASK_4254_LISTS = [
  "ruling chat carriers",
  "ruling email carriers",
  "ruling native_email_carriers",
  "ruling first-party non-carriers",
  "ruling cut_surfaces",
  "Rust ServiceKind enum",
  "Rust service_kind_from_id",
  "Rust service_descriptors",
  "Rust EmailProvider enum",
  "Rust NativeAppId enum",
  "Rust NATIVE_APPS manifest",
  "Rust FirefoxServiceId enum",
  "Rust FIREFOX_SERVICES allowlist",
  "Rust service_host SERVICES",
  "Rust mass_cleanup manifest",
  "TS ServiceId union",
  "TS EmailProvider union",
  "TS OfferedEmailProvider union",
  "TS NativeAppId union",
  "TS services.ts serviceIds",
  "TS services.ts emailProviders",
  "TS services.ts firefoxServiceIds",
  "TS homeAppDefinitions",
  "TS autoscrub serviceIds",
  "TS service-guide serviceIds",
  "TS browserServiceQaIds",
  "TS desktopServicePolicies",
  "TS nativePreviewApps fallback",
  "TS previewRegistry fallback",
  "Rust CHAT_APP_TIMER_POLICIES",
  "Rust MESSAGING_RISK_FACT_ROWS",
  "Rust AdapterService enum",
] as const;

function assertSameSet(name: string, actual: readonly string[], expected: readonly string[]): void {
  const actualSet = unique(actual);
  const expectedSet = unique(expected);
  const missing = expectedSet.filter((id) => !actualSet.includes(id));
  const extra = actualSet.filter((id) => !expectedSet.includes(id));
  expect(
    { actual: actualSet, expected: expectedSet, missing, extra },
    `surface enumeration drift in ${name}: missing from ${name} vs ruling=${missing.join(",") || "(none)"}; extra in ${name} not in ruling=${extra.join(",") || "(none)"}`,
  ).toEqual({ actual: expectedSet, expected: expectedSet, missing: [], extra: [] });
}

function source(path: string): string {
  return readFileSync(new URL(path, root), "utf8");
}

function quotedValues(text: string): string[] {
  return [...text.matchAll(/"([a-z][a-z0-9-]*)"/g)].map((match) => match[1]);
}

function tsUnion(text: string, name: string): string[] {
  const match = new RegExp(`export type ${name} = ([^;]+);`, "u").exec(text);
  if (!match) throw new Error(`missing TS union ${name}`);
  return quotedValues(match[1]);
}

function tsConstArray(text: string, name: string): string[] {
  const match = new RegExp(`(?:export )?const ${name}(?:: readonly [^=]+)? = \\[([\\s\\S]*?)\\](?: as const)?;`, "u").exec(text);
  if (!match) throw new Error(`missing TS array ${name}`);
  return quotedValues(match[1]);
}

function tsConstSet(text: string, name: string): string[] {
  const match = new RegExp(`const ${name} = new Set<[^>]+>\\(\\[([\\s\\S]*?)\\]\\);`, "u").exec(text);
  if (!match) throw new Error(`missing TS set ${name}`);
  return quotedValues(match[1]);
}

function tsConstArrayBody(text: string, name: string): string {
  const match = new RegExp(`const ${name}(?:: [^=]+)? = \\[([\\s\\S]*?)\\];`, "u").exec(text);
  if (!match) throw new Error(`missing TS array ${name}`);
  return match[1];
}

function tsConstObjectIds(text: string, name: string): string[] {
  return [...tsConstArrayBody(text, name).matchAll(/\{\s*id:\s*"([a-z][a-z0-9-]*)"/g)].map((match) => match[1]);
}

function tsConstServiceCallIds(text: string, name: string): string[] {
  return [...tsConstArrayBody(text, name).matchAll(/\bservice\("([a-z][a-z0-9-]*)"/g)].map((match) => match[1]);
}

function rustEnum(text: string, name: string): string[] {
  const match = new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`, "u").exec(text);
  if (!match) throw new Error(`missing Rust enum ${name}`);
  return [...match[1].matchAll(/^\s*([A-Z][A-Za-z0-9]*)\b/gm)].map((entry) => rustVariantId(entry[1]));
}

function rustVariantId(variant: string): string {
  return variant
    .replace("WhatsApp", "Whatsapp")
    .replace("MailCom", "Maildotcom")
    .replace("Maildotcom", "Maildotcom")
    .replace(/[A-Z]/g, (letter, offset) => `${offset === 0 ? "" : "-"}${letter.toLowerCase()}`)
    .replace("whats-app", "whatsapp")
    .replace("maildotcom", "maildotcom");
}

function rustFunctionBody(text: string, name: string): string {
  const start = text.indexOf(`fn ${name}`);
  if (start < 0) throw new Error(`missing Rust function ${name}`);
  const open = text.indexOf("{", start);
  let depth = 0;
  for (let index = open; index < text.length; index += 1) {
    if (text[index] === "{") depth += 1;
    if (text[index] === "}") depth -= 1;
    if (depth === 0) return text.slice(open + 1, index);
  }
  throw new Error(`unterminated Rust function ${name}`);
}

function rustServiceKindRefs(text: string): string[] {
  return [...text.matchAll(/ServiceKind::([A-Z][A-Za-z0-9]*)/g)].map((match) => rustVariantId(match[1]));
}

function rustServiceAliasRefs(text: string): string[] {
  return [...text.matchAll(/Service::([A-Z][A-Za-z0-9]*)/g)].map((match) => rustVariantId(match[1]));
}

function rustNativeAppRefs(text: string): string[] {
  return [...text.matchAll(/NativeAppId::([A-Z][A-Za-z0-9]*)/g)].map((match) => rustVariantId(match[1]));
}

function rustConstArrayBody(text: string, name: string): string {
  const start = Math.max(text.indexOf(`pub const ${name}`), text.indexOf(`const ${name}`));
  if (start < 0) throw new Error(`missing Rust const array ${name}`);
  const assignment = text.indexOf("= [", start);
  if (assignment < 0) throw new Error(`missing Rust const array initializer ${name}`);
  const open = text.indexOf("[", assignment);
  let depth = 0;
  for (let index = open; index < text.length; index += 1) {
    if (text[index] === "[") depth += 1;
    if (text[index] === "]") depth -= 1;
    if (depth === 0) return text.slice(open + 1, index);
  }
  throw new Error(`unterminated Rust const array ${name}`);
}

function rustStructFieldValues(text: string, name: string, field: string): string[] {
  const body = rustConstArrayBody(text, name);
  return [...body.matchAll(new RegExp(`${field}:\\s*"([a-z][a-z0-9-]*)"`, "g"))].map((match) => match[1]);
}

function rustCallFirstArgs(text: string, name: string, callee: string): string[] {
  const body = rustConstArrayBody(text, name);
  return [...body.matchAll(new RegExp(`${callee}\\("([a-z][a-z0-9-]*)"`, "g"))].map((match) => match[1]);
}

function cssTileColourIds(text: string): string[] {
  return unique([
    ...[...text.matchAll(/\.app-tile\[data-service-kind="([a-z][a-z0-9-]*)"\][^{]*\{[^}]*--service:/g)].map((match) => match[1]),
    ...[...text.matchAll(/data-home-app="([a-z][a-z0-9-]*)"[^{]*\{[^}]*--service:/g)].map((match) => match[1]),
  ]);
}

function assertText(path: string, actual: string, expected: string): void {
  if (actual !== expected) {
    throw new Error(`TASK4086_MAILBOX_RULING_WORDING ${path}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function assertNoTask4254SkippedLists(checkedLists: readonly CheckedList[]): void {
  const names = checkedLists.map((entry) => entry.name);
  const skipped = REQUIRED_TASK_4254_LISTS.filter((name) => !names.includes(name));
  console.log(`TASK4254_SERVICE_LISTS_CHECKED=${checkedLists.length}`);
  console.log(`TASK4254_SERVICE_LISTS_SKIPPED=${skipped.length}`);
  console.log(`TASK4254_SERVICE_LIST_INSIDE_CODE_CHECKED=${names.includes("Rust MESSAGING_RISK_FACT_ROWS")}`);
  expect(
    skipped,
    `TASK4254 skipped service lists: ${skipped.join(",") || "(none)"}`,
  ).toEqual([]);
  expect(checkedLists, "TASK4254 must read the 32 service lists counted by 4250").toHaveLength(REQUIRED_TASK_4254_LISTS.length);
}

describe("surface ruling synchronization", () => {
  it("keeps every executable enumeration reconciled to the 2026-08-05 ruling", () => {
    const servicesTs = source("apps/osl-hub-ui/src/services.ts");
    const modelsRs = source("apps/osl-hub/src/models.rs");
    const servicesRs = source("apps/osl-hub/src/services.rs");
    const nativeAppsRs = source("apps/osl-hub/src/native_apps.rs");
    const chatAppTimerRs = source("apps/osl-hub/src/chat_app_timer_policy.rs");
    const serviceHostRs = source("apps/osl-hub/src/service_host.rs");
    const massCleanupRs = source("apps/osl-hub/src/mass_cleanup.rs");
    const autoscrubTs = source("apps/osl-hub-ui/src/autoscrub-contract.ts");
    const serviceGuideTs = source("apps/osl-hub-ui/src/service-guide.ts");
    const browserQaTs = source("apps/osl-hub-ui/src/browser-service-qa-shell.ts");
    const desktopPolicyTs = source("apps/osl-hub-ui/src/desktop-service-policy.ts");
    const adapterProfileRs = source("crates/adapter-profile/src/schema.rs");
    const stylesCss = source("apps/osl-hub-ui/src/styles.css");
    const servicesTestTs = source("apps/osl-hub-ui/src/services.test.ts");
    const massCleanupTestTs = source("apps/osl-hub-ui/src/mass-cleanup.test.ts");
    const supportMatrix = JSON.parse(source("docs/status/support-matrix.json")) as { surface_ruling: Ruling };
    const pricing = JSON.parse(source("data/pricing.json")) as { surface_policy: { surface_ruling: Ruling } };

    const checkedLists: CheckedList[] = [
      { name: "ruling chat carriers", actual: ruling.chat_carriers, expected: ["discord", "signal", "whatsapp", "telegram"] },
      { name: "ruling email carriers", actual: ruling.email_carriers, expected: ["gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud", "tuta"] },
      { name: "ruling native_email_carriers", actual: ruling.native_email_carriers, expected: ["outlook"] },
      { name: "ruling first-party non-carriers", actual: ruling.first_party_surfaces, expected: ["osl-chats", "osl-mail"] },
      { name: "ruling cut_surfaces", actual: ruling.cut_surfaces, expected: legacyTimerSurfaceIds },
      { name: "Rust ServiceKind enum", actual: rustEnum(modelsRs, "ServiceKind"), expected: serviceIds },
      { name: "Rust service_kind_from_id", actual: quotedValues(rustFunctionBody(servicesRs, "service_kind_from_id")), expected: serviceIds },
      { name: "Rust service_descriptors", actual: rustServiceKindRefs(rustFunctionBody(servicesRs, "service_descriptors")), expected: serviceIds },
      { name: "Rust EmailProvider enum", actual: rustEnum(modelsRs, "EmailProvider"), expected: ruling.email_carriers },
      { name: "Rust NativeAppId enum", actual: rustEnum(nativeAppsRs, "NativeAppId"), expected: nativeAppIds },
      { name: "Rust NATIVE_APPS manifest", actual: rustNativeAppRefs(nativeAppsRs.slice(nativeAppsRs.indexOf("const NATIVE_APPS"), nativeAppsRs.indexOf("#[cfg(any(target_os = \"windows\", test))]", nativeAppsRs.indexOf("const NATIVE_APPS")))), expected: nativeAppIds },
      { name: "Rust FirefoxServiceId enum", actual: rustEnum(nativeAppsRs, "FirefoxServiceId"), expected: ruling.email_carriers },
      { name: "Rust FIREFOX_SERVICES allowlist", actual: [...nativeAppsRs.slice(nativeAppsRs.indexOf("const FIREFOX_SERVICES"), nativeAppsRs.indexOf("fn manifest")).matchAll(/FirefoxServiceId::([A-Z][A-Za-z0-9]*)/g)].map((match) => rustVariantId(match[1])), expected: ruling.email_carriers },
      { name: "Rust service_host SERVICES", actual: quotedValues(serviceHostRs.slice(serviceHostRs.indexOf("const SERVICES"), serviceHostRs.indexOf("const EMAIL_GMAIL"))), expected: serviceIds },
      { name: "Rust mass_cleanup manifest", actual: rustServiceAliasRefs(rustFunctionBody(massCleanupRs, "compiled_manifest")), expected: serviceIds },
      { name: "TS ServiceId union", actual: tsUnion(servicesTs, "ServiceId"), expected: serviceIds },
      { name: "TS EmailProvider union", actual: tsUnion(servicesTs, "EmailProvider"), expected: ruling.email_carriers },
      { name: "TS OfferedEmailProvider union", actual: tsUnion(servicesTs, "OfferedEmailProvider"), expected: ruling.email_carriers },
      { name: "TS NativeAppId union", actual: tsUnion(servicesTs, "NativeAppId"), expected: nativeAppIds },
      { name: "TS services.ts serviceIds", actual: tsConstArray(servicesTs, "serviceIds"), expected: serviceIds },
      { name: "TS services.ts emailProviders", actual: tsConstArray(servicesTs, "emailProviders"), expected: ruling.email_carriers },
      { name: "TS services.ts firefoxServiceIds", actual: tsConstArray(servicesTs, "firefoxServiceIds"), expected: ruling.email_carriers },
      { name: "TS homeAppDefinitions", actual: [...servicesTs.matchAll(/homeApp\("([a-z][a-z0-9]*)"/g)].map((match) => match[1]), expected: homeAppIds },
      { name: "TS autoscrub serviceIds", actual: tsConstArray(autoscrubTs, "serviceIds"), expected: serviceIds },
      { name: "TS service-guide serviceIds", actual: tsConstSet(serviceGuideTs, "serviceIds"), expected: serviceIds },
      { name: "TS browserServiceQaIds", actual: tsConstArray(browserQaTs, "browserServiceQaIds"), expected: ruling.email_carriers },
      { name: "TS desktopServicePolicies", actual: [...desktopPolicyTs.matchAll(/policy\("([a-z][a-z0-9]*)"/g)].map((match) => match[1]), expected: homeAppIds },
      { name: "TS nativePreviewApps fallback", actual: tsConstObjectIds(servicesTs, "nativePreviewApps"), expected: nativeAppIds },
      { name: "TS previewRegistry fallback", actual: tsConstServiceCallIds(servicesTs, "previewRegistry"), expected: serviceIds },
      { name: "Rust CHAT_APP_TIMER_POLICIES", actual: rustStructFieldValues(chatAppTimerRs, "CHAT_APP_TIMER_POLICIES", "app_id"), expected: legacyTimerSurfaceIds },
      { name: "Rust MESSAGING_RISK_FACT_ROWS", actual: rustCallFirstArgs(servicesRs, "MESSAGING_RISK_FACT_ROWS", "messaging_risk_facts_row"), expected: unique([...nonSignalMessagingRiskServiceIds, ...ownerRestoredChatApps]) },
      { name: "Rust AdapterService enum", actual: rustEnum(adapterProfileRs, "AdapterService"), expected: adapterProfileSurfaceIds },
    ];

    const tileColourList: CheckedList = { name: "TS app tile colours", actual: cssTileColourIds(stylesCss), expected: tileColourSurfaceIds };
    assertNoTask4254SkippedLists(checkedLists);
    console.log(`TASK4254_TILE_COLOURS_CHECKED=${tileColourList.name === "TS app tile colours"}`);

    for (const list of checkedLists) {
      assertSameSet(list.name, list.actual, list.expected);
    }
    assertSameSet(tileColourList.name, tileColourList.actual, tileColourList.expected);

    assertSameSet("services.test validRegistry fixture", quotedValues(servicesTestTs.slice(servicesTestTs.indexOf("function validRegistry"), servicesTestTs.indexOf("return ids.map"))), serviceIds);
    assertSameSet("mass-cleanup.test fixture", tsConstArray(massCleanupTestTs, "serviceIds"), serviceIds);

    expect(supportMatrix.surface_ruling).toEqual(ruling);
    expect(pricing.surface_policy.surface_ruling).toEqual(ruling);

    for (const cut of ruling.cut_surfaces) {
      expect(homeAppIds, `cut surface ${cut} must not be in active home app ruling`).not.toContain(cut);
      expect(serviceIds, `cut surface ${cut} must not be in active service ruling`).not.toContain(cut);
    }
  });

  it("keeps the mailbox ruling record resolved for task 4086", () => {
    const expected: MailboxReadingRuling = {
      task: "4086",
      ruled_on: "2026-08-07",
      ruled_by: "Liam",
      owner_words: "mailboxes can be read",
      outside_services_correction: "Nothing was ever removed for the ten outside services, so there is nothing to put back there, only new work.",
    };

    expect(ruling.mailbox_reading_ruling, "TASK4086_MAILBOX_RULING_RECORD data/surface-ruling-2026-08-05.json$.mailbox_reading_ruling").toEqual(expected);
    assertText(
      "data/surface-ruling-2026-08-05.json$.mailbox_reading_ruling.owner_words task=4086",
      ruling.mailbox_reading_ruling.owner_words,
      expected.owner_words,
    );
  });
});
