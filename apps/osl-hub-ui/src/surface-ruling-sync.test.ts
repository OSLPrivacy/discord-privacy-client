import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type Ruling = {
  chat_carriers: string[];
  email_carriers: string[];
  native_email_carriers: string[];
  first_party_surfaces: string[];
  cut_surfaces: string[];
  mailbox_reading_ruling: {
    ruled_on: string;
    allowed: boolean;
    owner_words: string;
    outside_services_correction: string;
  };
};

const root = new URL("../../..", import.meta.url);
const ruling = JSON.parse(readFileSync(new URL("data/surface-ruling-2026-08-05.json", root), "utf8")) as Ruling;

const sorted = (values: readonly string[]) => [...values].sort();
const unique = (values: readonly string[]) => sorted([...new Set(values)]);
const browserOnlyChatCarriers = ["messenger"] as const;
// The 2026-08-05 data record is intentionally historical; the later owner
// ruling cuts Tuta from every shipping UI surface without rewriting that record.
const shippingEmailCarriers = ruling.email_carriers.filter((id) => id !== "tuta");
const serviceIds = unique([...ruling.chat_carriers, "email"]);
const homeAppIds = unique([...ruling.chat_carriers, ...shippingEmailCarriers]);
const nativeAppIds = unique([
  ...ruling.chat_carriers.filter((id) => !browserOnlyChatCarriers.includes(id as typeof browserOnlyChatCarriers[number])),
  ...ruling.native_email_carriers,
]);
const instagramCutSurfaceRecordedBefore = 3;
const unresolvedChoicePattern = new RegExp(`\\b(?:${["wait" + "ing", "await" + "ing"].join("|")})\\b`, "iu");

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

function rustEnum(text: string, name: string): string[] {
  const match = new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`, "u").exec(text);
  if (!match) throw new Error(`missing Rust enum ${name}`);
  return [...match[1].matchAll(/^\s*([A-Z][A-Za-z0-9]*)\b/gm)].map((entry) => rustVariantId(entry[1]));
}

function rustVariantId(variant: string): string {
  return variant
    .replace("WhatsApp", "Whatsapp")
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

function collectStringValues(value: unknown, path = "$"): Array<{ path: string; value: string }> {
  if (typeof value === "string") return [{ path, value }];
  if (!value || typeof value !== "object") return [];
  if (Array.isArray(value)) return value.flatMap((entry, index) => collectStringValues(entry, `${path}[${index}]`));
  return Object.entries(value).flatMap(([key, entry]) => collectStringValues(entry, `${path}.${key}`));
}

describe("surface ruling synchronization", () => {
  it("task 4250 keeps Instagram in the recorded three cut-surface lists", () => {
    const lists = [
      {
        name: "data/surface-ruling-2026-08-05.json",
        cutSurfaces: ruling.cut_surfaces,
      },
      {
        name: "docs/status/support-matrix.json",
        cutSurfaces: (JSON.parse(source("docs/status/support-matrix.json")) as { surface_ruling: Ruling }).surface_ruling.cut_surfaces,
      },
      {
        name: "data/pricing.json",
        cutSurfaces: (JSON.parse(source("data/pricing.json")) as { surface_policy: { surface_ruling: Ruling } }).surface_policy.surface_ruling.cut_surfaces,
      },
    ];
    const holders = lists.filter((entry) => entry.cutSurfaces.includes("instagram")).map((entry) => entry.name);

    console.info(
      `TASK4250 instagram_cut_surface_list_count=${holders.length} recorded_before=${instagramCutSurfaceRecordedBefore} holders=${holders.join(", ")}`,
    );
    expect(
      holders.length,
      `Instagram cut-surface list count ${holders.length} no longer matches recorded before figure ${instagramCutSurfaceRecordedBefore}; holders=${holders.join(", ") || "(none)"}`,
    ).toBe(instagramCutSurfaceRecordedBefore);
  });

  it("task 4257 keeps Messenger in every service list with restored catalogue metadata", () => {
    const servicesTs = source("apps/osl-hub-ui/src/services.ts");
    const servicesTestTs = source("apps/osl-hub-ui/src/services.test.ts");
    const modelsRs = source("apps/osl-hub/src/models.rs");
    const servicesRs = source("apps/osl-hub/src/services.rs");
    const serviceHostRs = source("apps/osl-hub/src/service_host.rs");
    const nativeAppsRs = source("apps/osl-hub/src/native_apps.rs");
    const massCleanupRs = source("apps/osl-hub/src/mass_cleanup.rs");
    const autoscrubTs = source("apps/osl-hub-ui/src/autoscrub-contract.ts");
    const serviceGuideTs = source("apps/osl-hub-ui/src/service-guide.ts");
    const mainTs = source("apps/osl-hub-ui/src/main.ts");
    const desktopPolicyTs = source("apps/osl-hub-ui/src/desktop-service-policy.ts");
    const massCleanupTestTs = source("apps/osl-hub-ui/src/mass-cleanup.test.ts");
    const logosTs = source("apps/osl-hub-ui/src/logos.ts");
    const supportMatrix = JSON.parse(source("docs/status/support-matrix.json")) as { surface_ruling: Ruling };
    const pricing = JSON.parse(source("data/pricing.json")) as { surface_policy: { surface_ruling: Ruling } };

    const audited = [
      ["data/surface-ruling chat_carriers", ruling.chat_carriers],
      ["docs/status/support-matrix chat_carriers", supportMatrix.surface_ruling.chat_carriers],
      ["data/pricing surface_policy chat_carriers", pricing.surface_policy.surface_ruling.chat_carriers],
      ["Rust ServiceKind enum", rustEnum(modelsRs, "ServiceKind")],
      ["Rust service_kind_from_id", quotedValues(rustFunctionBody(servicesRs, "service_kind_from_id"))],
      ["Rust service_descriptors", rustServiceKindRefs(rustFunctionBody(servicesRs, "service_descriptors"))],
      ["Rust service_host SERVICES", quotedValues(serviceHostRs.slice(serviceHostRs.indexOf("const SERVICES"), serviceHostRs.indexOf("const EMAIL_GMAIL")))],
      ["Rust FirefoxServiceId enum", rustEnum(nativeAppsRs, "FirefoxServiceId")],
      ["Rust FIREFOX_SERVICES allowlist", [...nativeAppsRs.slice(nativeAppsRs.indexOf("const FIREFOX_SERVICES"), nativeAppsRs.indexOf("fn manifest")).matchAll(/FirefoxServiceId::([A-Z][A-Za-z0-9]*)/g)].map((match) => rustVariantId(match[1]))],
      ["Rust mass_cleanup manifest", rustServiceAliasRefs(rustFunctionBody(massCleanupRs, "compiled_manifest"))],
      ["TS ServiceId union", tsUnion(servicesTs, "ServiceId")],
      ["TS services.ts serviceIds", tsConstArray(servicesTs, "serviceIds")],
      ["TS services.ts firefoxServiceIds", tsConstArray(servicesTs, "firefoxServiceIds")],
      ["TS homeAppDefinitions", [...servicesTs.matchAll(/homeApp\("([a-z][a-z0-9]*)"/g)].map((match) => match[1])],
      ["TS autoscrub serviceIds", tsConstArray(autoscrubTs, "serviceIds")],
      ["TS service-guide serviceIds", tsConstSet(serviceGuideTs, "serviceIds")],
      ["TS main autoScrubServiceLabels", [...mainTs.slice(mainTs.indexOf("const autoScrubServiceLabels"), mainTs.indexOf("const supportedNativeAppIds")).matchAll(/^\s*([a-z][a-z0-9]*):/gm)].map((match) => match[1])],
      ["TS main importedFirefoxHomeAppIds", quotedValues(mainTs.slice(mainTs.indexOf("const importedFirefoxHomeAppIds"), mainTs.indexOf("const friendsDialogPageSize")))],
      ["TS desktopServicePolicies", [...desktopPolicyTs.matchAll(/policy\("([a-z][a-z0-9]*)"/g)].map((match) => match[1])],
      ["services.test validRegistry fixture", quotedValues(servicesTestTs.slice(servicesTestTs.indexOf("function validRegistry"), servicesTestTs.indexOf("return ids.map")))],
      ["mass-cleanup.test fixture", tsConstArray(massCleanupTestTs, "serviceIds")],
    ] as const;
    const missing = audited.filter(([, values]) => !values.includes("messenger")).map(([name]) => name);
    const serviceHostMessenger = /id: "messenger",\s*display_name: "([^"]+)",\s*initial_url: "([^"]+)"/u.exec(serviceHostRs);
    const descriptorMessenger = /ServiceKind::Messenger,\s*"([^"]+)",\s*"MS"/u.exec(servicesRs);
    const homeTileMessenger = /homeApp\("messenger", "([^"]+)", "messenger", null, "launch", "comingSoon"\)/u.exec(servicesTs);

    console.info(`TASK4257_LISTS_AUDITED=${audited.length}`);
    console.info(`TASK4257_LISTS_MISSING_MESSENGER=${missing.length} missing=${missing.join(",") || "(none)"}`);
    console.info(`TASK4257_SERVICE_ROW_NAME=${descriptorMessenger?.[1] ?? "(missing)"}`);
    console.info(`TASK4257_HOME_TILE_NAME=${homeTileMessenger?.[1] ?? "(missing)"}`);
    console.info(`TASK4257_WEB_ADDRESS=${serviceHostMessenger?.[2] ?? "(missing)"}`);
    console.info(`TASK4257_PICTURE_SOURCE=${logosTs.includes("messenger: siMessenger") ? "siMessenger" : "(missing)"}`);

    expect(missing).toEqual([]);
    expect(descriptorMessenger?.[1]).toBe("Facebook Messenger");
    expect(serviceHostMessenger?.[1]).toBe("Facebook Messenger");
    expect(serviceHostMessenger?.[2]).toBe("https://www.facebook.com/messages/");
    expect(homeTileMessenger?.[1]).toBe("Messenger");
    expect(logosTs).toContain("siMessenger");
    expect(logosTs).toContain("messenger: siMessenger");
  });

  it("keeps every executable enumeration reconciled to the 2026-08-05 ruling", () => {
    const servicesTs = source("apps/osl-hub-ui/src/services.ts");
    const modelsRs = source("apps/osl-hub/src/models.rs");
    const servicesRs = source("apps/osl-hub/src/services.rs");
    const nativeAppsRs = source("apps/osl-hub/src/native_apps.rs");
    const serviceHostRs = source("apps/osl-hub/src/service_host.rs");
    const massCleanupRs = source("apps/osl-hub/src/mass_cleanup.rs");
    const autoscrubTs = source("apps/osl-hub-ui/src/autoscrub-contract.ts");
    const serviceGuideTs = source("apps/osl-hub-ui/src/service-guide.ts");
    const browserQaTs = source("apps/osl-hub-ui/src/browser-service-qa-shell.ts");
    const desktopPolicyTs = source("apps/osl-hub-ui/src/desktop-service-policy.ts");
    const servicesTestTs = source("apps/osl-hub-ui/src/services.test.ts");
    const massCleanupTestTs = source("apps/osl-hub-ui/src/mass-cleanup.test.ts");
    const supportMatrix = JSON.parse(source("docs/status/support-matrix.json")) as { surface_ruling: Ruling };
    const pricing = JSON.parse(source("data/pricing.json")) as { surface_policy: { surface_ruling: Ruling } };

    assertSameSet("ruling chat carriers", ruling.chat_carriers, ["discord", "signal", "whatsapp", "telegram", "messenger"]);
    assertSameSet("ruling email carriers", ruling.email_carriers, ["gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud", "tuta"]);
    assertSameSet("ruling first-party non-carriers", ruling.first_party_surfaces, ["osl-chats", "osl-mail"]);
    expect(ruling.mailbox_reading_ruling).toEqual({
      ruled_on: "2026-08-07",
      allowed: true,
      owner_words: "mailboxes can be read",
      outside_services_correction: "Nothing was ever removed for the ten outside services, so there is nothing to put back there, only new work.",
    });

    assertSameSet("Rust ServiceKind enum", rustEnum(modelsRs, "ServiceKind"), serviceIds);
    assertSameSet("Rust service_kind_from_id", quotedValues(rustFunctionBody(servicesRs, "service_kind_from_id")), serviceIds);
    assertSameSet("Rust service_descriptors", rustServiceKindRefs(rustFunctionBody(servicesRs, "service_descriptors")), serviceIds);
    assertSameSet("Rust EmailProvider enum", rustEnum(modelsRs, "EmailProvider"), ruling.email_carriers);
    assertSameSet("Rust NativeAppId enum", rustEnum(nativeAppsRs, "NativeAppId"), nativeAppIds);
    assertSameSet("Rust NATIVE_APPS manifest", rustNativeAppRefs(nativeAppsRs.slice(nativeAppsRs.indexOf("const NATIVE_APPS"), nativeAppsRs.indexOf("#[cfg(any(target_os = \"windows\", test))]", nativeAppsRs.indexOf("const NATIVE_APPS")))), nativeAppIds);
    assertSameSet("Rust FirefoxServiceId enum", rustEnum(nativeAppsRs, "FirefoxServiceId"), ["messenger", ...ruling.email_carriers]);
    assertSameSet("Rust FIREFOX_SERVICES allowlist", [...nativeAppsRs.slice(nativeAppsRs.indexOf("const FIREFOX_SERVICES"), nativeAppsRs.indexOf("fn manifest")).matchAll(/FirefoxServiceId::([A-Z][A-Za-z0-9]*)/g)].map((match) => rustVariantId(match[1])), ["messenger", ...ruling.email_carriers]);
    assertSameSet("Rust service_host SERVICES", quotedValues(serviceHostRs.slice(serviceHostRs.indexOf("const SERVICES"), serviceHostRs.indexOf("const EMAIL_GMAIL"))), serviceIds);
    assertSameSet("Rust mass_cleanup manifest", rustServiceAliasRefs(rustFunctionBody(massCleanupRs, "compiled_manifest")), serviceIds);

    assertSameSet("TS ServiceId union", tsUnion(servicesTs, "ServiceId"), serviceIds);
    assertSameSet("TS EmailProvider union", tsUnion(servicesTs, "EmailProvider"), shippingEmailCarriers);
    assertSameSet("TS OfferedEmailProvider union", tsUnion(servicesTs, "OfferedEmailProvider"), shippingEmailCarriers);
    assertSameSet("TS NativeAppId union", tsUnion(servicesTs, "NativeAppId"), nativeAppIds);
    assertSameSet("TS services.ts serviceIds", tsConstArray(servicesTs, "serviceIds"), serviceIds);
    assertSameSet("TS services.ts emailProviders", tsConstArray(servicesTs, "emailProviders"), shippingEmailCarriers);
    assertSameSet("TS services.ts firefoxServiceIds", tsConstArray(servicesTs, "firefoxServiceIds"), ["messenger", ...shippingEmailCarriers]);
    assertSameSet("TS homeAppDefinitions", [...servicesTs.matchAll(/homeApp\("([a-z][a-z0-9]*)"/g)].map((match) => match[1]), homeAppIds);
    assertSameSet("TS autoscrub serviceIds", tsConstArray(autoscrubTs, "serviceIds"), serviceIds);
    assertSameSet("TS service-guide serviceIds", tsConstSet(serviceGuideTs, "serviceIds"), serviceIds);
    assertSameSet("TS browserServiceQaIds", tsConstArray(browserQaTs, "browserServiceQaIds"), shippingEmailCarriers);
    assertSameSet("TS desktopServicePolicies", [...desktopPolicyTs.matchAll(/policy\("([a-z][a-z0-9]*)"/g)].map((match) => match[1]), [...homeAppIds, "instagram", "x"]);

    assertSameSet("services.test validRegistry fixture", quotedValues(servicesTestTs.slice(servicesTestTs.indexOf("function validRegistry"), servicesTestTs.indexOf("return ids.map"))), serviceIds);
    assertSameSet("mass-cleanup.test fixture", tsConstArray(massCleanupTestTs, "serviceIds"), serviceIds);

    expect(supportMatrix.surface_ruling).toEqual(ruling);
    expect(pricing.surface_policy.surface_ruling).toEqual(ruling);

    for (const cut of ruling.cut_surfaces) {
      expect(homeAppIds, `cut surface ${cut} must not be in active home app ruling`).not.toContain(cut);
      expect(serviceIds, `cut surface ${cut} must not be in active service ruling`).not.toContain(cut);
    }
  });

  it("keeps the surface ruling decisions resolved", () => {
    const supportMatrix = JSON.parse(source("docs/status/support-matrix.json")) as { surface_ruling: Ruling };
    const pricing = JSON.parse(source("data/pricing.json")) as { surface_policy: { surface_ruling: Ruling } };
    const records = [
      { name: "data/surface-ruling-2026-08-05.json", value: ruling },
      { name: "docs/status/support-matrix.json", value: supportMatrix.surface_ruling },
      { name: "data/pricing.json", value: pricing.surface_policy.surface_ruling },
    ];

    const matches = records.flatMap((record) =>
      collectStringValues(record.value)
        .filter((entry) => unresolvedChoicePattern.test(entry.value))
        .map((entry) => `${record.name}${entry.path}: ${entry.value}`),
    );

    expect(matches, `unresolved choice text in the surface ruling: ${matches.join("; ")}`).toEqual([]);
  });
});
