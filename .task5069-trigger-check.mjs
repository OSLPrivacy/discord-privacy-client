import { readFileSync } from "node:fs";

const source = readFileSync("apps/osl-hub-ui/src/main.ts", "utf8");
const checks = [
  ["profile", /\[data-open-chat-profile-appearance\].*addEventListener\("click"/u],
  ["pencil", /\[data-start-something\].*addEventListener\("click",.*inboxPrimaryAction\(\)/u],
  ["safety number", /\[data-open-safety-number\].*addEventListener\("click",.*openSafetyNumberPanel\(/u],
];
let failed = false;
for (const [name, pattern] of checks) {
  if (!pattern.test(source)) {
    console.error(`FAIL: ${name} trigger is not wired to its real action`);
    failed = true;
  } else {
    console.log(`PASS: ${name} trigger reaches its real action`);
  }
}
if (/\[data-start-something\].*showToast\(/u.test(source)) {
  console.error("FAIL: pencil trigger is a toast, not the real action");
  failed = true;
}
process.exitCode = failed ? 1 : 0;
