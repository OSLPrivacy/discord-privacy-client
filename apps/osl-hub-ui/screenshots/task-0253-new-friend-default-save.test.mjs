import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const REPO_ROOT = path.resolve(APP_ROOT, "..", "..");
const FIXTURE_PAGE = "screenshots/task-0253-new-friend-default-save-fixture.html";
const SAVED = Object.freeze({
  accountReach: "all_shared_chats",
  autoWhitelist: "only_if_a_friend",
  verificationWarnings: "never",
});

async function evaluateValue(page, expression) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
  }
  return result.result.value;
}

async function waitReady(page) {
  await evaluateValue(page, `new Promise((resolve, reject) => {
    const deadline = Date.now() + 15000;
    const tick = () => {
      if (document.querySelector("#fixture")?.dataset.task0253State === "ready") return resolve(true);
      if (Date.now() > deadline) return reject(new Error("TASK 0253 fixture did not become ready"));
      setTimeout(tick, 20);
    };
    tick();
  })`);
}

const READ_CHOICES = `(() => Object.fromEntries(
  [...document.querySelectorAll("[data-new-friend-control]")].map((control) => [
    control.dataset.newFriendControl,
    control.querySelector('input[type="radio"]:checked')?.value ?? "",
  ]),
))()`;

test("TASK 0253 Save default persists all three choices across reopening the fixture", async () => {
  const commandSource = readFileSync(path.join(REPO_ROOT, "crates", "ipc", "src", "commands.rs"), "utf8");
  assert.match(commandSource, /pub fn cmd_osl_save_new_friend_defaults\s*\(/u);
  const dto = /pub struct NewFriendDefaultsDto \{([^}]*)\}/u.exec(commandSource);
  assert.ok(dto, "NewFriendDefaultsDto is missing from the real command source");
  assert.deepEqual(
    [...dto[1].matchAll(/pub ([a-z_]+): String/gu)].map((match) => match[1]),
    ["account_reach", "auto_whitelist", "verification_warnings"],
  );

  const server = await createServer({
    root: APP_ROOT,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await server.listen();
  const address = server.httpServer?.address();
  assert.ok(address && typeof address !== "string");
  const url = `http://127.0.0.1:${address.port}/${FIXTURE_PAGE}`;
  const chrome = await launchChrome({
    args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "about:blank"],
  });
  const page = await chrome.openPage();
  try {
    await page.navigate(url, { timeoutMs: 30_000 });
    await waitReady(page);
    await evaluateValue(page, "localStorage.clear(); location.reload(); true");
    await waitReady(page);

    const savedRun = await evaluateValue(page, `(async () => {
      const choose = (control, value) => {
        const input = document.querySelector('[data-new-friend-control="' + control + '"] input[value="' + value + '"]');
        if (!input) throw new Error("missing choice " + control + ":" + value);
        input.click();
      };
      choose("account-reach", ${JSON.stringify(SAVED.accountReach)});
      choose("auto-whitelist", ${JSON.stringify(SAVED.autoWhitelist)});
      choose("verification-warnings", ${JSON.stringify(SAVED.verificationWarnings)});
      document.querySelector('[data-new-friend-action="save-defaults"]').click();
      const deadline = Date.now() + 15000;
      while (!document.querySelector("#fixture")?.dataset.task0253Saved) {
        if (Date.now() > deadline) throw new Error("Save default did not complete");
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      return {
        saved: document.querySelector("#fixture").dataset.task0253Saved,
        calls: window.task0253CommandCalls,
      };
    })()`);
    const saveCalls = savedRun.calls.filter((call) => call.command === "cmd_osl_save_new_friend_defaults");
    assert.equal(saveCalls.length, 1);
    assert.deepEqual(saveCalls[0].args.defaults, {
      account_reach: SAVED.accountReach,
      auto_whitelist: SAVED.autoWhitelist,
      verification_warnings: SAVED.verificationWarnings,
    });
    assert.equal(savedRun.saved, Object.values(SAVED).join(","));

    // A fresh navigation runs the fixture startup path again and reads the
    // backend store; it does not retain the old radio DOM.
    await page.navigate(url, { timeoutMs: 30_000 });
    await waitReady(page);
    const reopened = await evaluateValue(page, READ_CHOICES);
    const reopenedCalls = await evaluateValue(page, "window.task0253CommandCalls");
    assert.deepEqual(reopened, {
      "account-reach": SAVED.accountReach,
      "auto-whitelist": SAVED.autoWhitelist,
      "verification-warnings": SAVED.verificationWarnings,
    });
    assert.equal(Object.keys(reopened).length, 3);
    assert.equal(reopenedCalls[0]?.command, "cmd_osl_get_new_friend_defaults");

    console.log(
      `TASK0253_SAVE command=${saveCalls[0].command} save_calls=${saveCalls.length} account_reach=${saveCalls[0].args.defaults.account_reach} auto_whitelist=${saveCalls[0].args.defaults.auto_whitelist} verification_warnings=${saveCalls[0].args.defaults.verification_warnings}`,
    );
    console.log(
      `TASK0253_REOPEN account_reach=${reopened["account-reach"]} auto_whitelist=${reopened["auto-whitelist"]} verification_warnings=${reopened["verification-warnings"]} choice_count=${Object.keys(reopened).length}`,
    );
  } finally {
    await chrome.close();
    await server.close();
  }
});
