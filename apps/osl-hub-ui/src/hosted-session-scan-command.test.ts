import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

function sourceBetween(source: string, startNeedle: string, endNeedle: string): string {
  const start = source.indexOf(startNeedle);
  if (start < 0) throw new Error(`${startNeedle} missing`);
  const end = source.indexOf(endNeedle, start + startNeedle.length);
  if (end < 0) throw new Error(`${endNeedle} missing`);
  return source.slice(start, end);
}

function commandSurface(source: string): string {
  return sourceBetween(
    source,
    "macro_rules! hub_tauri_commands",
    "macro_rules! hub_tauri_generate_handler",
  );
}

describe("hosted session scan command handler", () => {
  const main = readRelative("../../osl-hub/src/main.rs");

  it("registers and grants only the scan command", () => {
    const handler = commandSurface(main);
    const permissions = readRelative("../../osl-hub/permissions/hub.toml");
    const capability = JSON.parse(readRelative("../../osl-hub/capabilities/hub.json")) as {
      permissions: string[];
    };

    expect(handler).toContain("request_hosted_session_scan_command,");
    expect(permissions).toContain('commands.allow = ["request_hosted_session_scan_command"]');
    expect(capability.permissions).toContain("allow-request-hosted-session-scan-command");
    expect(handler).not.toContain("preview_discord_guided_deletion,");
    expect(handler).not.toContain("execute_discord_guided_deletion,");
    expect(permissions).not.toContain('commands.allow = ["preview_discord_guided_deletion"]');
    expect(permissions).not.toContain('commands.allow = ["execute_discord_guided_deletion"]');
  });

  it("routes the request handler through CheckedHost before scanning", () => {
    const command = sourceBetween(
      main,
      "async fn request_hosted_session_scan_command(",
      "\nfn active_unlocked_osl_user_id(",
    );
    const helper = sourceBetween(
      main,
      "fn checked_hosted_session_scan_flow",
      "\n/// Open the hosted-session scan surface",
    );
    const checkIndex = helper.indexOf("let checked = build_checked()?;");
    const bindingIndex = helper.indexOf("let operator_names = bind_operators(&checked)?;");
    const scanIndex = helper.indexOf("let scan = scan(&checked, &operator_names)?;");
    const recheckIndex = helper.indexOf("recheck(&checked)?;");

    expect(command).toContain("run_checked_hosted_session_scan(app)");
    expect(checkIndex).toBeGreaterThan(-1);
    expect(bindingIndex).toBeGreaterThan(checkIndex);
    expect(scanIndex).toBeGreaterThan(bindingIndex);
    expect(recheckIndex).toBeGreaterThan(scanIndex);
    expect(command).not.toMatch(/\bservice_id\s*:\s*String\b|\baccount_id\s*:\s*String\b|\bString\s*,\s*$/u);
  });

  it("refuses missing attended operator binding instead of permitting an empty scan", () => {
    const checkedHost = sourceBetween(main, "struct CheckedHost {", "\n/// Scan the exact checked native-hosted Discord context.");
    expect(checkedHost).toContain("fn attended_operator_names(&self) -> Result<Vec<String>, String>");
    expect(checkedHost).toContain("Err(\"Hosted session scan requires a reviewed attended operator-name binding\".to_owned())");
    expect(checkedHost).not.toMatch(/Ok\s*\(\s*Vec::new\s*\(\s*\)\s*\)|Ok\s*\(\s*vec!\s*!\s*\[/u);
    expect(checkedHost).not.toMatch(/derive\s*\(\s*(?:Debug|Display)/u);
  });
});
