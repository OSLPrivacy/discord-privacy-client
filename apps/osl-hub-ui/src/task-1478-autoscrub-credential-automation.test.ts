import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { AUTOSCRUB_SIGN_IN_YOURSELF } from "./autoscrub-credential-boundary";
import { checkTask1478Fixture } from "./task-1478-autoscrub-credential-check";

const fixtureDirectory = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures");
const defaultFixture = path.join(fixtureDirectory, "task-1478-autoscrub-credential-data.json");
const fixturePath = process.env.TASK1478_CREDENTIAL_FIXTURE
  ? path.resolve(process.env.TASK1478_CREDENTIAL_FIXTURE)
  : defaultFixture;

describe("TASK 1478 break AutoScrub credential automation", () => {
  it("pauses a due logged-out run without reading or submitting credentials or a human check", () => {
    const proof = checkTask1478Fixture(fixturePath);

    console.info(`TASK1478_FIXTURE_PATH=${path.relative(process.cwd(), fixturePath)}`);
    console.info(`TASK1478_DUE_RUN=${proof.fixture.dueRun.runId}`);
    console.info(`TASK1478_LOGOUT=${proof.fixture.logout}`);
    console.info(`TASK1478_PASSWORD_FIXTURE=${proof.fixture.password}`);
    console.info(`TASK1478_CODE_FIXTURE=${proof.fixture.code}`);
    console.info(`TASK1478_HUMAN_CHECK_FIXTURE=${proof.fixture.humanCheck}`);
    console.info(`TASK1478_ACTIVITY=${proof.activityText}`);
    console.info(`TASK1478_CREDENTIAL_READS=${proof.credentialReads}`);
    console.info(`TASK1478_CREDENTIAL_SUBMITS=${proof.credentialSubmits}`);
    console.info(`TASK1478_HUMAN_CHECK_SUBMITS=${proof.humanCheckSubmits}`);

    expect(proof.status).toBe("paused");
    expect(proof.activityText).toBe(AUTOSCRUB_SIGN_IN_YOURSELF);
    expect(proof.renderedActivity).toContain(AUTOSCRUB_SIGN_IN_YOURSELF);
    expect(proof.credentialReads).toBe(0);
    expect(proof.credentialSubmits).toBe(0);
    expect(proof.humanCheckSubmits).toBe(0);
  });
});
