import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const runbookPath = new URL("./cutting-a-release.md", import.meta.url);
const versionChecker = new URL("../../scripts/check_version_consistency.py", import.meta.url);
const changelogExtractor = new URL("../../scripts/extract_changelog_section.py", import.meta.url);

async function loadContract() {
  const document = await readFile(runbookPath, "utf8");
  const match = document.match(/```json release-runbook\n([\s\S]*?)\n```/);
  assert.ok(match, "runbook must declare its executable release contract");
  return JSON.parse(match[1]);
}

async function run(command, args, cwd) {
  return execFileAsync(command, args, { cwd, encoding: "utf8" });
}

async function writeFixture(directory, version) {
  await mkdir(join(directory, "apps/osl-hub"), { recursive: true });
  await mkdir(join(directory, "apps/osl-hub-ui"), { recursive: true });
  await writeFile(join(directory, "apps/osl-hub/tauri.conf.json"), JSON.stringify({ version }));
  await writeFile(join(directory, "apps/osl-hub/Cargo.toml"), `[package]\nversion = "${version}"\n`);
  await writeFile(join(directory, "apps/osl-hub-ui/package.json"), JSON.stringify({ version }));
  await writeFile(join(directory, "CHANGELOG.md"), `# Changelog\n\n## [${version}] - 2026-08-01\n\n- Test release.\n`);
  await run("git", ["init", "--quiet"], directory);
  await run("git", ["config", "user.email", "release-test@example.invalid"], directory);
  await run("git", ["config", "user.name", "Release Test"], directory);
  await run("git", ["add", "."], directory);
  await run("git", ["commit", "--quiet", "-m", "prepare release"], directory);
}

async function preflight(directory, contract) {
  await run("python3", [versionChecker.pathname, "--root", directory], directory);
  const config = JSON.parse(await readFile(join(directory, contract.version_files[0]), "utf8"));
  const { stdout: tag } = await run("git", ["describe", "--tags", "--exact-match"], directory);
  const actualTag = tag.trim();
  const expectedTag = `${contract.tag_prefix}${config.version}`;
  assert.equal(actualTag, expectedTag, `Tag ${actualTag} does not match OSL Privacy version tag ${expectedTag}`);
  await run("python3", [changelogExtractor.pathname, "--tag", actualTag, "--changelog", join(directory, "CHANGELOG.md")], directory);
}

test("the runbook preflight accepts a throwaway tag that matches all release versions", async (t) => {
  const contract = await loadContract();
  const directory = await mkdtemp(join(tmpdir(), "osl-release-runbook-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await writeFixture(directory, "1.2.3");
  await run("git", ["tag", "-a", "hub-v1.2.3", "-m", "OSL Privacy 1.2.3"], directory);

  await preflight(directory, contract);
});

test("the runbook preflight stops a throwaway tag that mismatches the config version", async (t) => {
  const contract = await loadContract();
  const directory = await mkdtemp(join(tmpdir(), "osl-release-runbook-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await writeFixture(directory, "1.2.3");
  await run("git", ["tag", "-a", "hub-v1.2.4", "-m", "OSL Privacy 1.2.4"], directory);

  await assert.rejects(
    preflight(directory, contract),
    /Tag hub-v1\.2\.4 does not match OSL Privacy version tag hub-v1\.2\.3/,
  );
});
