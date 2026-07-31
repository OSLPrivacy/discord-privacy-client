import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const native = readFileSync(new URL("../../osl-hub/src/scrub_imap.rs", import.meta.url), "utf8");
const commands = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");

describe("native IMAP identity and deletion authority", () => {
  it("scopes live state, auth epochs, and credential keys to the active owner", () => {
    expect(native).toContain("owner_account_key(owner, account_id)");
    expect(native).toContain("keyring_entry(owner, account_id)");
    expect(native).toContain("next_auth_epoch(owner, account_id)");
    expect(native).toContain("config_for_epoch(owner");
    expect(commands).toContain("active_unlocked_osl_user_id(&app.state::<HubCoreState>())");
  });

  it("revokes live IMAP authority across identity and burn transitions", () => {
    // Whitespace-normalized on purpose. The property under test is that each of
    // these transitions revokes ALL live IMAP authority before it returns, which
    // is a security property. A literal substring match also pinned the source
    // FORMATTING: rustfmt line-wrapped
    //   app.state::<...ScrubImapState>().revoke_all()?;
    // into two lines and this test went red while the revoke was still there.
    // Collapsing runs of whitespace keeps the gate exact about the call and
    // indifferent to how rustfmt happens to break the line.
    const collapse = (value: string): string => value.replace(/\s+/gu, "");
    const flat = collapse(commands);
    for (const name of ["switch_hub_identity", "burn_active_hub_identity", "execute_hub_full_cleanup", "burn_hub_service_account", "burn_active_hub_context"]) {
      const start = commands.indexOf(`async fn ${name}`);
      expect(start, name).toBeGreaterThanOrEqual(0);
      // Slice on the collapsed text so the 900-char budget measures code, not indentation.
      const flatStart = flat.indexOf(collapse(`async fn ${name}`));
      expect(flatStart, name).toBeGreaterThanOrEqual(0);
      expect(flat.slice(flatStart, flatStart + 900), name).toContain(
        collapse("ScrubImapState>().revoke_all()"),
      );
    }
  });

  it("keeps native deletion fail-closed while read-only operations remain available", () => {
    const start = native.indexOf("pub fn delete(");
    const end = native.indexOf("fn verify_with", start);
    const deletion = native.slice(start, end);
    expect(deletion).toContain("Native IMAP deletion is disabled");
    expect(deletion).not.toContain("delete_with(");
    expect(native).toContain("pub fn enumerate(");
    expect(native).toContain("pub fn inspect(");
    expect(native).toContain("pub fn verify(");
  });
});
