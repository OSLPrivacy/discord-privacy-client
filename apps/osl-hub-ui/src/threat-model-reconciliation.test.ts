import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

type Reconciliation = {
  v4_pairwise_dm: {
    shipping_default: boolean;
    retirement_reason: string;
    fallback_wire: number;
  };
  rn_wire_in: {
    production_encrypt_decrypt: boolean;
    downgrade_refusal_guard: boolean;
  };
  v5_sender_keys: {
    public_product_default: boolean;
    ipc_owner_switch_default: boolean;
    default_false_rationale: string;
    remediation: string[];
  };
  v5_rotation_limits: {
    implemented_triggers: string[];
    unimplemented_triggers: string[];
  };
};

function readRepo(relativePath: string): string {
  return readFileSync(
    fileURLToPath(new URL(`../../../${relativePath}`, import.meta.url)),
    "utf8",
  );
}

function threatModelReconciliation(): Reconciliation {
  const threatModel = readRepo("docs/THREAT_MODEL.md");
  const blocks = [
    ...threatModel.matchAll(
      /```json threat-model-reconciliation-v1\n([\s\S]*?)\n```/gu,
    ),
  ];
  expect(blocks).toHaveLength(1);
  return JSON.parse(blocks[0][1]) as Reconciliation;
}

function rustBooleanInitializer(source: string, field: string): boolean {
  const expression = new RegExp(
    `${field}:\\s*AtomicBool::new\\((true|false)\\)`,
    "u",
  );
  const match = source.match(expression);
  expect(match, `${field} initializer must stay explicit`).not.toBeNull();
  return match![1] === "true";
}

function coreBridgeReadsIpcSenderKeySwitch(source: string): boolean {
  return /group_sender_keys_enabled:\s*state\.osl\.sender_keys_enabled\.load\(Ordering::Acquire\),/u.test(
    source,
  );
}

describe("THREAT_MODEL reconciliation for retired v4 and v5 limits", () => {
  it("threat_model_reconciles_v4_retirement_and_v5_ratchet_limits", () => {
    const model = threatModelReconciliation();

    expect(model.v4_pairwise_dm).toEqual({
      shipping_default: false,
      retirement_reason: "ratchet_desynchronization_failures",
      fallback_wire: 3,
    });
    expect(model.rn_wire_in).toEqual({
      production_encrypt_decrypt: false,
      downgrade_refusal_guard: true,
    });
    expect(model.v5_rotation_limits.implemented_triggers).toEqual([
      "twenty_four_hours",
      "membership_change",
    ]);
    expect(model.v5_rotation_limits.unimplemented_triggers).toEqual([
      "one_hour",
      "five_hundred_messages",
      "suspicious_event",
    ]);
  });

  it("v5_sender_key_defaults_agree_across_the_documented_core_and_bridge", () => {
    const model = threatModelReconciliation();
    const state = readRepo("crates/ipc/src/state.rs");
    const coreBridge = readRepo("apps/osl-hub/src/core_bridge.rs");

    expect(model.v5_sender_keys.public_product_default).toBe(false);
    expect(model.v5_sender_keys.ipc_owner_switch_default).toBe(true);
    expect(rustBooleanInitializer(state, "sender_keys_enabled")).toBe(
      model.v5_sender_keys.ipc_owner_switch_default,
    );
    expect(coreBridgeReadsIpcSenderKeySwitch(coreBridge)).toBe(true);
    expect(model.v5_sender_keys.default_false_rationale).toBe(
      "account_scoped_chain_state_is_not_device_bound",
    );
    expect(new Set(model.v5_sender_keys.remediation)).toEqual(
      new Set([
        "device_bound_sender_key_chains",
        "multi_device_ordering_tests",
        "rotation_claims_limited_to_implemented_triggers",
      ]),
    );
  });
});
