#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const repoArgIndex = process.argv.indexOf("--repo");
const repo = repoArgIndex === -1
  ? resolve(new URL("..", import.meta.url).pathname)
  : resolve(process.argv[repoArgIndex + 1] ?? "");
if (repoArgIndex !== -1 && !process.argv[repoArgIndex + 1]) {
  throw new Error("TASK4334 --repo requires a path");
}
const read = (path) => readFileSync(resolve(repo, path), "utf8");

const serviceHost = read("apps/osl-hub/src/service_host.rs");
const nativeApps = read("apps/osl-hub/src/native_apps.rs");
const nativeOutlook = read("apps/osl-hub/src/native_outlook_adapter.rs");
const gmailPreload = read("apps/osl-hub-ui/src/scrub-provider-preloads.ts");
const sharedMailboxReader = read("apps/osl-hub/src/shared_mailbox_reader.rs");

const WEB_PAGE_SET_SERVICES = [
  {
    id: "gmail",
    constName: "EMAIL_GMAIL",
    name: "Gmail",
    initialUrl: "https://mail.google.com/",
    allowedPages: ["mail.google.com", "accounts.google.com"],
    builtSend: false,
    sendRun: null,
    builtRead: false,
    readRun: null,
  },
  {
    id: "proton",
    constName: "EMAIL_PROTON",
    name: "Proton Mail",
    initialUrl: "https://mail.proton.me/",
    allowedPages: ["mail.proton.me", "account.proton.me"],
    builtSend: false,
    sendRun: null,
    builtRead: false,
    readRun: null,
  },
  {
    id: "tuta",
    constName: "EMAIL_TUTA",
    name: "Tuta Mail",
    initialUrl: "https://app.tuta.com/",
    allowedPages: ["app.tuta.com"],
    builtSend: false,
    sendRun: null,
    builtRead: false,
    readRun: null,
  },
  {
    id: "yahoo",
    constName: "EMAIL_YAHOO",
    name: "Yahoo Mail",
    initialUrl: "https://mail.yahoo.com/",
    allowedPages: ["mail.yahoo.com", "login.yahoo.com"],
    builtSend: false,
    sendRun: null,
    builtRead: false,
    readRun: null,
  },
  {
    id: "aol",
    constName: "EMAIL_AOL",
    name: "AOL Mail",
    initialUrl: "https://mail.aol.com/",
    allowedPages: ["mail.aol.com", "login.aol.com", "login.yahoo.com"],
    builtSend: false,
    sendRun: null,
    builtRead: false,
    readRun: null,
  },
  {
    id: "gmx",
    constName: "EMAIL_GMX",
    name: "GMX",
    initialUrl: "https://www.gmx.com/",
    allowedPages: ["www.gmx.com", "login.gmx.com", "navigator.gmx.com"],
    builtSend: false,
    sendRun: null,
    builtRead: false,
    readRun: null,
  },
  {
    id: "maildotcom",
    constName: "EMAIL_MAIL_COM",
    name: "Mail.com",
    initialUrl: "https://www.mail.com/",
    allowedPages: ["www.mail.com", "login.mail.com", "navigator-lxa.mail.com"],
    builtSend: false,
    sendRun: null,
    builtRead: false,
    readRun: null,
  },
  {
    id: "icloud",
    constName: "EMAIL_ICLOUD",
    name: "iCloud Mail",
    initialUrl: "https://www.icloud.com/mail/",
    allowedPages: ["www.icloud.com", "idmsa.apple.com"],
    builtSend: false,
    sendRun: null,
    builtRead: false,
    readRun: null,
  },
];

const REFUSED_WEB_SERVICE = {
  id: "outlook-web",
  name: "Outlook on the web",
  refusedByName: "Outlook opens only through the verified native app",
  builtSend: false,
  sendRun: null,
  builtRead: false,
  readRun: null,
};

const PROGRAM_SERVICE = {
  id: "outlook-desktop",
  name: "Outlook desktop",
  executable: String.raw`Microsoft Office\root\Office16\OUTLOOK.EXE`,
  titleReader: "scripts/qa/outlook/read-outlook-desktop-title.ps1",
  builtSend: false,
  sendRun: null,
  builtRead: false,
  readRun: null,
};

function assertIncludes(source, needle, label) {
  if (!source.includes(needle)) {
    throw new Error(`TASK4334 missing ${label}: ${needle}`);
  }
}

function assertRegex(source, regex, label) {
  if (!regex.test(source)) {
    throw new Error(`TASK4334 missing ${label}: ${regex}`);
  }
}

function serviceManifestBlock(source, constName) {
  const pattern = new RegExp(`const ${constName}: ServiceManifest = ServiceManifest \\{([\\s\\S]*?)\\n\\};`);
  const match = source.match(pattern);
  if (!match) {
    throw new Error(`TASK4334 missing manifest block: ${constName}`);
  }
  return match[1];
}

for (const service of WEB_PAGE_SET_SERVICES) {
  const block = serviceManifestBlock(serviceHost, service.constName);
  assertIncludes(block, `display_name: "${service.name}"`, `${service.id} display name`);
  assertIncludes(block, `initial_url: "${service.initialUrl}"`, `${service.id} initial URL`);
  for (const allowedPage of service.allowedPages) {
    assertIncludes(block, `"${allowedPage}"`, `${service.id} allowed page ${allowedPage}`);
  }
}

assertIncludes(
  serviceHost,
  "EmailProvider::Outlook => return Err(ServiceHostError::ServiceUnavailable)",
  "Outlook web service-host refusal",
);
assertIncludes(nativeApps, REFUSED_WEB_SERVICE.refusedByName, "Outlook Firefox refusal");
assertIncludes(nativeApps, String.raw`relative_path: r"Microsoft Office\root\Office16\OUTLOOK.EXE"`, "Outlook desktop executable path");
assertIncludes(nativeOutlook, "OUTLOOK.EXE", "Outlook classic desktop executable target");
assertRegex(nativeOutlook, /driver_id:\s*"outlook-desktop-win32"/, "Outlook desktop Win32 driver");

assertIncludes(gmailPreload, "GMAIL_WEB_PRELOAD_SCHEMA", "dead Gmail page markings name");
assertIncludes(gmailPreload, 'providerId: "gmail-web"', "dead Gmail page markings provider id");
assertIncludes(gmailPreload, 'allowedHosts: ["mail.google.com"]', "dead Gmail page markings host");

assertIncludes(sharedMailboxReader, "pub struct SharedMailboxReader", "unused mail-reading building block");
assertIncludes(
  sharedMailboxReader,
  "Mail adapters own provider-specific IMAP or webmail access.",
  "unused mail-reading building block boundary",
);

const lines = [
  ...WEB_PAGE_SET_SERVICES.map((service) => ({
    ...service,
    shape: "web page set",
    sendExists: `name=${service.name}; webAddress=${service.initialUrl}; allowedPages=${service.allowedPages.join("|")}`,
    sendDoesNotExist: `builtSend=${service.builtSend}; no live outside-mail send run is recorded by this task`,
    readExists: `name=${service.name}; webAddress=${service.initialUrl}; allowedPages=${service.allowedPages.join("|")}`,
    readDoesNotExist: `builtRead=${service.builtRead}; no live outside-mail read run is recorded by this task`,
  })),
  {
    ...REFUSED_WEB_SERVICE,
    shape: "refused web service",
    sendExists: "refusal by name exists before URL/profile/page-set selection",
    sendDoesNotExist: `webAddress=none; allowedPages=none; builtSend=${REFUSED_WEB_SERVICE.builtSend}`,
    readExists: "refusal by name exists before URL/profile/page-set selection",
    readDoesNotExist: `webAddress=none; allowedPages=none; builtRead=${REFUSED_WEB_SERVICE.builtRead}`,
  },
  {
    ...PROGRAM_SERVICE,
    shape: "program on the machine",
    sendExists: `program=${PROGRAM_SERVICE.executable}; titleReader=${PROGRAM_SERVICE.titleReader}`,
    sendDoesNotExist: `webAddress=none; allowedPages=none; builtSend=${PROGRAM_SERVICE.builtSend}`,
    readExists: `program=${PROGRAM_SERVICE.executable}; titleReader=${PROGRAM_SERVICE.titleReader}`,
    readDoesNotExist: `webAddress=none; allowedPages=none; builtRead=${PROGRAM_SERVICE.builtRead}`,
  },
];

for (const line of lines) {
  if (line.builtSend && !line.sendRun) {
    throw new Error(`TASK4334 claim has no backing run: ${line.id} send builtSend=true`);
  }
  if (line.builtRead && !line.readRun) {
    throw new Error(`TASK4334 claim has no backing run: ${line.id} read builtRead=true`);
  }
}

const shapeCounts = lines.reduce((counts, line) => {
  counts[line.shape] = (counts[line.shape] ?? 0) + 1;
  return counts;
}, {});
const builtClaimCount = lines.filter((line) => line.builtSend || line.builtRead).length;

if (lines.length !== 10) throw new Error(`TASK4334 expected 10 lines, got ${lines.length}`);
if (shapeCounts["web page set"] !== 8) throw new Error(`TASK4334 expected 8 web page sets, got ${shapeCounts["web page set"] ?? 0}`);
if (shapeCounts["refused web service"] !== 1) throw new Error(`TASK4334 expected 1 refused web service, got ${shapeCounts["refused web service"] ?? 0}`);
if (shapeCounts["program on the machine"] !== 1) throw new Error(`TASK4334 expected 1 program on the machine, got ${shapeCounts["program on the machine"] ?? 0}`);
if (builtClaimCount !== 0) throw new Error(`TASK4334 expected 0 built claims, got ${builtClaimCount}`);

for (const line of lines) {
  console.log(
    `TASK4334_SERVICE id=${line.id} name="${line.name}" shape="${line.shape}" send_exists="${line.sendExists}" send_missing="${line.sendDoesNotExist}" read_exists="${line.readExists}" read_missing="${line.readDoesNotExist}"`,
  );
}

console.log(`TASK4334_COUNT total_lines=${lines.length}`);
console.log(`TASK4334_COUNT web_page_sets=${shapeCounts["web page set"]}`);
console.log(`TASK4334_COUNT refused_web_services=${shapeCounts["refused web service"]}`);
console.log(`TASK4334_COUNT programs_on_machine=${shapeCounts["program on the machine"]}`);
console.log(`TASK4334_COUNT built_claims=${builtClaimCount}`);
console.log("TASK4334_DEAD_GMAIL_PAGE_MARKINGS=apps/osl-hub-ui/src/scrub-provider-preloads.ts:GMAIL_WEB_PRELOAD_SCHEMA");
console.log("TASK4334_UNUSED_MAIL_READING_BLOCK=apps/osl-hub/src/shared_mailbox_reader.rs:SharedMailboxReader");
console.log("TASK4334_RESULT=PASS");
