import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  importLocalMessageExport,
  LOCAL_MESSAGE_IMPORT_MAX_BYTES,
  LOCAL_MESSAGE_IMPORT_MAX_CANDIDATES,
  LOCAL_MESSAGE_IMPORT_MAX_TEXT_BYTES,
} from "./local-message-import";

const context = { serviceId: "instagram", accountId: "test@example.com" };

describe("local message export import", () => {
  it("imports one non-empty plain-text line per candidate without granting delete authority", () => {
    expect(importLocalMessageExport("first\n\n second \r\nthird", context)).toEqual([
      {
        serviceId: "instagram",
        accountId: "test@example.com",
        conversationId: "local-import",
        messageLocator: "local-import-1",
        authoredBySelf: false,
        createdAtUnixMs: null,
        text: "first",
      },
      expect.objectContaining({ messageLocator: "local-import-2", authoredBySelf: false, text: " second " }),
      expect.objectContaining({ messageLocator: "local-import-3", authoredBySelf: false, text: "third" }),
    ]);
  });

  it("imports JSON strings and supported object text fields", () => {
    const imported = importLocalMessageExport(JSON.stringify([
      "string message",
      { content: "content message", conversationId: "thread-1", messageLocator: "message-1" },
      { message: "message field", authoredBySelf: true, createdAtUnixMs: 1_700_000_000_000 },
      { text: "text wins", content: "ignored" },
    ]), { ...context, conversationId: "fallback-thread" });

    expect(imported).toHaveLength(4);
    expect(imported?.[0]).toMatchObject({ text: "string message", conversationId: "fallback-thread", authoredBySelf: false });
    expect(imported?.[1]).toMatchObject({ text: "content message", conversationId: "thread-1", messageLocator: "message-1", authoredBySelf: false });
    expect(imported?.[2]).toMatchObject({ text: "message field", authoredBySelf: true, createdAtUnixMs: 1_700_000_000_000 });
    expect(imported?.[3]?.text).toBe("text wins");
  });

  it("defaults missing authorship to false", () => {
    const imported = importLocalMessageExport('[{"text":"unknown author"}]', context);
    expect(imported?.[0]?.authoredBySelf).toBe(false);
  });

  it("enforces UTF-8 byte limits, not JavaScript character counts", () => {
    expect(importLocalMessageExport("🔐".repeat(LOCAL_MESSAGE_IMPORT_MAX_TEXT_BYTES / 4), context)).toHaveLength(1);
    expect(importLocalMessageExport("🔐".repeat(LOCAL_MESSAGE_IMPORT_MAX_TEXT_BYTES / 4 + 1), context)).toBeNull();
    expect(importLocalMessageExport("x".repeat(LOCAL_MESSAGE_IMPORT_MAX_BYTES + 1), context)).toBeNull();
  });

  it("rejects exports over 2,000 candidates", () => {
    const atLimit = Array.from({ length: LOCAL_MESSAGE_IMPORT_MAX_CANDIDATES }, () => "ok");
    expect(importLocalMessageExport(JSON.stringify(atLimit), context)).toHaveLength(LOCAL_MESSAGE_IMPORT_MAX_CANDIDATES);
    expect(importLocalMessageExport(JSON.stringify([...atLimit, "too many"]), context)).toBeNull();
  });

  it("rejects malformed JSON arrays and invalid object fields", () => {
    expect(importLocalMessageExport("[not json", context)).toBeNull();
    expect(importLocalMessageExport('{"text":"not an array"}', context)).toHaveLength(1);
    expect(importLocalMessageExport('[{"text":""}]', context)).toBeNull();
    expect(importLocalMessageExport('[{"text":"ok","authoredBySelf":"yes"}]', context)).toBeNull();
    expect(importLocalMessageExport('[{"text":"ok","createdAtUnixMs":-1}]', context)).toBeNull();
  });

  it("rejects control-dangerous identifiers", () => {
    expect(importLocalMessageExport("message", { serviceId: "Instagram", accountId: "account" })).toBeNull();
    expect(importLocalMessageExport("message", { serviceId: "instagram", accountId: "bad\naccount" })).toBeNull();
    expect(importLocalMessageExport('[{"text":"ok","conversationId":"bad\\u0000id"}]', context)).toBeNull();
    expect(importLocalMessageExport('[{"text":"ok","messageLocator":"bad\\u202eid"}]', context)).toBeNull();
  });

  it("performs no persistence or network work", () => {
    expect(importLocalMessageExport("local only", context)?.[0]?.text).toBe("local only");
    expect(Object.keys(globalThis).includes("invoke")).toBe(false);
  });

  it("keeps the UI import and native scan on a stateless local-only path", () => {
    const ui = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const scanStart = ui.indexOf("async function scanPrivacyExport");
    const scanEnd = ui.indexOf("function sendingSettingsContent", scanStart);
    const scanFlow = ui.slice(scanStart, scanEnd);
    expect(scanStart).toBeGreaterThanOrEqual(0);
    expect(scanEnd).toBeGreaterThan(scanStart);
    expect(scanFlow).toContain("importLocalMessageExport(await file.text()");
    expect(scanFlow).toContain("await scanLocalPrivacy(candidates)");
    expect(scanFlow).not.toMatch(/localStorage|saveOnboardingPreferences|createServiceAccount|\binvoke\s*\(/);

    const nativeMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
    const commandStart = nativeMain.indexOf("async fn scan_local_privacy");
    const commandEnd = nativeMain.indexOf("fn active_unlocked_osl_user_id", commandStart);
    const command = nativeMain.slice(commandStart, commandEnd);
    expect(command).toContain("spawn_blocking");
    expect(command).toContain("privacy_scan::scan_local_messages(messages)");
    expect(command).not.toMatch(/State<'_|AppHandle|write_|save\(|persist/);

    const scanner = readFileSync(new URL("../../osl-hub/src/privacy_scan.rs", import.meta.url), "utf8");
    expect(scanner).toContain('analysis_location: "this_device_only"');
    expect(scanner).toContain("persisted: false");
    expect(scanner).not.toMatch(/std::fs|OpenOptions|reqwest|ureq/);
  });
});
