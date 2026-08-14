import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  RECORD_FORMAT,
  parseArgs,
  runReleaseDeployLiveRevisionCheck,
} from "./release-deploy-live-revision.mjs";

function health(revision) {
  return new Response(JSON.stringify({
    ok: true,
    revision,
    build_time: `2026-08-08T00:00:0${revision === "before-0439" ? "1" : "2"}Z`,
    configuration_name: "production-test",
  }), { status: 200, headers: { "content-type": "application/json" } });
}

function options(recordPath = "/evidence/test-deploy.json") {
  return parseArgs([
    "--host", "https://keyserver.example/",
    "--record", recordPath,
    "--", "wrangler", "deploy",
  ]);
}

test("test deploy record contains two different timestamped live revision reads", async () => {
  const responses = [health("before-0439"), health("after-0441")];
  const calls = [];
  const output = await mkdtemp(join(tmpdir(), "osl-release-deploy-record-"));
  const recordPath = join(output, "test-deploy.json");
  let tick = 1_786_000_000_000;
  try {
    const record = await runReleaseDeployLiveRevisionCheck(options(recordPath), {
      fetchImpl: async (url, init) => {
        calls.push({ url, init });
        return responses.shift();
      },
      now: () => (tick += 1_000),
      spawnSyncImpl: (command, args) => {
        calls.push({ command, args });
        return { status: 0 };
      },
    });

    assert.equal(record.format, RECORD_FORMAT);
    assert.equal(record.revision_reads.length, 2);
    assert.deepEqual(record.revision_reads.map((read) => read.stage), ["before-deploy", "after-deploy"]);
    assert.deepEqual(record.revision_reads.map((read) => read.revision), ["before-0439", "after-0441"]);
    assert.notEqual(record.revision_reads[0].revision, record.revision_reads[1].revision);
    assert.notEqual(record.revision_reads[0].read_at, record.revision_reads[1].read_at);
    assert.equal(calls.filter((call) => call.url).length, 2);
    assert.deepEqual(calls.find((call) => call.command), { command: "wrangler", args: ["deploy"] });
    const persisted = JSON.parse(await readFile(recordPath, "utf8"));
    assert.deepEqual(persisted.revision_reads, record.revision_reads);
    console.log(`test-deploy-record path=${recordPath} reads=${record.revision_reads.length} before=${record.revision_reads[0].revision}@${record.revision_reads[0].read_at} after=${record.revision_reads[1].revision}@${record.revision_reads[1].read_at}`);
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});
