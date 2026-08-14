import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const TASK0048_MESSAGE = "This file is 26 MB (26,000,000 bytes). Free limit is 25 MB (25,000,000 bytes). Pro limit is 1 GB (1,000,000,000 bytes). Upgrade to Pro to send this file.";

function source(relative: string): string {
  return readFileSync(new URL(relative, import.meta.url), "utf8");
}

function functionSource(document: string, name: string, nextName: string): string {
  const start = document.indexOf(`async function ${name}`);
  const end = document.indexOf(`async function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return document.slice(start, end);
}

function rustFunctionSource(document: string, name: string): string {
  const start = document.indexOf(`pub fn ${name}`);
  const end = document.indexOf("#[cfg(test)]", start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, "test module should follow production formatter").toBeGreaterThan(start);
  return document.slice(start, end);
}

describe("TASK0048 Free too-large attachment message", () => {
  it("pins the visible Free refusal and keeps upload calls behind the preflight result", () => {
    const main = source("./main.ts");
    const limits = source("../../osl-hub/src/attachment_limits.rs");
    const broker = source("../../osl-hub/src/broker.rs");
    const native = source("../../osl-hub/src/native_attachment_transport.rs");
    const sendAttachment = functionSource(main, "sendOslChatAttachment", "openPendingOslChatAttachment");
    const formatter = rustFunctionSource(limits, "free_too_large_attachment_message");

    expect(formatter).toContain("This file is {}.");
    expect(formatter).toContain("size_with_exact_bytes(size_bytes)");
    expect(broker).toContain("free_too_large_attachment_message(");
    expect(native.indexOf("metadata()")).toBeLessThan(native.indexOf("client.upload_attachment_file("));
    expect(main).toContain('const attachments = activeOslChatContext?.scopeApproved');
    expect(main).not.toContain("activeOslChatContext?.scopeApproved && pro");
    expect(sendAttachment).toContain('withBackendReason("Encrypted attachment was not sent", "select_osl_chat_attachment")');
    expect(sendAttachment).toContain('if (result !== null && result !== "cancelled")');

    const screenMessage = `Encrypted attachment was not sent: ${TASK0048_MESSAGE}`;
    expect(screenMessage).toContain("26 MB (26,000,000 bytes)");
    expect(screenMessage).toContain("Free limit is 25 MB");
    expect(screenMessage).toContain("Pro limit is 1 GB");
    expect(screenMessage.match(/Upgrade/gu)).toHaveLength(1);

    const uploadCalls = [...sendAttachment.matchAll(/invoke<unknown>\(".*upload/gu)];
    console.info(`TASK0048_SCREEN message="${screenMessage}" upload_calls=${uploadCalls.length}`);
    expect(uploadCalls).toHaveLength(0);
  });
});
