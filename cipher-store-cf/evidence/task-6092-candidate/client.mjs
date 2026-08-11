#!/usr/bin/env node
import { createCipheriv, createDecipheriv, createECDH, createHash, hkdfSync, randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

const [command, installDir, ...args] = process.argv.slice(2);
if (!command || !installDir) throw new Error("usage: client.mjs <init|send|receive> <install-dir> ...");
mkdirSync(installDir, { recursive: true });
const identityPath = join(installDir, "identity.json");
const consumedPath = join(installDir, "consumed.json");

function identity() {
  return JSON.parse(readFileSync(identityPath, "utf8"));
}

function clientId(publicHex) {
  return createHash("sha256").update(Buffer.from(publicHex, "hex")).digest("hex").slice(0, 32);
}

async function serviceFetch(url, options = {}) {
  let response;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    response = await fetch(url, options);
    const transientEdge = (response.status === 404 || response.status === 500)
      && (response.headers.get("content-type") ?? "").includes("text/html");
    if (!transientEdge) return response;
    await new Promise((resolveWait) => setTimeout(resolveWait, 1000));
  }
  return response;
}

function seal(recipientPublicHex, messageId, plaintext) {
  const ephemeral = createECDH("prime256v1");
  ephemeral.generateKeys();
  const shared = ephemeral.computeSecret(Buffer.from(recipientPublicHex, "hex"));
  const key = Buffer.from(hkdfSync("sha256", shared, Buffer.from(messageId, "hex"), Buffer.from("osl-task-6092-envelope-v1"), 32));
  const nonce = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, nonce);
  cipher.setAAD(Buffer.from(messageId, "hex"));
  const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  const tag = cipher.getAuthTag();
  return Buffer.concat([Buffer.from("OSL6092\0"), Buffer.from([1]), ephemeral.getPublicKey(), nonce, tag, ciphertext]);
}

function open(localIdentity, messageId, envelope) {
  if (envelope.subarray(0, 8).toString("binary") !== "OSL6092\0" || envelope[8] !== 1) {
    throw new Error("authenticated envelope header refused");
  }
  const peerPublic = envelope.subarray(9, 74);
  const nonce = envelope.subarray(74, 86);
  const tag = envelope.subarray(86, 102);
  const ciphertext = envelope.subarray(102);
  const local = createECDH("prime256v1");
  local.setPrivateKey(Buffer.from(localIdentity.private_hex, "hex"));
  const shared = local.computeSecret(peerPublic);
  const key = Buffer.from(hkdfSync("sha256", shared, Buffer.from(messageId, "hex"), Buffer.from("osl-task-6092-envelope-v1"), 32));
  const decipher = createDecipheriv("aes-256-gcm", key, nonce);
  decipher.setAAD(Buffer.from(messageId, "hex"));
  decipher.setAuthTag(tag);
  return Buffer.concat([decipher.update(ciphertext), decipher.final()]);
}

if (command === "init") {
  if (!existsSync(identityPath)) {
    const ecdh = createECDH("prime256v1");
    ecdh.generateKeys();
    const record = { private_hex: ecdh.getPrivateKey("hex"), public_hex: ecdh.getPublicKey("hex") };
    writeFileSync(identityPath, JSON.stringify(record));
    writeFileSync(consumedPath, "[]");
  }
  const current = identity();
  process.stdout.write(JSON.stringify({ client_id: clientId(current.public_hex), public_hex: current.public_hex }) + "\n");
} else if (command === "send") {
  const [serviceUrl, recipientPublicHex, kind, offlineText, source] = args;
  if (!serviceUrl || !recipientPublicHex || !["text", "file"].includes(kind) || !["true", "false"].includes(offlineText) || !source) {
    throw new Error("send arguments invalid");
  }
  const plaintext = kind === "file" ? readFileSync(source) : Buffer.from(source, "utf8");
  const messageId = randomBytes(16).toString("hex");
  const envelope = seal(recipientPublicHex, messageId, plaintext);
  const requestBody = JSON.stringify({
    id: messageId,
    recipient: clientId(recipientPublicHex),
    kind,
    offline: offlineText === "true",
    envelope_b64: envelope.toString("base64"),
  });
  const response = await serviceFetch(`${serviceUrl}/v1/messages`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: requestBody,
  });
  if (response.status !== 201) throw new Error(`service store refused ${response.status}: ${await response.text()}`);
  const receipt = await response.json();
  process.stdout.write(JSON.stringify({ message_id: messageId, object_key: receipt.object_key, service_mutant: receipt.mutant, envelope_bytes: envelope.length, plaintext_bytes: plaintext.length }) + "\n");
} else if (command === "receive") {
  const [serviceUrl, messageId, outputPath] = args;
  if (!serviceUrl || !messageId || !outputPath) throw new Error("receive arguments invalid");
  const consumed = JSON.parse(readFileSync(consumedPath, "utf8"));
  if (consumed.includes(messageId)) {
    process.stderr.write(`TASK6092_REFUSED already-consumed id=${messageId} plaintext_output=0\n`);
    process.exit(4);
  }
  const response = await serviceFetch(`${serviceUrl}/v1/messages/${messageId}`);
  if (response.status !== 200) throw new Error(`service fetch refused ${response.status}`);
  const envelope = Buffer.from(await response.arrayBuffer());
  let plaintext;
  try {
    plaintext = open(identity(), messageId, envelope);
  } catch {
    process.stderr.write(`TASK6092_REFUSED authentication-failed id=${messageId} plaintext_output=0\n`);
    process.exit(3);
  }
  writeFileSync(outputPath, plaintext);
  const commit = await serviceFetch(`${serviceUrl}/v1/messages/${messageId}/consume`, { method: "POST" });
  if (commit.status !== 200) {
    writeFileSync(outputPath, Buffer.alloc(0));
    throw new Error(`consume commit refused ${commit.status}`);
  }
  consumed.push(messageId);
  writeFileSync(consumedPath, JSON.stringify(consumed));
  process.stdout.write(JSON.stringify({ message_id: messageId, plaintext_bytes: plaintext.length, output: basename(outputPath) }) + "\n");
} else {
  throw new Error(`unknown command ${command}`);
}
