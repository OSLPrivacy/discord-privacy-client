import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

function readRepo(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(`../../../${relativePath}`, import.meta.url)), "utf8");
}

function lineNumber(source: string, needle: string): number {
  const index = source.indexOf(needle);
  expect(index, `missing source anchor: ${needle}`).toBeGreaterThanOrEqual(0);
  return source.slice(0, index).split("\n").length;
}

describe("THREAT_MODEL reconciliation for retired v4, v5 rationale, and RN guard", () => {
  const threatModel = readRepo("docs/THREAT_MODEL.md");
  const commands = readRepo("crates/ipc/src/commands.rs");
  const wireRn = readRepo("crates/ipc/src/wire_rn.rs");
  const state = readRepo("crates/ipc/src/state.rs");
  const coreBridge = readRepo("apps/osl-hub/src/core_bridge.rs");

  it("documents RN as a downgrade-refusal guard, not a shipping ratchet", () => {
    const rnGateLine = lineNumber(wireRn, "pub const RN_WIRE_IN_ENABLED: bool = false;");
    const rnPinLine = lineNumber(commands, "// Unit b1: RnWirePath dispatch seam.");
    const rnSendLine = lineNumber(wireRn, "pub fn send_rn(");
    const rnInboundLine = lineNumber(commands, "fn accept_rn_bootstrap_inbound_unknown(");

    expect(threatModel).toContain("downgrade-refusal guard, not RN traffic");
    expect(threatModel).toContain("has no production encrypt/decrypt path");
    expect(threatModel).toContain(`crates/ipc/src/wire_rn.rs:${rnGateLine}`);
    expect(threatModel).toContain(`crates/ipc/src/commands.rs:${rnPinLine}`);
    expect(threatModel).toContain(`crates/ipc/src/wire_rn.rs:${rnSendLine}`);
    expect(threatModel).toContain(`crates/ipc/src/commands.rs:${rnInboundLine}`);

    expect(threatModel).not.toContain("nothing uses `wire_rn`");
    expect(threatModel).not.toContain("Its only reference outside its own tests");
  });

  it("keeps the v4 retirement and v5 default-false rationale tied to source", () => {
    const v4GateLine = lineNumber(commands, "let v4_dm_enabled = false;");
    const v5GateLine = lineNumber(commands, "let v5_group_enabled = state");
    const v5StateLine = lineNumber(state, "/// Temporary compatibility kill-switch for v=5");
    const coreBridgeFalseLine = lineNumber(coreBridge, "group_sender_keys_enabled: false,");

    expect(threatModel).toContain(`crates/ipc/src/commands.rs:${v4GateLine}`);
    expect(threatModel).toContain(`crates/ipc/src/commands.rs:${v5GateLine}`);
    expect(threatModel).toContain(`crates/ipc/src/state.rs:${v5StateLine}`);
    expect(threatModel).toContain(`apps/osl-hub/src/core_bridge.rs:${coreBridgeFalseLine}`);
    expect(threatModel).toContain("one account on two machines");
    expect(threatModel).toContain("key receiver chains by `(account, device)`");

    const relevantSources = [commands, state, coreBridge].join("\n");
    expect(relevantSources).toContain("let v4_dm_enabled = false;");
    expect(relevantSources).not.toMatch(/let\s+v4_dm_enabled\s*=\s*true\s*;/u);
    expect(relevantSources).not.toMatch(/sender_keys_enabled\s*\.\s*store\s*\(\s*true\b/u);
  });
});
