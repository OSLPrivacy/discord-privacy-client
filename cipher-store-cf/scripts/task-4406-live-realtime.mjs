#!/usr/bin/env node
import crypto from "node:crypto";
import http from "node:http";

const FRAME_BYTES = 2048;
const DEFAULT_REALTIME_ADDRESS = "wss://ciphers.oslprivacy.com/v1/realtime";
const IDLE_TICK = " ".repeat(FRAME_BYTES);
const EMPTY_FRAME = JSON.stringify({
  delivery_tag: "0".repeat(32),
  blob_id: "0".repeat(32),
}).padEnd(FRAME_BYTES, " ");
const WEBSOCKET_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

function realtimeAddress() {
  const configured = process.env.OSL_REALTIME_SERVICE_URL?.trim();
  return configured && configured.length > 0 ? configured : DEFAULT_REALTIME_ADDRESS;
}

function isLocalHostname(hostname) {
  const host = hostname.toLowerCase();
  return (
    host === "localhost" ||
    host === "127.0.0.1" ||
    host === "::1" ||
    host.endsWith(".localhost") ||
    /^10\./.test(host) ||
    /^192\.168\./.test(host) ||
    /^172\.(1[6-9]|2\d|3[0-1])\./.test(host)
  );
}

function countsAsRealService(address) {
  const url = new URL(address);
  return url.protocol === "wss:" && !isLocalHostname(url.hostname);
}

async function probeWebSocket(address) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error(`timed out waiting for tick answer from ${address}`));
    }, 15_000);
    const socket = new WebSocket(address);

    socket.addEventListener("open", () => {
      socket.send(IDLE_TICK);
    });
    socket.addEventListener("message", async (event) => {
      clearTimeout(timeout);
      try {
        const text =
          typeof event.data === "string"
            ? event.data
            : Buffer.from(await event.data.arrayBuffer()).toString("utf8");
        socket.close();
        resolve({
          answered: text === EMPTY_FRAME,
          frameBytes: text.length,
          body: text.trim(),
        });
      } catch (error) {
        reject(error);
      }
    });
    socket.addEventListener("error", () => {
      clearTimeout(timeout);
      reject(new Error(`websocket error from ${address}`));
    });
  });
}

function websocketAccept(key) {
  return crypto.createHash("sha1").update(key + WEBSOCKET_GUID).digest("base64");
}

function readClientTextFrame(buffer) {
  const lenCode = buffer[1] & 0x7f;
  let offset = 2;
  let length = lenCode;
  if (lenCode === 126) {
    length = buffer.readUInt16BE(offset);
    offset += 2;
  }
  const mask = buffer.subarray(offset, offset + 4);
  offset += 4;
  const payload = Buffer.from(buffer.subarray(offset, offset + length));
  for (let index = 0; index < payload.length; index += 1) {
    payload[index] ^= mask[index % 4];
  }
  return payload.toString("utf8");
}

function writeServerTextFrame(socket, text) {
  const payload = Buffer.from(text, "utf8");
  const header = Buffer.alloc(4);
  header[0] = 0x81;
  header[1] = 126;
  header.writeUInt16BE(payload.length, 2);
  socket.write(Buffer.concat([header, payload]));
}

async function withFakeService(run) {
  const server = http.createServer();
  server.on("upgrade", (request, socket) => {
    const key = request.headers["sec-websocket-key"];
    socket.write(
      [
        "HTTP/1.1 101 Switching Protocols",
        "Upgrade: websocket",
        "Connection: Upgrade",
        `Sec-WebSocket-Accept: ${websocketAccept(String(key))}`,
        "",
        "",
      ].join("\r\n"),
    );
    socket.on("data", (chunk) => {
      if (readClientTextFrame(chunk) === IDLE_TICK) {
        writeServerTextFrame(socket, EMPTY_FRAME);
        socket.end();
      }
    });
  });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  const address = `ws://127.0.0.1:${port}/v1/realtime`;
  try {
    return await run(address);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

const realAddress = realtimeAddress();
const fake = await withFakeService(async (fakeAddress) => {
  const result = await probeWebSocket(fakeAddress);
  const counts = countsAsRealService(fakeAddress);
  console.log(`TASK4406_FAKE_ADDRESS=${fakeAddress}`);
  console.log(`TASK4406_FAKE_TICK_ANSWERED=${result.answered}`);
  console.log(`TASK4406_FAKE_COUNTS=${counts}`);
  return { counts };
});

console.log(`TASK4406_REAL_ADDRESS=${realAddress}`);
const realCounts = countsAsRealService(realAddress);
console.log(`TASK4406_REAL_ADDRESS_COUNTS=${realCounts}`);

let real;
try {
  real = await probeWebSocket(realAddress);
} catch (error) {
  console.log("TASK4406_TICK_ANSWERED_BY_REAL_SERVICE=false");
  console.error(`TASK4406_REAL_ERROR=${error.message}`);
  process.exitCode = 1;
}

if (real) {
  const tickAnsweredByRealService = realCounts && real.answered;
  const localFakesUsed = tickAnsweredByRealService ? 0 : 1;
  console.log(`TASK4406_REAL_FRAME_BYTES=${real.frameBytes}`);
  console.log(`TASK4406_REAL_FRAME_BODY=${real.body}`);
  console.log(`TASK4406_TICK_ANSWERED_BY_REAL_SERVICE=${tickAnsweredByRealService}`);
  console.log(`TASK4406_LOCAL_FAKES_USED=${localFakesUsed}`);
  if (!tickAnsweredByRealService) process.exitCode = 1;
}

if (fake.counts) {
  console.error("TASK4406_FAKE_ERROR=local fake was incorrectly counted");
  process.exitCode = 1;
}
