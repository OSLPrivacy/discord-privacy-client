import { execFileSync, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error - plain-JS operator command, imported for its pure helpers.
import {
  PAGE_NAME_CHECK,
  auditXRowAuthorSignals,
  renderAudit,
} from "./x-row-author-signal-audit.mjs";

const SCRIPT = join(process.cwd(), "scripts/x-row-author-signal-audit.mjs");
const FIXTURE = join(
  process.cwd(),
  "scripts/fixtures/task-4081/x-open-dm-author-signals.html",
);
const LADDER = join(process.cwd(), "ROW-OWNERSHIP-PROOF-LADDER.json");
const RENAMED_ROW_FIXTURE = join(
  process.cwd(),
  "scripts/fixtures/task-4081/x-renamed-row-page-name.html",
);

describe("TASK 4081 X row author signal audit", () => {
  it("checks all seven requested places and names the winning signal on the 4071 ladder", () => {
    const html = readFileSync(FIXTURE, "utf8");
    const audit = auditXRowAuthorSignals({ html, limit: 10 });
    const rendered = renderAudit(audit, {
      readDate: "2026-08-07",
      ladderPath: LADDER,
    });

    expect(audit.placesChecked).toBe(7);
    expect(audit.placesUnchecked).toBe(0);
    expect(audit.rowFindings).toHaveLength(10);
    expect(rendered).toContain("x_row_author_places=7");
    expect(rendered).toContain("x_row_author_places_unchecked=0");
    expect(rendered).toContain(
      "x_row_author_winner=per_row_identifier location=article[data-testid=\"conversationMessage\"]#x-sr-1[data-row-index=\"1\"]@data-sender-id ladder_kind=stable_provider_account_id_cross_check ladder_rank=2 strength=strong allowed_to_mark=true",
    );
    expect(rendered).toContain("x_place id=test_name_on_row checked=10 found=conversationMessage account_id_found=0");
    expect(rendered).toContain("x_place id=per_row_identifier checked=10 found_attr=data-sender-id numeric_account_ids=10");
    expect(rendered).toContain("x_place id=picture_address checked=10 pictures=10 account_number_in_url=10");
    expect(rendered).toContain("x_claims_without_measurement=0");
  });

  it("direct command prints measured rows and page-name failure behavior", () => {
    const output = execFileSync(
      "node",
      [
        SCRIPT,
        "--file",
        FIXTURE,
        "--limit",
        "10",
        "--read-date",
        "2026-08-07",
        "--ladder",
        LADDER,
      ],
      { encoding: "utf8" },
    );

    expect(output).toContain(`${PAGE_NAME_CHECK}=pass expected=conversationMessage rows=10`);
    expect(output).toContain(
      `x_when_page_names_change=${PAGE_NAME_CHECK} fails closed before author evidence is accepted`,
    );
    expect(output.match(/^x_row /gm)).toHaveLength(10);
  });

  it("fails closed when X changes the row page name", () => {
    const result = spawnSync(
      "node",
      [
        SCRIPT,
        "--file",
        RENAMED_ROW_FIXTURE,
        "--limit",
        "10",
        "--read-date",
        "2026-08-07",
      ],
      { encoding: "utf8" },
    );

    expect(result.status).toBe(2);
    expect(result.stderr).toContain(
      `${PAGE_NAME_CHECK}=fail expected=conversationMessage found=cellInnerDiv,dmMessage`,
    );
  });
});
