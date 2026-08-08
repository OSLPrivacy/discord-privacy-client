import { describe, expect, it } from "vitest";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import {
  OSL_ENCLAVE_ROLE_ABILITY_REASONS,
  oslEnclaveRoleAbilityCounts,
  oslEnclaveRoleAbilityRowsWithoutReason,
  oslEnclaveRoleAbilityViewMarkup,
  type OslEnclaveRoleAbilityModel,
} from "./osl-enclave-role-ability-view";
import rows from "./fixtures/task-4858-role-ability-rows.json" with { type: "json" };

const model = rows as OslEnclaveRoleAbilityModel;

const EXPECTED_ROWS = 40;
const EXPECTED_ALLOWED = 17;
const EXPECTED_DENIED = 23;

function screenshotHtml(markup: string): string {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"/><title>TASK 4858 Role ability</title><style>${styles.replaceAll("</style", "<\\/style")}</style><style>html,body,#app{width:100%;height:100%;margin:0;background:#080c0d}</style></head><body><div id="app">${markup}</div></body></html>`;
}

const FIXTURE_DIR = path.resolve("screenshots/artifacts");
const FIXTURE_PATH = path.join(FIXTURE_DIR, "task-4858-role-ability-fixture.html");

function drawnRows(markup: string) {
  return markup
    .split('<li class="osl-role-ability-row"')
    .slice(1)
    .map((chunk) => {
      const body = chunk.slice(0, chunk.indexOf("</li>"));
      return {
        permission: /data-permission="([^"]*)"/u.exec(body)?.[1] ?? "",
        state: /data-state="([^"]*)"/u.exec(body)?.[1] ?? "",
        reasonText: (
          /<p class="osl-role-ability-reason">([\s\S]*?)<\/p>/u.exec(body)?.[1] ?? ""
        ).trim(),
      };
    });
}

describe("TASK 4858 enclave role ability view", () => {
  it("draws 40 catalogue rows as 17 allowed and 23 denied", () => {
    const markup = oslEnclaveRoleAbilityViewMarkup(model);
    const drawn = drawnRows(markup);

    expect(drawn).toHaveLength(EXPECTED_ROWS);
    expect(drawn.filter((row) => row.state === "allowed")).toHaveLength(EXPECTED_ALLOWED);
    expect(drawn.filter((row) => row.state === "denied")).toHaveLength(EXPECTED_DENIED);
    expect(oslEnclaveRoleAbilityCounts(model)).toEqual({
      rows: EXPECTED_ROWS,
      allowed: EXPECTED_ALLOWED,
      denied: EXPECTED_DENIED,
    });

    mkdirSync(FIXTURE_DIR, { recursive: true });
    writeFileSync(FIXTURE_PATH, screenshotHtml(markup));

    console.log(`TASK4858_UI_ROWS=${drawn.length}`);
    console.log(`TASK4858_UI_ALLOWED=${EXPECTED_ALLOWED}`);
    console.log(`TASK4858_UI_DENIED=${EXPECTED_DENIED}`);
    console.log(`TASK4858_UI_FIXTURE_PATH=${path.relative(process.cwd(), FIXTURE_PATH)}`);
  });

  it("shows every denied row a plain reason, including the five named ones", () => {
    const markup = oslEnclaveRoleAbilityViewMarkup(model);
    const denied = drawnRows(markup).filter((row) => row.state === "denied");

    expect(oslEnclaveRoleAbilityRowsWithoutReason(model)).toEqual([]);
    expect(denied.filter((row) => row.reasonText.length === 0)).toEqual([]);

    for (const reason of OSL_ENCLAVE_ROLE_ABILITY_REASONS) {
      const count = denied.filter((row) => row.reasonText.includes(reason)).length;
      console.log(`TASK4858_UI_REASON_COUNT reason="${reason}" denied_rows=${count}`);
      expect(count).toBeGreaterThan(0);
      expect(markup).toContain(`Denied: ${reason}`);
    }
  });

  it("names an unexplained denied row instead of drawing a bare Denied", () => {
    // Strip the reason text off one denied row, exactly as the break-it check
    // does, and prove the screen refuses to explain it away.
    const stripped: OslEnclaveRoleAbilityModel = {
      ...model,
      rows: model.rows.map((row) =>
        row.permission === "rotate_keys" ? { ...row, reason: "", detail: "" } : row,
      ),
    };

    expect(oslEnclaveRoleAbilityRowsWithoutReason(stripped)).toEqual(["rotate_keys"]);

    const markup = oslEnclaveRoleAbilityViewMarkup(stripped);
    const row = drawnRows(markup).find((entry) => entry.permission === "rotate_keys");
    expect(row?.state).toBe("denied");
    expect(row?.reasonText).toBe("");
    console.log("TASK4858_UI_STRIPPED_ROW permission=rotate_keys reason_text_length=0");
  });

  it("keeps allowed rows free of denial reasons", () => {
    const markup = oslEnclaveRoleAbilityViewMarkup(model);
    for (const row of drawnRows(markup).filter((entry) => entry.state === "allowed")) {
      expect(row.reasonText).toBe("");
    }
    expect(markup).toContain("This role can do this here.");
  });
});
