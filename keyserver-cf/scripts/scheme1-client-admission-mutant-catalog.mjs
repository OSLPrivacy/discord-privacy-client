export const SCHEME1_CLIENT_ADMISSION_MUTANTS = Object.freeze([
  Object.freeze({
    name: "unsigned-client-evidence",
    begin: "// SCHEME1_CLIENT_SIGNATURE_MUTATION_BEGIN",
    end: "// SCHEME1_CLIENT_SIGNATURE_MUTATION_END",
    replacement: `if (signature.length !== 64) {
    throw new Error("scheme-1 client evidence signature is invalid");
  }`,
    expectedFailure:
      "rejects stale, replay-floor, wrong-epoch, wrong-challenge, and noncanonical signed evidence",
  }),
  Object.freeze({
    name: "fabricated-subreceipts",
    begin: "// SCHEME1_CLIENT_RECEIPT_DIGEST_MUTATION_BEGIN",
    end: "// SCHEME1_CLIENT_RECEIPT_DIGEST_MUTATION_END",
    replacement: "void payload;",
    expectedFailure:
      "rejects receipt-digest substitution and cross-class witness reuse",
  }),
  Object.freeze({
    name: "co-mutated-source-expectations",
    begin: "// SCHEME1_CLIENT_SOURCE_BINDING_MUTATION_BEGIN",
    end: "// SCHEME1_CLIENT_SOURCE_BINDING_MUTATION_END",
    replacement: "void exactSourceFiles;",
    expectedFailure:
      "recomputes source closure so co-mutated receipt expectations cannot pass",
  }),
  Object.freeze({
    name: "successor-contract-drift",
    begin: "// SCHEME1_FROZEN_SOURCE_CONTRACT_MUTATION_BEGIN",
    end: "// SCHEME1_FROZEN_SOURCE_CONTRACT_MUTATION_END",
    replacement: `if (
      frozenBytes.length === 0 ||
      deploymentBytes.length === 0
    ) {
      throw new Error(
        \`scheme-1 frozen source contract drift: \${sourcePath}\`,
      );
    }`,
    expectedFailure:
      "refuses an empty or substituted source and frozen-fixture closure",
  }),
]);
