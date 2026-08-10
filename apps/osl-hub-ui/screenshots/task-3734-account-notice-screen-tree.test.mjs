import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const FIXTURE = "screenshots/task-3734-account-notice-fixture.html";
const EXPECTED = Object.freeze({
  refund: Object.freeze({
    title: "Refund notice",
    notice: "OSL does not offer refunds. Your purchase and stored data are unchanged.",
  }),
  chargeback: Object.freeze({
    title: "Chargeback notice",
    notice:
      "Your payment was charged back, so Pro has ended. Your messages are unchanged. Pro files remain downloadable for 7 days, then expire.",
  }),
});

function axNodeSummary(nodes) {
  return nodes
    .filter((node) => !node.ignored)
    .map((node) => ({
      role: node.role?.value ?? "",
      name: node.name?.value ?? "",
      value: node.value?.value ?? "",
      description: node.description?.value ?? "",
    }))
    .filter((node) => node.role || node.name || node.value || node.description);
}

function treeText(nodes) {
  return nodes
    .flatMap((node) => [node.role, node.name, node.value, node.description])
    .filter(Boolean)
    .join("\n");
}

async function startVite() {
  const server = await createServer({
    root: APP_ROOT,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation threw");
  }
  return result.result.value;
}

test("TASK 3734 puts the correct refund and chargeback notice in the Account screen tree", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  const counts = { refund: 0, chargeback: 0 };
  try {
    for (const event of ["refund", "chargeback"]) {
      const expected = EXPECTED[event];
      const otherEvent = event === "refund" ? "chargeback" : "refund";
      const other = EXPECTED[otherEvent];
      const pageUrl = `${url}${FIXTURE}?event=${event}`;
      await page.navigate(pageUrl, { timeoutMs: 30_000 });
      await page.send("Accessibility.enable");
      await evaluate(page, `new Promise((resolve, reject) => {
        const deadline = Date.now() + 10000;
        const tick = () => {
          if (document.documentElement.dataset.task3734 === ${JSON.stringify(event)}) {
            requestAnimationFrame(() => requestAnimationFrame(resolve));
            return;
          }
          if (Date.now() > deadline) {
            reject(new Error("TASK 3734 Account notice did not render"));
            return;
          }
          setTimeout(tick, 25);
        };
        tick();
      })`);

      const screen = await evaluate(page, `(() => {
        const notice = document.querySelector(".account-buyer-notice");
        return {
          title: document.querySelector(".account-screen-heading")?.textContent?.trim() ?? "",
          event: notice?.dataset.accountBuyerEvent ?? "",
          noticeTitle: notice?.querySelector("h2")?.textContent?.trim() ?? "",
          notice: notice?.querySelector("p")?.textContent?.trim() ?? "",
          controls: [...(notice?.querySelectorAll("button") ?? [])].map((button) => button.textContent.trim()),
        };
      })()`);
      const ax = await page.send("Accessibility.getFullAXTree");
      const nodes = axNodeSummary(ax.nodes ?? []);
      const accessibleText = treeText(nodes);

      assert.equal(screen.title, "Account");
      assert.equal(screen.event, event);
      assert.equal(screen.noticeTitle, expected.title);
      assert.equal(screen.notice, expected.notice);
      assert.deepEqual(screen.controls, ["Contact support", "Close"]);
      for (const required of ["Account", expected.title, expected.notice, "Contact support", "Close"]) {
        assert.ok(accessibleText.includes(required), `${event} screen tree is missing: ${required}`);
      }
      assert.ok(!accessibleText.includes(other.title), `${event} tree names the ${otherEvent} event`);
      assert.ok(!accessibleText.includes(other.notice), `${event} tree contains the ${otherEvent} notice`);

      counts[event] = nodes.filter((node) => node.name === expected.notice).length;
      assert.ok(counts[event] >= 1, `${event} exact notice count`);

      const artifactPath = path.join(
        ARTIFACT_DIR,
        `task-3734-account-${event}-screen-tree.json`,
      );
      writeFileSync(
        artifactPath,
        `${JSON.stringify({ schema: "osl-task-3734-account-notice-screen-tree-v1", url: pageUrl, screen, axNodes: nodes }, null, 2)}\n`,
      );

      await evaluate(page, `document.querySelector('[data-account-action="contact-support"]').click()`);
      const contactedEvents = await evaluate(page, "window.oslContactSupportEvents");
      assert.deepEqual(contactedEvents, [event]);
      await evaluate(page, `document.querySelector('[data-account-action="close-buyer-notice"]').click()`);
      const closedEvents = await evaluate(page, "window.oslClosedBuyerNotices");
      const noticeAfterClose = await evaluate(page, "document.querySelector('.account-buyer-notice')");
      assert.deepEqual(closedEvents, [event]);
      assert.equal(noticeAfterClose, null);

      console.log(`TASK3734_${event.toUpperCase()}_TITLE=${screen.title}`);
      console.log(`TASK3734_${event.toUpperCase()}_EVENT=${screen.event}`);
      console.log(`TASK3734_${event.toUpperCase()}_NOTICE=${screen.notice}`);
      console.log(`TASK3734_${event.toUpperCase()}_NOTICE_COUNT=${counts[event]}`);
      console.log(`TASK3734_${event.toUpperCase()}_CONTROLS=${screen.controls.join("|")}`);
      console.log(`TASK3734_${event.toUpperCase()}_CONTACT_SUPPORT_EVENT=${contactedEvents[0]}`);
      console.log(`TASK3734_${event.toUpperCase()}_CLOSE_EVENT=${closedEvents[0]}`);
      console.log(`TASK3734_${event.toUpperCase()}_NOTICE_AFTER_CLOSE=${noticeAfterClose === null ? 0 : 1}`);
      console.log(`TASK3734_${event.toUpperCase()}_OTHER_EVENT_TITLE_COUNT=0`);
      console.log(`TASK3734_${event.toUpperCase()}_TREE=${artifactPath}`);
    }
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
