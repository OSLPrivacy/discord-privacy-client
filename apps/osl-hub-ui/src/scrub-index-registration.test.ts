import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function readRelative(relativePath: string): string {
  return readFileSync(new URL(relativePath, import.meta.url), "utf8");
}

function handlerCommands(source: string): Set<string> {
  const start = source.indexOf("macro_rules! hub_tauri_commands");
  if (start < 0) throw new Error("hub_tauri_commands macro missing");
  const end = source.indexOf("macro_rules! hub_tauri_generate_handler", start);
  if (end < 0) throw new Error("hub_tauri_commands terminator missing");
  return new Set(source.slice(start, end)
    .split("\n")
    .map((line) => line.trim().replace(/,$/u, ""))
    .filter((name) => /^[a-z_]+$/u.test(name)));
}

function permissionCommands(source: string): Map<string, string> {
  const permissions = new Map<string, string>();
  let identifier: string | null = null;
  for (const line of source.split("\n").map((value) => value.trim())) {
    const id = /^identifier = "([^"]+)"$/u.exec(line);
    if (id) {
      identifier = id[1]!;
      continue;
    }
    const command = /^commands\.allow = \["([^"]+)"\]$/u.exec(line);
    if (command && identifier) {
      permissions.set(identifier, command[1]!);
      identifier = null;
    }
  }
  return permissions;
}

function permissionFor(command: string): string {
  return `allow-${command.replaceAll("_", "-")}`;
}

function registeredAndGranted(
  handlers: Set<string>,
  permissions: Map<string, string>,
  capability: { permissions: string[] },
  command: string,
): boolean {
  const permission = permissionFor(command);
  return handlers.has(command)
    && permissions.get(permission) === command
    && capability.permissions.includes(permission);
}

describe("Scrub index registration", () => {
  it("src/scrub-index-registration.test.ts", () => {
    const main = readRelative("../../osl-hub/src/main.rs");
    const handlers = handlerCommands(main);
    const permissions = permissionCommands(readRelative("../../osl-hub/permissions/hub.toml"));
    const capability = JSON.parse(readRelative("../../osl-hub/capabilities/hub.json")) as {
      permissions: string[];
    };

    const commands = [
      "initialize_scrub_index",
      "append_scrub_index_chunk",
      "get_scrub_index_status",
      "pause_scrub_index",
      "resume_scrub_index",
      "cancel_scrub_index",
      "get_autoscrub_run_fl",
      "start_autoscrub_reviewed_run",
      "request_autoscrub_global_stop",
    ] as const;

    for (const command of commands) {
      expect(registeredAndGranted(handlers, permissions, capability, command), command).toBe(true);
    }

    const missing = new Set(handlers);
    missing.delete("cancel_scrub_index");
    expect(registeredAndGranted(missing, permissions, capability, "cancel_scrub_index")).toBe(false);
    expect(new Set(capability.permissions).size).toBe(capability.permissions.length);
  });
});
