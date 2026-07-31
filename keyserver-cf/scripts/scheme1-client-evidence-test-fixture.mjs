const ZERO40 = "0".repeat(40);
const ZERO64 = "0".repeat(64);

export function repetitiveClientEvidenceMutations(positive) {
  return [
    mutation("missing-top", positive, (payload) => {
      delete payload.run;
    }),
    mutation("extra-top", positive, (payload) => {
      payload.unexpected = true;
    }),
    mutation("zero-client-commit", positive, (payload) => {
      payload.client.commit = ZERO40;
    }),
    mutation("zero-client-repository-tree", positive, (payload) => {
      payload.client.repository_tree = ZERO40;
    }),
    mutation("wrong-descriptor-sha256", positive, (payload) => {
      payload.contract.descriptor_sha256 = differentHex(
        payload.contract.descriptor_sha256,
        64,
      );
    }),
    mutation("wrong-fixture-sha256", positive, (payload) => {
      payload.contract.fixture_sha256 = differentHex(
        payload.contract.fixture_sha256,
        64,
      );
    }),
    mutation("empty-cross", positive, (payload) => {
      payload.cross_language_receipt.cases = [];
      payload.cross_language_receipt.case_count = 0;
    }),
    mutation("duplicate-id-cross", positive, (payload) => {
      payload.cross_language_receipt.cases[1].id =
        payload.cross_language_receipt.cases[0].id;
    }),
    mutation("duplicate-witness-cross", positive, (payload) => {
      payload.cross_language_receipt.cases[1].witness_sha256 =
        payload.cross_language_receipt.cases[0].witness_sha256;
    }),
    mutation("noncanonical-cross-order", positive, (payload) => {
      payload.cross_language_receipt.cases.reverse();
    }),
    mutation("empty-restart", positive, (payload) => {
      payload.restart_receipt.cases = [];
    }),
    mutation("duplicate-restart", positive, (payload) => {
      payload.restart_receipt.cases[1].id = payload.restart_receipt.cases[0].id;
    }),
    mutation("identical-processes", positive, (payload) => {
      payload.restart_receipt.post_process_sha256 =
        payload.restart_receipt.pre_process_sha256;
    }),
    mutation("scheme0", positive, (payload) => {
      payload.restart_receipt.identity_scheme = 0;
    }),
    mutation("generation0", positive, (payload) => {
      payload.restart_receipt.lifecycle_generation = 0;
    }),
    mutation("noncanonical-batch", positive, (payload) => {
      payload.restart_receipt.generation_batch =
        payload.restart_receipt.generation_batch.replace(/=$/, "");
    }),
    mutation("zero-batch", positive, (payload) => {
      payload.restart_receipt.generation_batch =
        Buffer.alloc(32).toString("base64");
    }),
    mutation("empty-downgrade", positive, (payload) => {
      payload.downgrade_receipt.cases = [];
    }),
    mutation("duplicate-downgrade", positive, (payload) => {
      payload.downgrade_receipt.cases[1].id =
        payload.downgrade_receipt.cases[0].id;
    }),
    mutation("accepted-disposition", positive, (payload) => {
      payload.downgrade_receipt.cases[0].disposition = "accepted";
    }),
    mutation("zero-downgrade-witness", positive, (payload) => {
      payload.downgrade_receipt.cases[0].witness = ZERO64;
    }),
    mutation("duplicate-downgrade-witness", positive, (payload) => {
      payload.downgrade_receipt.cases[1].witness =
        payload.downgrade_receipt.cases[0].witness;
    }),
    mutation("nonzero-exit", positive, (payload) => {
      payload.run.exit_code = 1;
    }),
    mutation("zero-count", positive, (payload) => {
      payload.run.test_count = 0;
    }),
    mutation("empty-argv", positive, (payload) => {
      payload.run.command_argv = [];
    }),
    mutation("finish-before-start", positive, (payload) => {
      payload.run.started_at = "2030-01-01T00:00:00.000Z";
      payload.run.finished_at = "2029-01-01T00:00:00.000Z";
    }),
  ];
}

function mutation(name, positive, mutate) {
  const payload = deepClone(positive);
  mutate(payload);
  return { name, payload };
}

function deepClone(value) {
  return JSON.parse(JSON.stringify(value));
}

function differentHex(current, length) {
  const preferred = "f".repeat(length);
  return current === preferred ? "e".repeat(length) : preferred;
}
