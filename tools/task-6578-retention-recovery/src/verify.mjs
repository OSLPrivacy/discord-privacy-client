import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { FAILURE_EDGES, MUTANTS, PROMISE, runRecoveryCampaign, runTerminalCampaign } from "./model.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");

function fail(message) { throw new Error(message); }

function verifyPromise(mutant) {
  const surfaces = [
    "apps/osl-hub-ui/src/feature-claims.ts",
    "docs/operations/retention-cleanup.md",
    "crates/english-catalogue/catalogues/en-US.v1.json",
  ];
  let exact = 0;
  let combined = "";
  for (const surface of surfaces) {
    const text = readFileSync(resolve(ROOT, surface), "utf8");
    if (!text.includes(PROMISE)) fail(`promise exact unattended string absent surface=${surface}`);
    exact += text.split(PROMISE).length - 1;
    combined += `\n${text.split(PROMISE).join("")}`;
  }
  if (mutant?.startsWith("promise_")) {
    combined += `\nA monitored team promises a ${mutant.split("_")[1]} minute human response time.`;
  }
  const forbidden = /\bon[- ]call\b|monitored team|reaction time|\b(?:5|10|15)[ -]?minute\b[^.\n]*(?:human|response)|staffed team/iu.exec(combined);
  if (forbidden) fail(`forbidden words=${forbidden[0]}`);
  return { surfaces: surfaces.length, exact };
}

async function verifyOne(mutant = null, starved = null) {
  const campaign = await runRecoveryCampaign({ mutant, starved });
  for (const object of campaign.oracleObjects) {
    if (object.policy === "free_7d" && object.dueAt - object.createdAt !== 7 * 86_400) {
      fail(`object=${object.id} failure=free_boundary recovery_edge=provider_time`);
    }
    if (object.policy === "pro_30d" && object.dueAt - object.createdAt !== 30 * 86_400) {
      fail(`object=${object.id} failure=pro_boundary recovery_edge=provider_time`);
    }
    if (object.policy === "downgraded_pro_7d" && object.dueAt !== Math.min(object.originalExpiresAt, object.downgradedAt + 7 * 86_400)) {
      fail(`object=${object.id} failure=downgrade_boundary recovery_edge=provider_time`);
    }
    if (object.policy === "backup_cleanup" && object.dueAt - object.backupCompletedAt !== 30 * 86_400) {
      fail(`object=${object.id} failure=backup_boundary recovery_edge=provider_time`);
    }
  }
  for (const id of campaign.expectedDue) {
    if (campaign.provider.exists(id)) fail(`object=${id} failure=still_due recovery_edge=${mutant || "green"}`);
    const calls = campaign.provider.deleteCalls.get(id) || 0;
    if (calls !== 1) fail(`object=${id} failure=delete_count_${calls} recovery_edge=exactly_once`);
  }
  for (const id of campaign.preserve) {
    if (!campaign.provider.exists(id)) fail(`object=${id} failure=not_due_deleted recovery_edge=provider_time`);
    if ((campaign.provider.deleteCalls.get(id) || 0) !== 0) fail(`object=${id} failure=not_due_delete_call recovery_edge=provider_time`);
  }
  for (const edge of FAILURE_EDGES) {
    if (!campaign.observedEdges.has(edge)) fail(`absent starvation failure=${edge}`);
  }
  const terminal = await runTerminalCampaign({ mutant });
  if (terminal.telegram.length !== 1) fail(`report=terminal Telegram count=${terminal.telegram.length}`);
  const text = terminal.telegram[0].text;
  for (const field of ["policy=pro_30d", "object=terminal-object", "oldest_due_item=terminal-object", "attempts=3", "terminal_reason=policy_lock_permanent"]) {
    if (!text.includes(field)) fail(`report=terminal missing_field=${field}`);
  }
  const promise = verifyPromise(mutant);
  return { campaign, terminal, promise };
}

export async function verify6579({ mutant = null, starved = null } = {}) {
  if (mutant) return verifyOne(mutant, starved);
  const green = await verifyOne(null, starved);
  const killed = starved?.startsWith("mutant:") ? starved.slice(7) : null;
  const detected = [];
  for (const candidate of MUTANTS) {
    if (candidate === killed) continue;
    try { await verifyOne(candidate); }
    catch (error) { detected.push({ candidate, reason: error.message }); continue; }
    fail(`mutant=${candidate} failure=not_detected`);
  }
  for (const candidate of MUTANTS) {
    if (!detected.some(item => item.candidate === candidate)) {
      fail(`absent starvation mutant=${candidate}`);
    }
  }
  return { ...green, mutants: detected };
}
