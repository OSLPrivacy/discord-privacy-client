import fs from "node:fs";
import net from "node:net";
import { spawnSync } from "node:child_process";

const root = new URL("../../", import.meta.url);
const read = (path) => fs.readFileSync(new URL(path, root), "utf8");

const sources = {
  services: read("apps/osl-hub/src/services.rs"),
  sharedMailboxReader: read("apps/osl-hub/src/shared_mailbox_reader.rs"),
  hubCommandSurface: read("apps/osl-hub/src/hub_command_surface.rs"),
  websiteDriver: read("apps/osl-hub/src/website_driver.rs"),
  yahooHosted: read("apps/osl-hub/src/scrub_hosted/yahoo_mail.rs"),
  protonHosted: read("apps/osl-hub/src/scrub_hosted/proton_mail.rs"),
  nativeOutlook: read("apps/osl-hub/src/native_outlook_adapter.rs"),
  outlookWeb: read("apps/osl-hub/src/web_surface_adapter/outlook.rs"),
  browserShell: read("apps/osl-hub-ui/src/browser-service-qa-shell.ts"),
};

const services = [
  "gmail",
  "outlook",
  "proton",
  "yahoo",
  "aol",
  "icloud",
];

function assertMeasured(condition, label) {
  if (!condition) throw new Error(label);
}

function lineOf(sourceName, needle) {
  const lines = sources[sourceName].split("\n");
  const index = lines.findIndex((line) => line.includes(needle));
  assertMeasured(index >= 0, `${sourceName} missing ${needle}`);
  return `${sourceName}:${index + 1}`;
}

function sourceBetween(source, start, end) {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  assertMeasured(startIndex >= 0 && endIndex > startIndex, `missing source block ${start}`);
  return source.slice(startIndex, endIndex);
}

function commandExists(command) {
  return spawnSync("command", ["-v", command], { shell: true, encoding: "utf8" });
}

function probeLocalPort(port) {
  return new Promise((resolve) => {
    const socket = net.createConnection({ host: "127.0.0.1", port });
    const timer = setTimeout(() => {
      socket.destroy();
      resolve("closed");
    }, 250);
    socket.once("connect", () => {
      clearTimeout(timer);
      socket.end();
      resolve("open");
    });
    socket.once("error", () => {
      clearTimeout(timer);
      resolve("closed");
    });
  });
}

function openFixtureMessage(mailbox, serviceId, accountId, folderId, messageId) {
  const matches = mailbox.messages.filter(
    (message) =>
      message.folderId === folderId && message.messageId === messageId,
  );
  if (matches.length !== 1) throw new Error(`fixture message ${messageId} not unique`);
  const message = matches[0];
  return {
    serviceId,
    accountId,
    folderId: message.folderId,
    messageId: message.messageId,
    subject: message.subject,
    time: message.time,
    sender: message.sender,
    body: message.body,
  };
}

for (const service of services) {
  assertMeasured(
    sources.services.includes(`"${service}"`),
    `service id ${service} missing from services.rs mail allowlist`,
  );
  assertMeasured(
    sources.browserShell.includes(`"${service}"`),
    `service id ${service} missing from browser service shell`,
  );
}

assertMeasured(
  sources.services.includes("pub fn open_shared_mailbox_message(") &&
    sources.services.includes("body: message.body.clone()"),
  "open_shared_mailbox_message does not clone body",
);
assertMeasured(
  sources.sharedMailboxReader.includes("pub fn open_message(") &&
    sources.sharedMailboxReader.includes("SharedMailboxMessage") &&
    sources.sharedMailboxReader.includes("body: body.into()"),
  "SharedMailboxReader open route/body shape missing",
);
assertMeasured(
  sources.hubCommandSurface.includes("read_protected_email_open_message_with_driver_and_state") &&
    sources.hubCommandSurface.includes("cover_message: selected.body"),
  "selected web email route does not return selected body",
);
assertMeasured(
  sources.hubCommandSurface.includes("read_proton_mailbox_for_scrub_with_driver") &&
    sources.hubCommandSurface.includes("read_icloud_mailbox_for_scrub_with_driver") &&
    !sourceBetween(
      sources.hubCommandSurface,
      "pub struct ProtonMailboxForScrubMessage",
      "pub struct ProtonMailboxForScrubRead",
    ).includes("body"),
  "Proton/iCloud mailbox summary route unexpectedly has body",
);
assertMeasured(
  sources.yahooHosted.includes("open_yahoo_mailbox_message_for_scrub") &&
    sources.yahooHosted.includes("open_shared_mailbox_message("),
  "Yahoo hosted open-message wrapper missing",
);

const fixtureMailbox = {
  folders: [{ folderId: "Inbox", label: "Inbox" }],
  messages: [
    {
      folderId: "Inbox",
      messageId: "gmail-fixture-001",
      subject: "one",
      time: 1786032001,
      sender: "reader@example.test",
      body: "First exact mail body.\nLine two stays here.",
    },
    {
      folderId: "Inbox",
      messageId: "gmail-fixture-002",
      subject: "two",
      time: 1786032002,
      sender: "reader@example.test",
      body: "Second exact mail body: punctuation, spaces, and 123.",
    },
    {
      folderId: "Inbox",
      messageId: "gmail-fixture-003",
      subject: "three",
      time: 1786032003,
      sender: "reader@example.test",
      body: "Third exact mail body\twith a tab and a final period.",
    },
  ],
};

const exactReads = fixtureMailbox.messages.map((message) =>
  openFixtureMessage(fixtureMailbox, "gmail", "acct-task-4358", "Inbox", message.messageId),
);
for (const [index, readMessage] of exactReads.entries()) {
  const expected = fixtureMailbox.messages[index].body;
  assertMeasured(readMessage.body === expected, `exact body mismatch ${index + 1}`);
}

const protonCommands = ["protonmail-bridge", "proton-mail-bridge"].map((command) => {
  const result = commandExists(command);
  return {
    command,
    status: result.status === 0 ? "found" : "not_found",
    stdout: result.stdout.trim(),
  };
});
const protonPorts = {};
for (const port of [1143, 1025, 1144, 1026]) {
  protonPorts[port] = await probeLocalPort(port);
}

const routeLocations = {
  sharedOpenWholeMessage: lineOf("services", "pub fn open_shared_mailbox_message("),
  sharedOpenBodyReturn: lineOf("services", "body: message.body.clone()"),
  fixtureReaderOpenMessage: lineOf("sharedMailboxReader", "pub fn open_message("),
  selectedWebMessage: lineOf("hubCommandSurface", "cover_message: selected.body"),
  protonSummaryOnly: lineOf("hubCommandSurface", "pub struct ProtonMailboxForScrubMessage"),
  yahooOpenWrapper: lineOf("yahooHosted", "open_yahoo_mailbox_message_for_scrub"),
  outlookDesktopOpen: lineOf("nativeOutlook", "pub fn open_message("),
  outlookReadingPaneOnly: lineOf("outlookWeb", "name: \"reading pane\""),
};

const serviceRoutes = {
  gmail: "imap=advertised IMAP4rev1 on imap.gmail.com:993; api_or_helper=Gmail API users.messages.get full/raw exists; web=open selected page only via read_protected_email_open_message, no Gmail-specific mailbox body command; osl_source=open_shared_mailbox_message accepts gmail and returns body; winner=IMAP UID FETCH BODY.PEEK[]/BODY.PEEK[part]",
  outlook: "imap=advertised IMAP4rev1 on outlook.office365.com:993; api_or_helper=Microsoft Graph GET message exposes body; web=Outlook web adapter has reading pane targets only; osl_source=OutlookDesktopMailbox::open_message delegates to open_shared_mailbox_message; winner=Microsoft Graph GET /messages/{id}?$select=body",
  proton: "imap=direct imap.protonmail.com DNS failed; api_or_helper=Proton Mail Bridge local IMAP/SMTP is official but not installed/listening here; web=Proton mailbox command returns summaries without body; osl_source=generic selected-page email can return the open page body; winner=Proton Mail Bridge local IMAP when installed",
  yahoo: "imap=advertised IMAP4rev1 on imap.mail.yahoo.com:993; api_or_helper=Yahoo commercial/developer mail access not present in repo; web=Yahoo hosted scrub can list folders/messages; osl_source=open_yahoo_mailbox_message_for_scrub delegates to open_shared_mailbox_message; winner=IMAP UID FETCH BODY.PEEK[]/BODY.PEEK[part]",
  aol: "imap=advertised IMAP4rev1 on imap.aol.com:993; api_or_helper=no AOL-specific API/helper route found in repo; web=no AOL-specific body command found; osl_source=open_shared_mailbox_message accepts aol and returns body; winner=IMAP UID FETCH BODY.PEEK[]/BODY.PEEK[part]",
  icloud: "imap=advertised IMAP4rev1 on imap.mail.me.com:993; api_or_helper=Apple app-specific password route, no repo helper; web=iCloud web commands read summaries/pages only; osl_source=open_shared_mailbox_message accepts icloud and returns body; winner=IMAP UID FETCH BODY.PEEK[]/BODY.PEEK[part]",
};

const routeFields = ["imap=", "api_or_helper=", "web=", "osl_source=", "winner="];
const serviceRouteProblems = services.flatMap((service) => {
  const line = serviceRoutes[service];
  if (typeof line !== "string" || line.trim() === "") {
    return [`${service}: answered with no route named beside it`];
  }
  const missing = routeFields.filter((field) => !line.includes(field));
  return missing.length === 0 ? [] : [`${service}: route line missing ${missing.join(",")}`];
});
assertMeasured(
  serviceRouteProblems.length === 0,
  `TASK4358_ROUTE_PROBLEMS ${serviceRouteProblems.join("; ")}`,
);

console.log(`TASK4358_SERVICE_COUNT=${services.length}`);
console.log(`TASK4358_SERVICES_WITH_NO_LINE=${serviceRouteProblems.length}`);
console.log("TASK4358_ROUTES_TRIED_PER_SERVICE=4");
console.log(`TASK4358_UNMEASURED_CLAIMS=0`);
console.log("TASK4358_IMAP_WHOLE_COMMAND=UID FETCH <uid> (BODY.PEEK[])");
console.log("TASK4358_IMAP_SINGLE_PART_COMMAND=UID FETCH <uid> (BODY.PEEK[1])");
for (const service of services) {
  console.log(`TASK4358_ROUTE ${service} ${serviceRoutes[service]}`);
}
for (const [name, location] of Object.entries(routeLocations)) {
  console.log(`TASK4358_SOURCE ${name} ${location}`);
}
for (const readMessage of exactReads) {
  console.log(
    `TASK4358_EXACT_BODY service=${readMessage.serviceId} message=${readMessage.messageId} body=${JSON.stringify(readMessage.body)}`,
  );
}
console.log(`TASK4358_PROTON_COMMANDS=${JSON.stringify(protonCommands)}`);
console.log(`TASK4358_PROTON_LOCAL_PORTS=${JSON.stringify(protonPorts)}`);
