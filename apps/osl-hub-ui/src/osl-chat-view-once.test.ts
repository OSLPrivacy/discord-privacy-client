import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

function cargoTest(args: string[], expectedPassed = 1): void {
  const result = spawnSync("cargo", args, {
    cwd: repoRoot,
    encoding: "utf8",
    env: { ...process.env, CARGO_TERM_COLOR: "never" },
  });
  const output = `${result.stdout}\n${result.stderr}`;
  expect(result.status, output).toBe(0);
  expect(output, output).toMatch(new RegExp(`test result: ok\\. ${expectedPassed} passed`, "u"));
}

describe("OSL Chat view-once capture protection", () => {
  it("executes the compiled runtime capture gate", () => {
    cargoTest([
      "test",
      "-p",
      "runtime",
      "--lib",
      "screenshot_gate::tests::rejects_each_missing_prerequisite_before_a_pixel_can_render",
      "--",
      "--exact",
    ]);
    cargoTest([
      "test",
      "-p",
      "runtime",
      "--lib",
      "screenshot_gate::tests::silent_affinity_downgrade_is_not_success",
      "--",
      "--exact",
    ]);
  }, 120_000);

  it("executes the local view-once reveal and reverify gates", () => {
    cargoTest([
      "test",
      "--manifest-path",
      "apps/osl-hub/Cargo.toml",
      "--lib",
      "view_once_open::tests::unprotectable_devices_hold_sealed_bytes_without_rendering_plaintext",
      "--",
      "--exact",
    ]);
    cargoTest([
      "test",
      "--manifest-path",
      "apps/osl-hub/Cargo.toml",
      "--lib",
      "view_once_watch::tests::affinity_flip_mid_render_closes_the_viewer",
      "--",
      "--exact",
    ]);
  }, 120_000);

  it("executes the shipping bilateral capture-consent state", () => {
    cargoTest([
      "test",
      "--manifest-path",
      "apps/osl-hub/Cargo.toml",
      "--test",
      "chat_capture_protection",
      "t14_t17_shipping_state_wires_local_and_peer_transitions",
      "--",
      "--exact",
    ]);
  }, 120_000);
});
