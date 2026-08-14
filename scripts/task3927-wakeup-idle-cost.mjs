#!/usr/bin/env node
import crypto from "node:crypto";
import net from "node:net";
import { once } from "node:events";
import { performance } from "node:perf_hooks";

const FRAME_BYTES = 2048;
const TICK_INTERVAL_MS = 4000;
const DEFAULT_DURATION_SECONDS = 600;
const ZERO_ID = "0".repeat(32);
const IDLE_TICK = " ".repeat(FRAME_BYTES);
const EMPTY_FRAME = JSON.stringify({
  delivery_tag: ZERO_ID,
  blob_id: ZERO_ID,
}).padEnd(FRAME_BYTES, " ");

function argEnabled(name) {
  return process.argv.includes(name);
}

const stubReceive = argEnabled("--stub-receive");
const durationSeconds = Number(
  process.env.TASK3927_DURATION_SECONDS ?? DEFAULT_DURATION_SECONDS,
);

if (!Number.isFinite(durationSeconds) || durationSeconds <= 0) {
  throw new Error(`invalid TASK3927_DURATION_SECONDS=${durationSeconds}`);
}

class ByteCounter {
  constructor() {
    this.clientSent = 0;
    this.clientReceived = 0;
    this.serverSent = 0;
    this.serverReceived = 0;
  }

  clientNetworkBytes() {
    return this.clientSent + this.clientReceived;
  }

  reset() {
    this.clientSent = 0;
    this.clientReceived = 0;
    this.serverSent = 0;
    this.serverReceived = 0;
  }
}

function websocketAccept(key) {
  return crypto
    .createHash("sha1")
    .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
    .digest("base64");
}

function writeCounted(socket, chunk, counter, field) {
  counter[field] += chunk.length;
  socket.write(chunk);
}

function encodeClientTextFrame(text) {
  const payload = Buffer.from(text, "utf8");
  const mask = crypto.randomBytes(4);
  const header = Buffer.alloc(payload.length <= 125 ? 2 : 4);
  header[0] = 0x81;
  if (payload.length <= 125) {
    header[1] = 0x80 | payload.length;
  } else {
    header[1] = 0x80 | 126;
    header.writeUInt16BE(payload.length, 2);
  }
  const masked = Buffer.alloc(payload.length);
  for (let index = 0; index < payload.length; index += 1) {
    masked[index] = payload[index] ^ mask[index % 4];
  }
  return Buffer.concat([header, mask, masked]);
}

function encodeServerTextFrame(text) {
  const payload = Buffer.from(text, "utf8");
  const header = Buffer.alloc(payload.length <= 125 ? 2 : 4);
  header[0] = 0x81;
  if (payload.length <= 125) {
    header[1] = payload.length;
  } else {
    header[1] = 126;
    header.writeUInt16BE(payload.length, 2);
  }
  return Buffer.concat([header, payload]);
}

function decodeFrame(buffer) {
  if (buffer.length < 2) return null;
  const opcode = buffer[0] & 0x0f;
  const masked = (buffer[1] & 0x80) !== 0;
  let length = buffer[1] & 0x7f;
  let offset = 2;
  if (length === 126) {
    if (buffer.length < offset + 2) return null;
    length = buffer.readUInt16BE(offset);
    offset += 2;
  } else if (length === 127) {
    throw new Error("64-bit websocket frames are outside this harness");
  }
  const maskLength = masked ? 4 : 0;
  if (buffer.length < offset + maskLength + length) return null;
  let mask = null;
  if (masked) {
    mask = buffer.subarray(offset, offset + 4);
    offset += 4;
  }
  const payload = Buffer.from(buffer.subarray(offset, offset + length));
  if (mask) {
    for (let index = 0; index < payload.length; index += 1) {
      payload[index] ^= mask[index % 4];
    }
  }
  return {
    opcode,
    text: payload.toString("utf8"),
    rest: buffer.subarray(offset + length),
  };
}

async function startWakeupServer(counter) {
  const server = net.createServer((socket) => {
    let handshaken = false;
    let buffer = Buffer.alloc(0);

    socket.on("data", (chunk) => {
      counter.serverReceived += chunk.length;
      buffer = Buffer.concat([buffer, chunk]);

      if (!handshaken) {
        const headerEnd = buffer.indexOf("\r\n\r\n");
        if (headerEnd === -1) return;
        const header = buffer.subarray(0, headerEnd).toString("utf8");
        const key = /^Sec-WebSocket-Key:\s*(.+)$/im.exec(header)?.[1]?.trim();
        if (!key) {
          socket.destroy(new Error("missing websocket key"));
          return;
        }
        const response = Buffer.from(
          "HTTP/1.1 101 Switching Protocols\r\n" +
            "Upgrade: websocket\r\n" +
            "Connection: Upgrade\r\n" +
            `Sec-WebSocket-Accept: ${websocketAccept(key)}\r\n` +
            "\r\n",
          "utf8",
        );
        writeCounted(socket, response, counter, "serverSent");
        buffer = buffer.subarray(headerEnd + 4);
        handshaken = true;
      }

      for (;;) {
        const frame = decodeFrame(buffer);
        if (!frame) break;
        buffer = frame.rest;
        if (frame.opcode === 0x8) {
          socket.end();
          break;
        }
        if (frame.opcode !== 0x1 || frame.text !== IDLE_TICK) {
          socket.destroy(new Error("unexpected wakeup frame"));
          break;
        }
        writeCounted(socket, encodeServerTextFrame(EMPTY_FRAME), counter, "serverSent");
      }
    });
  });

  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("wakeup server did not expose a TCP port");
  }
  return { server, port: address.port };
}

async function openClient(port, counter, onFrame) {
  const socket = net.createConnection({ host: "127.0.0.1", port });
  await once(socket, "connect");
  let buffer = Buffer.alloc(0);
  let handshaken = false;
  let handshakeResolve;
  const handshake = new Promise((resolve) => {
    handshakeResolve = resolve;
  });

  socket.on("data", (chunk) => {
    counter.clientReceived += chunk.length;
    buffer = Buffer.concat([buffer, chunk]);
    if (!handshaken) {
      const headerEnd = buffer.indexOf("\r\n\r\n");
      if (headerEnd === -1) return;
      const status = buffer.subarray(0, headerEnd).toString("utf8").split("\r\n")[0];
      if (!status.includes(" 101 ")) {
        socket.destroy(new Error(`websocket handshake refused: ${status}`));
        return;
      }
      buffer = buffer.subarray(headerEnd + 4);
      handshaken = true;
      handshakeResolve();
    }
    for (;;) {
      const frame = decodeFrame(buffer);
      if (!frame) break;
      buffer = frame.rest;
      if (frame.opcode === 0x1) onFrame(frame.text);
    }
  });

  const key = crypto.randomBytes(16).toString("base64");
  const request = Buffer.from(
    "GET /v1/realtime HTTP/1.1\r\n" +
      `Host: 127.0.0.1:${port}\r\n` +
      "Upgrade: websocket\r\n" +
      "Connection: Upgrade\r\n" +
      `Sec-WebSocket-Key: ${key}\r\n` +
      "Sec-WebSocket-Version: 13\r\n" +
      "\r\n",
    "utf8",
  );
  writeCounted(socket, request, counter, "clientSent");
  await handshake;

  return {
    sendTick() {
      writeCounted(socket, encodeClientTextFrame(IDLE_TICK), counter, "clientSent");
    },
    close() {
      socket.end();
    },
  };
}

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function cpuMsSince(start) {
  const usage = process.cpuUsage(start);
  return Math.round((usage.user + usage.system) / 1000);
}

async function runOn(port, counter, seconds, receiveEnabled) {
  const startCpu = process.cpuUsage();
  const deadline = performance.now() + seconds * 1000;
  let peakRss = process.memoryUsage.rss();
  let receivedFrames = 0;
  const client = await openClient(port, counter, (frame) => {
    if (receiveEnabled && frame === EMPTY_FRAME) receivedFrames += 1;
  });
  let ticks = 0;
  while (performance.now() < deadline) {
    client.sendTick();
    ticks += 1;
    peakRss = Math.max(peakRss, process.memoryUsage.rss());
    await wait(Math.min(TICK_INTERVAL_MS, Math.max(0, deadline - performance.now())));
  }
  client.close();
  await wait(50);
  return {
    configuredSeconds: seconds,
    processorTimeMs: cpuMsSince(startCpu),
    peakRssBytes: peakRss,
    networkBytes: counter.clientNetworkBytes(),
    ticks,
    receivedFrames,
  };
}

async function runOff(seconds) {
  const startCpu = process.cpuUsage();
  const deadline = performance.now() + seconds * 1000;
  let peakRss = process.memoryUsage.rss();
  while (performance.now() < deadline) {
    peakRss = Math.max(peakRss, process.memoryUsage.rss());
    await wait(Math.min(1000, Math.max(0, deadline - performance.now())));
  }
  return {
    configuredSeconds: seconds,
    processorTimeMs: cpuMsSince(startCpu),
    peakRssBytes: peakRss,
    networkBytes: 0,
  };
}

async function proveNetworkMoves(port, counter, receiveEnabled) {
  let received = 0;
  const client = await openClient(port, counter, (frame) => {
    if (receiveEnabled && frame === EMPTY_FRAME) received += 1;
  });
  counter.reset();
  const before = counter.clientNetworkBytes();
  client.sendTick();
  const deadline = performance.now() + 2000;
  while (received === 0 && performance.now() < deadline) {
    await wait(10);
  }
  const moved = counter.clientNetworkBytes() - before;
  client.close();
  if (moved <= 0) {
    throw new Error("network measurement did not move after one wake-up message");
  }
  if (received === 0) {
    throw new Error("receiving job did not observe the wake-up replies");
  }
  return moved;
}

const counter = new ByteCounter();
const { server, port } = await startWakeupServer(counter);

try {
  const precheckMoved = await proveNetworkMoves(port, counter, !stubReceive);
  console.log(`TASK3927_NETWORK_PRECHECK_MOVED_BYTES=${precheckMoved}`);
  if (stubReceive) {
    throw new Error("stubbed receive unexpectedly passed");
  }

  counter.reset();
  const on = await runOn(port, counter, durationSeconds, true);
  counter.reset();
  const off = await runOff(durationSeconds);
  const processorDiff = on.processorTimeMs - off.processorTimeMs;

  console.log(`TASK3927_ON_RUN_SECONDS=${on.configuredSeconds}`);
  console.log(`TASK3927_ON_PROCESSOR_TIME_MS=${on.processorTimeMs}`);
  console.log(`TASK3927_ON_PEAK_RSS_BYTES=${on.peakRssBytes}`);
  console.log(`TASK3927_ON_NETWORK_BYTES=${on.networkBytes}`);
  console.log(`TASK3927_ON_TICKS_SENT=${on.ticks}`);
  console.log(`TASK3927_ON_REPLIES_RECEIVED=${on.receivedFrames}`);
  console.log(`TASK3927_OFF_RUN_SECONDS=${off.configuredSeconds}`);
  console.log(`TASK3927_OFF_PROCESSOR_TIME_MS=${off.processorTimeMs}`);
  console.log(`TASK3927_OFF_PEAK_RSS_BYTES=${off.peakRssBytes}`);
  console.log(`TASK3927_OFF_NETWORK_BYTES=${off.networkBytes}`);
  console.log(`TASK3927_PROCESSOR_TIME_DIFF_MS=${processorDiff}`);

  if (on.configuredSeconds !== DEFAULT_DURATION_SECONDS) {
    throw new Error(`main measurement must be ${DEFAULT_DURATION_SECONDS} seconds`);
  }
  if (off.configuredSeconds !== DEFAULT_DURATION_SECONDS) {
    throw new Error(`off measurement must be ${DEFAULT_DURATION_SECONDS} seconds`);
  }
  if (on.receivedFrames === 0) {
    throw new Error("receiving job did not observe the wake-up replies");
  }
} catch (error) {
  console.error(`TASK3927_FAIL=${error.message}`);
  process.exitCode = 1;
} finally {
  server.close();
}
