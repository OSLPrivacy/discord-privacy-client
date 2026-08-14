import { describe, expect, it } from "vitest";
import { friendRemovalButtonMarkup, friendWideWhitelistButtonsMarkup } from "./ui-behavior";

describe("friend-wide whitelist buttons", () => {
  it("renders both Whitelist everywhere and Whitelist nowhere buttons on one friend page fixture", () => {
    const personId = "hub-person-task-0260";
    const managementFixture = `<details class="friend-management"><summary>Manage</summary><div>${friendWideWhitelistButtonsMarkup(personId, (value) => value)}${friendRemovalButtonMarkup(personId, (value) => value)}</div></details>`;

    expect(managementFixture).toContain('data-whitelist-everywhere="hub-person-task-0260"');
    expect(managementFixture).toContain('data-whitelist-nowhere="hub-person-task-0260"');
    expect(managementFixture).toContain("Whitelist everywhere");
    expect(managementFixture).toContain("Whitelist nowhere");
    expect(managementFixture).toContain('data-remove-person="hub-person-task-0260"');
  });
});
