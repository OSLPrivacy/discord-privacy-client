import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

function registeredCommands(): Set<string> {
  const source = readFileSync(join(here, "main.rs"), "utf8");
  const start = source.indexOf("tauri::generate_handler![");
  expect(start).toBeGreaterThanOrEqual(0);
  const end = source.indexOf("]);", start);
  expect(end).toBeGreaterThan(start);

  return new Set(
    source
      .slice(start, end)
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && !line.startsWith("#["))
      .map((line) => line.replace(/,$/, ""))
      .filter((line) => /^[A-Za-z0-9_]+$/.test(line)),
  );
}

function permissionCommands(): Set<string> {
  const permissions = readFileSync(join(root, "permissions", "hub.toml"), "utf8");
  return new Set(
    permissions
      .split(/\r?\n/)
      .map((line) => line.trim())
      .map((line) => /^commands\.allow = \["([^"]+)"\]$/.exec(line)?.[1])
      .filter((command): command is string => Boolean(command)),
  );
}

function capabilityPermissions(): Set<string> {
  const capability = JSON.parse(
    readFileSync(join(root, "capabilities", "hub.json"), "utf8"),
  ) as { permissions: string[] };
  return new Set(capability.permissions);
}

describe("src/scrub-index-registration.test.ts", () => {
  it("registers and ACL-grants all 9 scrub_index Tauri commands in the merged manifest", () => {
    const commands = [
      "initialize_scrub_index",
      "set_scrub_index_manifest",
      "get_scrub_index_manifest",
      "get_scrub_index_scan",
      "append_scrub_index_chunk",
      "get_scrub_index_status",
      "pause_scrub_index",
      "resume_scrub_index",
      "cancel_scrub_index",
    ];
    const registered = registeredCommands();
    const granted = permissionCommands();
    const capabilities = capabilityPermissions();

    for (const command of commands) {
      expect(registered.has(command), `${command} handler registration`).toBe(true);
      expect(granted.has(command), `${command} permission grant`).toBe(true);
      expect(
        capabilities.has(`allow-${command.replaceAll("_", "-")}`),
        `${command} capability permission`,
      ).toBe(true);
    }
  });
});
