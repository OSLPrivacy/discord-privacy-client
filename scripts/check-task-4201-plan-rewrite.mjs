#!/usr/bin/env node
import { readdirSync, readFileSync, statSync } from "node:fs";

const AUDITS_ROOT = process.env.TASK4201_AUDITS_ROOT ?? "/home/liamw/osl-plan/OSL-AUDITS";

const PLAN_FILES = [
  `${AUDITS_ROOT}/todo/12-final-audit.txt`,
  `${AUDITS_ROOT}/OSL-TODO.txt`,
  `${AUDITS_ROOT}/v2/in/12-final-audit-backup.txt`,
];

const TICK_SOURCES = [
  `${AUDITS_ROOT}/OSL-TODO.txt`,
  `${AUDITS_ROOT}/todo`,
  `${AUDITS_ROOT}/v2/in`,
];

const expectedTickCount = Number(process.env.TASK4201_EXPECTED_TICK_COUNT);

function fail(message) {
  console.error(`TASK4201_FAIL=${message}`);
  process.exit(1);
}

function taskBlock(text, taskId) {
  const start = text.indexOf(`TASK ${taskId}`);
  if (start === -1) fail(`missing TASK ${taskId}`);
  const rest = text.slice(start);
  const next = rest.search(/\nTASK \d+/);
  return next === -1 ? rest : rest.slice(0, next);
}

function line(block, prefix) {
  const found = block
    .split(/\r?\n/)
    .find((candidate) => candidate.startsWith(prefix));
  if (!found) fail(`missing ${prefix}`);
  return found;
}

function mustContain(block, needle, label) {
  if (!block.includes(needle)) fail(`missing ${label}`);
}

function filesUnder(path) {
  const stat = statSync(path);
  if (stat.isFile()) return [path];
  if (!stat.isDirectory()) return [];
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    const child = `${path}/${entry.name}`;
    if (entry.isDirectory()) return filesUnder(child);
    return entry.isFile() ? [child] : [];
  });
}

function namedList(block, label) {
  const match = block.match(new RegExp(`${label} is ([^.]+)\\.`));
  if (!match) fail(`missing ${label}`);
  return match[1];
}

function checkFinishLine(block, taskId) {
  const finishLine = line(block, "done when:");
  const frozenNumberMatches = finishLine.match(/\bexactly\s+\d+\b/gi) ?? [];
  if (frozenNumberMatches.length > 0) {
    fail(`${taskId} finish line has frozen number: ${frozenNumberMatches.join(",")}`);
  }
  return finishLine;
}

const texts = PLAN_FILES.map((file) => [file, readFileSync(file, "utf8")]);
const activeTodo = texts.find(([file]) => file.endsWith("/todo/12-final-audit.txt"))?.[1];
if (!activeTodo) fail("missing active todo text");
if (!activeTodo.includes("TASK 3740 [x]")) fail("TASK 3740 tick removed");
if (!activeTodo.includes("TASK 3742 [x]")) fail("TASK 3742 tick removed");

let finishLineFrozenMatches = 0;
for (const [file, text] of texts) {
  const telegram = taskBlock(text, "3740");
  const whatsapp = taskBlock(text, "3742");

  mustContain(telegram, "Add 2 added places: supergroup and saved messages", `${file} 3740 added places`);
  mustContain(
    telegram,
    "Telegram 6-name list is direct_message, group_chat, channel, public_post, supergroup and saved_messages",
    `${file} 3740 six-name list`,
  );
  if (/\bstory\b/i.test(namedList(telegram, "Telegram 6-name list"))) {
    fail("3740 telegram story must stay refused");
  }
  mustContain(
    telegram,
    "1029a and 4218 are the only way a Telegram story can ever be added",
    `${file} 3740 story gate`,
  );

  mustContain(
    whatsapp,
    "Add 3 added places: community, community group and broadcast list",
    `${file} 3742 added places`,
  );
  mustContain(
    whatsapp,
    "WhatsApp 6-name list is direct_message, group_chat, channel, community, community_group and broadcast_list",
    `${file} 3742 six-name list`,
  );
  if (/\bstatus\b/i.test(namedList(whatsapp, "WhatsApp 6-name list"))) {
    fail("3742 whatsapp status must stay refused");
  }
  mustContain(
    whatsapp,
    "1089d and 4220 are the only way WhatsApp Status can ever be added",
    `${file} 3742 Status gate`,
  );

  finishLineFrozenMatches += (checkFinishLine(telegram, "3740").match(/\bexactly\s+\d+\b/gi) ?? []).length;
  finishLineFrozenMatches += (checkFinishLine(whatsapp, "3742").match(/\bexactly\s+\d+\b/gi) ?? []).length;
}

const tickCount = TICK_SOURCES.flatMap(filesUnder).reduce(
  (count, file) => count + (readFileSync(file, "utf8").match(/\[x\]/g) ?? []).length,
  0,
);
if (!Number.isFinite(expectedTickCount)) {
  fail("set TASK4201_EXPECTED_TICK_COUNT to the pre-edit tick count");
}
if (tickCount < expectedTickCount) {
  fail(`tick count fell from ${expectedTickCount} to ${tickCount}`);
}

console.log(`TASK4201_FILES_CHECKED=${PLAN_FILES.length}`);
console.log("TASK4201_3740_TICK_PRESENT=true");
console.log("TASK4201_3742_TICK_PRESENT=true");
console.log("TASK4201_3740_ADDED_PLACE_COUNT=2");
console.log("TASK4201_3740_ADDED_PLACES=supergroup,saved messages");
console.log("TASK4201_3740_TELEGRAM_LIST_COUNT=6");
console.log("TASK4201_3740_TELEGRAM_LIST=direct_message,group_chat,channel,public_post,supergroup,saved_messages");
console.log("TASK4201_3740_STORY_ONLY_WAY=1029a+4218");
console.log("TASK4201_3742_ADDED_PLACE_COUNT=3");
console.log("TASK4201_3742_ADDED_PLACES=community,community group,broadcast list");
console.log("TASK4201_3742_WHATSAPP_LIST_COUNT=6");
console.log("TASK4201_3742_WHATSAPP_LIST=direct_message,group_chat,channel,community,community_group,broadcast_list");
console.log("TASK4201_3742_STATUS_ONLY_WAY=1089d+4220");
console.log(`TASK4201_FINISH_LINES_EXACTLY_NUMBER_MATCHES=${finishLineFrozenMatches}`);
console.log(`TASK4201_TICK_COUNT_BEFORE=${expectedTickCount}`);
console.log(`TASK4201_TICK_COUNT_AFTER=${tickCount}`);
console.log(`TASK4201_TICK_COUNT_DELTA=${tickCount - expectedTickCount}`);
console.log("TASK4201_NO_TICK_REMOVED=true");
