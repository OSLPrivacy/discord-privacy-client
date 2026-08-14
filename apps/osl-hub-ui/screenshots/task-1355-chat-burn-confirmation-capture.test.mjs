import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { inflateSync } from "node:zlib";
import test from "node:test";
import * as esbuild from "esbuild";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const ARTIFACT_DIR = join(ROOT, "screenshots", "evidence");
const PNG_PATH = join(ARTIFACT_DIR, "task-1355-chat-burn-confirmation-1000x800.png");
const AX_PATH = join(ARTIFACT_DIR, "task-1355-chat-burn-confirmation-1000x800.ax.json");
const WINDOW = Object.freeze({ width: 1000, height: 800 });

// The server fixture: a channel inside a server, so the screen must offer both
// the channel and the whole server.
const SERVER_PLACE = {
  kind: "channel",
  id: "channel-general",
  name: "#general",
  serverId: "server-study-hall",
  serverName: "Study Hall",
  serviceId: "osl-chat",
  serviceName: "OSL Chat",
  accountId: "account-task-1355",
  accountName: "you@osl",
};

async function screenBundle() {
  const outFile = join(tmpdir(), `osl-chat-burn-fixture-${process.pid}-${Date.now()}.js`);
  await esbuild.build({
    stdin: {
      contents: `
        import {
          applyChatBurnConfirmationEvent,
          burnScopeChoices,
          chatBurnConfirmationMarkup,
          chatBurnEventForAction,
          chatBurnRequestDto,
          initialChatBurnConfirmation,
        } from "${join(ROOT, "src", "chat-burn-confirmation.ts")}";
        const place = ${JSON.stringify(SERVER_PLACE)};
        let state = initialChatBurnConfirmation(place);
        const root = document.getElementById("screen-root");
        const publish = () => {
          window.__burnState = { scope: state.scope, side: state.side, hideOthers: state.hideOthers, outcome: state.outcome };
          window.__burnRequest = chatBurnRequestDto(state);
          window.__burnScopeChoices = burnScopeChoices(place);
        };
        const render = () => { root.innerHTML = chatBurnConfirmationMarkup(state); publish(); };
        document.addEventListener("click", (clickEvent) => {
          const control = clickEvent.target.closest("[data-burn-action]");
          if (!control) return;
          const event = chatBurnEventForAction(control.dataset.burnAction, control.dataset.burnValue);
          if (!event) return;
          state = applyChatBurnConfirmationEvent(state, event);
          render();
        });
        render();
      `,
      resolveDir: ROOT,
      sourcefile: "chat-burn-confirmation-fixture-entry.ts",
      loader: "ts",
    },
    bundle: true,
    platform: "browser",
    format: "iife",
    outfile: outFile,
    loader: { ".css": "empty", ".svg": "text", ".png": "dataurl" },
    logLevel: "silent",
  });
  return readFileSync(outFile, "utf8");
}

function fixtureHtml(script) {
  const css = [
    readFileSync(join(ROOT, "src", "styles.css"), "utf8"),
    readFileSync(join(ROOT, "src", "chat-burn-confirmation.css"), "utf8"),
    "html, body { height: 100%; } body { margin: 0; background: var(--bg); }",
  ].join("\n");
  return `<!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>Confirm this burn</title>
        <style>${css}</style>
      </head>
      <body>
        <div id="screen-root"></div>
        <script>${script}</script>
      </body>
    </html>`;
}

function fixtureServer(html) {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  return new Promise((resolveServer) => {
    server.listen(0, "127.0.0.1", () => {
      resolveServer({ server, url: `http://127.0.0.1:${server.address().port}/` });
    });
  });
}

function closeServer(server) {
  return new Promise((resolveClose, reject) =>
    server.close((error) => (error ? reject(error) : resolveClose())),
  );
}

function readPng(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const data = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString("ascii");
    const chunk = buffer.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;
    if (type === "IHDR") {
      width = chunk.readUInt32BE(0);
      height = chunk.readUInt32BE(4);
      assert.equal(chunk[8], 8, "PNG must be 8-bit for this evidence decoder");
      colorType = chunk[9];
      assert.ok(colorType === 2 || colorType === 6, `unsupported PNG color type ${colorType}`);
    } else if (type === "IDAT") {
      data.push(chunk);
    } else if (type === "IEND") {
      break;
    }
  }
  const channels = colorType === 6 ? 4 : 3;
  const stride = width * channels;
  const inflated = inflateSync(Buffer.concat(data));
  const colors = new Set();
  let source = 0;
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[source];
    source += 1;
    const row = Buffer.from(inflated.subarray(source, source + stride));
    source += stride;
    for (let x = 0; x < stride; x += 1) {
      const left = x >= channels ? row[x - channels] : 0;
      const up = previous[x] ?? 0;
      const upLeft = x >= channels ? previous[x - channels] : 0;
      if (filter === 1) row[x] = (row[x] + left) & 0xff;
      else if (filter === 2) row[x] = (row[x] + up) & 0xff;
      else if (filter === 3) row[x] = (row[x] + Math.floor((left + up) / 2)) & 0xff;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        row[x] = (row[x] + (pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft)) & 0xff;
      } else assert.equal(filter, 0, `unsupported PNG row filter ${filter}`);
    }
    for (let x = 0; x < width; x += 1) {
      const src = x * channels;
      colors.add(`${row[src]},${row[src + 1]},${row[src + 2]}`);
    }
    previous = row;
  }
  return { width, height, uniqueColors: colors.size };
}

function axNames(nodes) {
  return nodes
    .map((node) => ({ role: node.role?.value ?? "", name: node.name?.value ?? "" }))
    .filter((node) => node.name);
}

test("TASK 1355 captures the server chat burn confirmation with channel, whole-server and all three sides", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await fixtureServer(fixtureHtml(await screenBundle()));
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 10_000 });

    // 1. What the server fixture puts on screen, before anything is clicked.
    const offered = await page.evaluate(`(() => {
      const read = (action) => [...document.querySelectorAll('[data-burn-action="' + action + '"]')]
        .map((element) => ({
          value: element.dataset.burnValue,
          label: element.textContent.trim().split("\\n")[0].trim(),
          pressed: element.getAttribute("aria-pressed"),
        }));
      return {
        scopes: read("scope"),
        sides: read("side"),
        hideOthersPresent: Boolean(document.querySelector('#chat-burn-hide-others')),
        backPresent: Boolean(document.querySelector('#chat-burn-back')),
        confirmPresent: Boolean(document.querySelector('#chat-burn-confirm')),
        confirmDisabled: document.querySelector('#chat-burn-confirm').disabled,
        visibleText: document.body.innerText,
        fits: document.documentElement.scrollHeight <= window.innerHeight,
      };
    })()`);

    const scopeValues = offered.scopes.map((entry) => entry.value);
    const sideValues = offered.sides.map((entry) => entry.value);
    console.log(`TASK1355_SCOPE_CHOICES=${scopeValues.join(",")}`);
    console.log(`TASK1355_SCOPE_CHOICE_COUNT=${scopeValues.length}`);
    console.log(`TASK1355_SIDE_CHOICES=${sideValues.join(",")}`);
    console.log(`TASK1355_SIDE_CHOICE_COUNT=${sideValues.length}`);
    console.log(
      `TASK1355_SCOPE_LABELS=${offered.scopes.map((entry) => `${entry.value}:${entry.label}`).join("|")}`,
    );
    console.log(
      `TASK1355_SIDE_LABELS=${offered.sides.map((entry) => `${entry.value}:${entry.label}`).join("|")}`,
    );
    console.log(`TASK1355_CONTROLS=scope,side,hide-others,back,confirm`);
    console.log(`TASK1355_CONFIRM_DISABLED_BEFORE_CHOICES=${offered.confirmDisabled}`);
    console.log(`TASK1355_FITS_1000x800=${offered.fits}`);

    // Both server choices ...
    assert.ok(scopeValues.includes("channel"), "server fixture must offer the channel choice");
    assert.ok(scopeValues.includes("server"), "server fixture must offer the whole-server choice");
    // ... and all three side choices.
    assert.deepEqual(sideValues, ["yourSide", "theirSide", "bothSides"]);
    assert.equal(sideValues.length, 3);
    // ... and the remaining three controls.
    assert.equal(offered.hideOthersPresent, true, "hide-others control must be on screen");
    assert.equal(offered.backPresent, true, "Back must be on screen");
    assert.equal(offered.confirmPresent, true, "Confirm must be on screen");
    assert.equal(offered.confirmDisabled, true, "Confirm must wait for a scope and a side");
    assert.equal(offered.fits, true, "screen must fit the fixed window without scrolling");
    assert.ok(offered.scopes.every((entry) => entry.pressed === "false"), "no scope is chosen for you");
    assert.ok(offered.sides.every((entry) => entry.pressed === "false"), "no side is chosen for you");

    for (const label of [
      "This channel",
      "#general",
      "Whole server",
      "Study Hall",
      "Your side",
      "Their side",
      "Both sides",
      "Also hide other people's messages",
      "Back",
      "Confirm burn",
    ]) {
      assert.ok(offered.visibleText.includes(label), `server fixture missing ${label}`);
    }

    // 2. Every choice is really pressable: click each scope and each side in turn.
    const pressed = await page.evaluate(`(() => {
      const click = (action, value) => {
        const element = document.querySelector('[data-burn-action="' + action + '"][data-burn-value="' + value + '"]');
        if (!element) throw new Error("missing control " + action + "=" + value);
        element.click();
        const after = document.querySelector('[data-burn-action="' + action + '"][data-burn-value="' + value + '"]');
        return after.getAttribute("aria-pressed");
      };
      const out = {};
      for (const scope of ["channel", "server"]) out["scope:" + scope] = click("scope", scope);
      for (const side of ["yourSide", "theirSide", "bothSides"]) out["side:" + side] = click("side", side);
      out.summary = document.querySelector('.burnconf-summary').textContent.trim();
      out.confirmDisabled = document.querySelector('#chat-burn-confirm').disabled;
      return out;
    })()`);
    console.log(
      `TASK1355_PRESSED=${Object.entries(pressed)
        .filter(([key]) => key.includes(":"))
        .map(([key, value]) => `${key}=${value}`)
        .join(",")}`,
    );
    console.log(`TASK1355_SUMMARY=${pressed.summary}`);
    for (const key of ["scope:channel", "scope:server", "side:yourSide", "side:theirSide", "side:bothSides"]) {
      assert.equal(pressed[key], "true", `${key} did not take the press`);
    }
    assert.equal(pressed.confirmDisabled, false, "Confirm opens once a scope and a side are chosen");
    assert.equal(pressed.summary, "Confirm burns whole server (Study Hall), both sides.");

    // 3. hide-others and Confirm produce the real backend arguments.
    const confirmed = await page.evaluate(`(() => {
      document.querySelector('#chat-burn-hide-others').click();
      const summary = document.querySelector('.burnconf-summary').textContent.trim();
      document.querySelector('#chat-burn-confirm').click();
      return { summary, state: window.__burnState, request: window.__burnRequest };
    })()`);
    console.log(`TASK1355_HIDE_OTHERS=${confirmed.state.hideOthers}`);
    console.log(`TASK1355_OUTCOME=${confirmed.state.outcome}`);
    console.log(`TASK1355_REQUEST=${JSON.stringify(confirmed.request)}`);
    assert.equal(confirmed.state.hideOthers, true);
    assert.equal(confirmed.state.outcome, "confirmed");
    assert.deepEqual(confirmed.request, {
      choice: "bothSides",
      hideOthersMessages: true,
      scope: {
        scopeKind: "server",
        scopeId: "server-study-hall",
        serviceId: "osl-chat",
        accountId: "account-task-1355",
        serverId: "server-study-hall",
      },
    });
    assert.match(confirmed.summary, /Other people's messages are also hidden from your view\./u);

    // 4. Back on a fresh screen leaves without burning.
    const backed = await page.evaluate(`(() => {
      location.reload();
      return true;
    })()`);
    assert.equal(backed, true);
    await page.navigate(url, { timeoutMs: 10_000 });
    const afterBack = await page.evaluate(`(() => {
      document.querySelector('[data-burn-action="scope"][data-burn-value="channel"]').click();
      document.querySelector('[data-burn-action="side"][data-burn-value="yourSide"]').click();
      document.querySelector('#chat-burn-back').click();
      return { state: window.__burnState };
    })()`);
    console.log(`TASK1355_BACK_OUTCOME=${afterBack.state.outcome}`);
    assert.equal(afterBack.state.outcome, "went-back");

    // 5. The capture itself, from the untouched server fixture.
    await page.navigate(url, { timeoutMs: 10_000 });
    await page.send("Accessibility.enable");
    const axTree = await page.send("Accessibility.getFullAXTree");
    const names = axNames(axTree.nodes);
    writeFileSync(AX_PATH, JSON.stringify(names, null, 2));
    for (const exact of [
      "Confirm this burn",
      "How much to burn",
      "Whose copies",
      "Back",
      "Confirm burn",
    ]) {
      assert.ok(names.some((node) => node.name === exact), `screen tree missing ${exact}`);
    }
    console.log(`TASK1355_AX_NAMED_NODES=${names.length}`);

    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    const image = readPng(png);
    console.log(`TASK1355_PNG=${PNG_PATH}`);
    console.log(`TASK1355_PNG_SIZE=${image.width}x${image.height}`);
    console.log(`TASK1355_PNG_UNIQUE_COLORS=${image.uniqueColors}`);
    assert.deepEqual({ width: image.width, height: image.height }, WINDOW);
    assert.ok(image.uniqueColors > 50, `screenshot looks blank (${image.uniqueColors} colors)`);
  } finally {
    await chrome.close();
    await closeServer(server);
  }
});
