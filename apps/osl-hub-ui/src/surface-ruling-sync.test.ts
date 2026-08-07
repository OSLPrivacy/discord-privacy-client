import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type Ruling = {
  chat_carriers: string[];
  email_carriers: string[];
  native_email_carriers: string[];
  first_party_surfaces: string[];
  cut_surfaces: string[];
};

const root = new URL("../../..", import.meta.url);
const ruling = JSON.parse(readFileSync(new URL("data/surface-ruling-2026-08-05.json", root), "utf8")) as Ruling;

const sorted = (values: readonly string[]) => [...values].sort();
const unique = (values: readonly string[]) => sorted([...new Set(values)]);
const serviceIds = unique([...ruling.chat_carriers, "email"]);
const homeAppIds = unique([...ruling.chat_carriers, ...ruling.email_carriers]);
const nativeAppIds = unique([...ruling.chat_carriers, ...ruling.native_email_carriers]);

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

describe("surface ruling synchronization", () => {
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

    assertSameSet("ruling chat carriers", ruling.chat_carriers, ["discord", "signal", "whatsapp", "telegram", "x"]);
    assertSameSet("ruling email carriers", ruling.email_carriers, ["gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud", "tuta"]);
    assertSameSet("ruling first-party non-carriers", ruling.first_party_surfaces, ["osl-chats", "osl-mail"]);

    assertSameSet("Rust ServiceKind enum", rustEnum(modelsRs, "ServiceKind"), serviceIds);
    assertSameSet("Rust service_kind_from_id", quotedValues(rustFunctionBody(servicesRs, "service_kind_from_id")), serviceIds);
    assertSameSet("Rust service_descriptors", rustServiceKindRefs(rustFunctionBody(servicesRs, "service_descriptors")), serviceIds);
    assertSameSet("Rust EmailProvider enum", rustEnum(modelsRs, "EmailProvider"), ruling.email_carriers);
    assertSameSet("Rust NativeAppId enum", rustEnum(nativeAppsRs, "NativeAppId"), nativeAppIds);
    assertSameSet("Rust NATIVE_APPS manifest", rustNativeAppRefs(nativeAppsRs.slice(nativeAppsRs.indexOf("const NATIVE_APPS"), nativeAppsRs.indexOf("#[cfg(any(target_os = \"windows\", test))]", nativeAppsRs.indexOf("const NATIVE_APPS")))), nativeAppIds);
    assertSameSet("Rust FirefoxServiceId enum", rustEnum(nativeAppsRs, "FirefoxServiceId"), ruling.email_carriers);
    assertSameSet("Rust FIREFOX_SERVICES allowlist", [...nativeAppsRs.slice(nativeAppsRs.indexOf("const FIREFOX_SERVICES"), nativeAppsRs.indexOf("fn manifest")).matchAll(/FirefoxServiceId::([A-Z][A-Za-z0-9]*)/g)].map((match) => rustVariantId(match[1])), ruling.email_carriers);
    assertSameSet("Rust service_host SERVICES", quotedValues(serviceHostRs.slice(serviceHostRs.indexOf("const SERVICES"), serviceHostRs.indexOf("const EMAIL_GMAIL"))), serviceIds);
    assertSameSet("Rust mass_cleanup manifest", rustServiceAliasRefs(rustFunctionBody(massCleanupRs, "compiled_manifest")), serviceIds);

    assertSameSet("TS ServiceId union", tsUnion(servicesTs, "ServiceId"), serviceIds);
    assertSameSet("TS EmailProvider union", tsUnion(servicesTs, "EmailProvider"), ruling.email_carriers);
    assertSameSet("TS OfferedEmailProvider union", tsUnion(servicesTs, "OfferedEmailProvider"), ruling.email_carriers);
    assertSameSet("TS NativeAppId union", tsUnion(servicesTs, "NativeAppId"), nativeAppIds);
    assertSameSet("TS services.ts serviceIds", tsConstArray(servicesTs, "serviceIds"), serviceIds);
    assertSameSet("TS services.ts emailProviders", tsConstArray(servicesTs, "emailProviders"), ruling.email_carriers);
    assertSameSet("TS services.ts firefoxServiceIds", tsConstArray(servicesTs, "firefoxServiceIds"), ruling.email_carriers);
    assertSameSet("TS homeAppDefinitions", [...servicesTs.matchAll(/homeApp\("([a-z][a-z0-9]*)"/g)].map((match) => match[1]), homeAppIds);
    assertSameSet("TS autoscrub serviceIds", tsConstArray(autoscrubTs, "serviceIds"), serviceIds);
    assertSameSet("TS service-guide serviceIds", tsConstSet(serviceGuideTs, "serviceIds"), serviceIds);
    assertSameSet("TS browserServiceQaIds", tsConstArray(browserQaTs, "browserServiceQaIds"), ruling.email_carriers);
    assertSameSet("TS desktopServicePolicies", [...desktopPolicyTs.matchAll(/policy\("([a-z][a-z0-9]*)"/g)].map((match) => match[1]), homeAppIds);

    assertSameSet("services.test validRegistry fixture", quotedValues(servicesTestTs.slice(servicesTestTs.indexOf("function validRegistry"), servicesTestTs.indexOf("return ids.map"))), serviceIds);
    assertSameSet("mass-cleanup.test fixture", tsConstArray(massCleanupTestTs, "serviceIds"), serviceIds);

    expect(supportMatrix.surface_ruling).toEqual(ruling);
    expect(pricing.surface_policy.surface_ruling).toEqual(ruling);

    for (const cut of ruling.cut_surfaces) {
      expect(homeAppIds, `cut surface ${cut} must not be in active home app ruling`).not.toContain(cut);
      expect(serviceIds, `cut surface ${cut} must not be in active service ruling`).not.toContain(cut);
    }
  });
});
