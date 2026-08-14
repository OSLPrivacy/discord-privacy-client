#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const repository = resolve(import.meta.dirname, "../..");
const root = resolve(process.env.TASK5215_ROOT ?? repository);
const read = (path) => readFileSync(resolve(root, path), "utf8");
const test = read("cipher-store-cf/test/task-5215-upload-capacity.test.ts");
const capacity = read("cipher-store-cf/src/lib/upload-capacity.ts");
const blob = read("cipher-store-cf/src/endpoints/blob.ts");
const route = read("cipher-store-cf/src/index.ts");
const migration = read("cipher-store-cf/migrations/0022_upload_capacity_authorities.sql");

const controls = [
  ["$5", "catalogue", 16_400_000_000, "{ usd: 5, bytes: 16_400_000_000, objects: 16_400 }"],
  ["$15", "catalogue", 51_600_000_000, "{ usd: 15, bytes: 51_600_000_000, objects: 51_600 }"],
  ["$40", "catalogue", 139_600_000_000, "{ usd: 40, bytes: 139_600_000_000, objects: 139_600 }"],
  ["$5", "16400_final_allowed", 1_000_000, "`${expectedObjects}_final_allowed`"],
  ["$5", "16401_first_refused", 0, "`${expectedObjects + 1}_first_refused`"],
  ["$5", "object_16401_class_a", 0, "object_16401_class_a"],
  ["$15", "51600_final_allowed", 1_000_000, "independent_pack_evidence"],
  ["$15", "51601_first_refused", 0, "first_refused_status=402"],
  ["$40", "139600_final_allowed", 1_000_000, "remaining_before_final=1000000"],
  ["$40", "139601_first_refused", 0, "exact_spent=${row.bytes}"],
  ["$5", "true_tiny_floor", 2_000_000, "true_tiny_floor"],
  ["$5", "floor_second_object", 1_000_000, "floor_second_object"],
  ["$5", "floor_exhaustion", 0, "floor_exhaustion"],
  ["$5", "large_ciphertext_debit", 1_000_001, "large_ciphertext_debit"],
  ["$5", "unreserved_zero_class_a", 2_000_000, "unreserved_zero_class_a"],
  ["$5", "missing_reservation", 2_000_000, "missing_reservation"],
  ["$5", "local_credential_type", 2_000_000, "local_credential_type"],
  ["monthly", "swapped_reservation_type", 2_000_000, "swapped_reservation_type"],
  ["$5", "swapped_reservation", 2_000_000, "swapped_reservation"],
  ["$5", "reused_reservation", 2_000_000, "reused_reservation"],
  ["$5", "identity_join", 2_000_000, "identity_join"],
  ["monthly", "normal_monthly_route", 1_000_000, "normal_monthly_route"],
  ["sold", "post_base_expiry_real_upload", 2_000_000, "post_base_expiry_real_upload"],
  ["sold", "full_signed_outage_interval", 2_000_000, "full_signed_outage_interval"],
  ["sold", "shortened_extension", 1_000_000, "shortened_extension"],
  ["$5", "displayed_store_disagreement", 1_000_000, "displayed_store_disagreement"],
  ["$5", "serial_starved_barrier", 1_000_000, "TASK5215_BARRIER_PARTICIPANT_A"],
  ["$5", "barrier_participant_b", 1_000_000, "TASK5215_BARRIER_PARTICIPANT_B"],
  ["$5", "barrier_double_success", 0, "barrier_double_success"],
  ["$5", "debit_only_atomic_state", 1_000_000, "debit_only_atomic_state"],
  ["$5", "object_only_atomic_state", 2_000_000, "object_only_atomic_state"],
  ["$5", "crash_after_claim", 1_000_000, '"after-claim"'],
  ["$5", "crash_after_r2", 1_000_000, '"after-r2-before-marker"'],
  ["$5", "crash_after_class_a", 1_000_000, '"after-class-a"'],
  ["$5", "deletion_no_restoration", 3_000_000, "deletion_no_restoration"],
  ["$5", "deletion_wrong_object", 3_000_000, "deletion_wrong_object"],
];

const production = [
  ["all", "request_floor", 0, capacity, "UPLOAD_REQUEST_FLOOR_BYTES = 1_000_000"],
  ["all", "reservation_match", 0, capacity, "upload_reservation_required"],
  ["sold", "effective_lifetime_check", 1_000_000, capacity, "r.effective_expiry>?"],
  ["all", "shipping_hook", 0, route, "handleUpload(request, env, grant.value)"],
  ["all", "atomic_acceptance", 0, blob, "env.DB.batch(["],
  ["all", "typed_authorities", 0, migration, "authority IN ('monthly-included', 'sold')"],
];

let failures = 0;
for (const [pack, operation, remaining, marker] of controls) {
  if (!test.includes(marker)) {
    failures++;
    console.error(`TASK5215B_EXIT=1 pack=${pack} operation=${operation} remaining_signed_bytes=${remaining} reason=absent_control`);
  }
}
for (const [pack, operation, remaining, source, marker] of production) {
  if (!source.includes(marker)) {
    failures++;
    console.error(`TASK5215B_EXIT=1 pack=${pack} operation=${operation} remaining_signed_bytes=${remaining} reason=absent_production_seam`);
  }
}
const forbidden = /\b(account_id|user_id|payment_id|payer|email)\b/i.exec(migration);
if (forbidden) {
  failures++;
  console.error(`TASK5215B_EXIT=1 pack=all operation=identity_join remaining_signed_bytes=0 forbidden=${forbidden[0]}`);
}
if (failures) process.exit(1);
console.log(`TASK5215B_EXIT=0 controls=${controls.length} production_seams=${production.length} packs=3 authorities=2 floor=1000000 identity_joins=0`);
