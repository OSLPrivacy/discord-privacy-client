import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

function readRepo(path: string): string {
  return readFileSync(new URL(path, `file://${repoRoot}/`), "utf8");
}

describe("§7.11 public ratchet claims stay within the live gate", () => {
  it("does not claim RN traffic is blocked by a false compile-time fuse", () => {
    const cargo = readRepo("Cargo.toml");
    const wireRn = readRepo("crates/ipc/src/wire_rn.rs");
    const state = readRepo("crates/ipc/src/state.rs");

    // Positive controls: the scanner sees both the stale claim shape and the
    // real guard shape if either exists.
    expect("RN_WIRE_IN_ENABLED = false").toMatch(/RN_WIRE_IN_ENABLED\s*=\s*false/u);
    expect(state).toMatch(/rn_wire_in_enabled:\s*AtomicBool::new\(false\)/u);

    expect(wireRn).toMatch(/pub const RN_WIRE_IN_ENABLED:\s*bool\s*=\s*true;/u);
    expect(cargo).not.toMatch(/RN_WIRE_IN_ENABLED\s*=\s*false/u);
    expect(cargo).toMatch(/runtime gate/u);
  });
});
