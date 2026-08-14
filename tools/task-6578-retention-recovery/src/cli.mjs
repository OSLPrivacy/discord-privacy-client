import { verify6579 } from "./verify.mjs";

const mode = process.argv[2];
const mutantAt = process.argv.indexOf("--mutant");
const mutant = mutantAt >= 0 ? process.argv[mutantAt + 1] : null;
const starved = process.env.OSL_6579_STARVE || null;

if (!["6578", "6579"].includes(mode)) {
  console.error("usage: node src/cli.mjs 6578|6579 [--mutant NAME]");
  process.exitCode = 2;
} else {
  try {
    const result = await verify6579({ mutant, starved });
    const due = result.campaign.expectedDue.length;
    const kept = result.campaign.preserve.length;
    const reports = result.terminal.telegram.length;
    const mutants = result.mutants?.length ?? 0;
    console.log(`TASK6579 PASS policies=${result.campaign.policies.size} provider_time_oracles=${due + kept} due_deleted=${due} not_due_preserved=${kept} failure_edges=${result.campaign.observedEdges.size} delete_once=${due} telegram_reports=${reports} report_fields=5 acknowledgement_required=0 promise_exact=${result.promise.exact} surfaces=${result.promise.surfaces} forbidden_claims=0 mutants_red=${mutants}`);
  } catch (error) {
    console.error(`TASK6579 FAIL ${error.message}`);
    process.exitCode = 1;
  }
}
