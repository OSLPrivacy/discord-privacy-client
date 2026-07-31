import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function readRelative(relativePath: string): string {
  return readFileSync(new URL(relativePath, import.meta.url), "utf8");
}

// The authoritative `hub_tauri_commands!` list was moved out of
// apps/osl-hub/src/main.rs into the library module
// apps/osl-hub/src/hub_command_surface.rs: main.rs is a `[[bin]]` with
// `required-features = ["desktop"]` that CI never compiles, so nothing that
// lived there was ever proven. main.rs kept only the `#[tauri::command]`
// wrappers, the `hub_tauri_commands!(hub_tauri_generate_handler)` invocation,
// and one literal handler list for the signal-qa shell binary. Registration
// proofs therefore have to read both files — reading main.rs alone now yields
// the expansion macro's definition text, which contains no command names at
// all and would silently make every registration assertion vacuous.
function tauriCommandMacroBody(source: string): string {
  const start = source.indexOf("macro_rules! hub_tauri_commands");
  if (start < 0) throw new Error("hub_tauri_commands macro missing");
  // Two spellings of the terminator exist because the surface is split:
  // main.rs follows the list with `hub_tauri_generate_handler`, the library
  // module with its test-only `hub_tauri_command_names` counterpart.
  const end = [
    "macro_rules! hub_tauri_generate_handler",
    "macro_rules! hub_tauri_command_names",
  ]
    .map((terminator) => source.indexOf(terminator, start + 1))
    .filter((index) => index >= 0)
    .sort((a, b) => a - b)[0];
  if (end === undefined) throw new Error("hub_tauri_commands terminator missing");
  return source.slice(start, end);
}

// Every `invoke_handler(tauri::generate_handler![...])` list written out
// literally in a source file — main.rs still registers one such list directly
// for the signal-qa shell build.
function literalInvokeHandlerLists(source: string): string {
  const marker = "invoke_handler(tauri::generate_handler![";
  const lists: string[] = [];
  for (
    let cursor = source.indexOf(marker);
    cursor >= 0;
    cursor = source.indexOf(marker, cursor + 1)
  ) {
    const end = source.indexOf("]);", cursor + marker.length);
    if (end < 0) continue;
    lists.push(source.slice(cursor + marker.length, end));
  }
  return lists.join("\n");
}

function hubCommandSurface(): string {
  return [
    tauriCommandMacroBody(readRelative("../../osl-hub/src/hub_command_surface.rs")),
    literalInvokeHandlerLists(readRelative("../../osl-hub/src/main.rs")),
  ].join("\n");
}

function handlerCommands(source: string): Set<string> {
  return new Set(source
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
    const handlers = handlerCommands(hubCommandSurface());
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
