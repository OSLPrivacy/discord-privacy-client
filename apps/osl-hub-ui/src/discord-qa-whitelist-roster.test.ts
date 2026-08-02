import { describe, expect, it } from "vitest";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const adapters = fs.readFileSync(new URL("./adapters.ts", import.meta.url), "utf8");
const styles = fs.readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const hubMain = fs.readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const hubCommandSurfaceModule = fs.readFileSync(
  new URL("../../osl-hub/src/hub_command_surface.rs", import.meta.url),
  "utf8",
);
const hubBroker = fs.readFileSync(new URL("../../osl-hub/src/broker.rs", import.meta.url), "utf8");
const hubSecurity = fs.readFileSync(new URL("../../osl-hub/src/security.rs", import.meta.url), "utf8");
const hubPermissions = fs.readFileSync(new URL("../../osl-hub/permissions/hub.toml", import.meta.url), "utf8");
const hubCapability = fs.readFileSync(new URL("../../osl-hub/capabilities/hub.json", import.meta.url), "utf8");
const settingsWindow = fs.readFileSync(new URL("../../../src-tauri/assets/settings_window.html", import.meta.url), "utf8");
const ipcCommands = fs.readFileSync(new URL("../../../crates/ipc/src/commands.rs", import.meta.url), "utf8");

// The authoritative `hub_tauri_commands!` list moved out of
// apps/osl-hub/src/main.rs into the library module
// apps/osl-hub/src/hub_command_surface.rs: main.rs is a `[[bin]]` with
// `required-features = ["desktop"]` that CI never compiles, so anything proven
// only there was proven by nothing. main.rs keeps the `#[tauri::command]`
// wrappers (still asserted against `hubMain` below) plus one literal handler
// list for the signal-qa shell build; registration is asserted against both.
const hubCommandSurface = ((): string => {
  const macroStart = hubCommandSurfaceModule.indexOf("macro_rules! hub_tauri_commands");
  const macroEnd = hubCommandSurfaceModule.indexOf(
    "macro_rules! hub_tauri_command_names",
    macroStart,
  );
  expect(macroStart).toBeGreaterThan(-1);
  expect(macroEnd).toBeGreaterThan(macroStart);
  const marker = "invoke_handler(tauri::generate_handler![";
  const lists: string[] = [];
  for (
    let cursor = hubMain.indexOf(marker);
    cursor >= 0;
    cursor = hubMain.indexOf(marker, cursor + 1)
  ) {
    const end = hubMain.indexOf("]);", cursor + marker.length);
    if (end < 0) continue;
    lists.push(hubMain.slice(cursor + marker.length, end));
  }
  return [hubCommandSurfaceModule.slice(macroStart, macroEnd), ...lists].join("\n");
})();

function body(startNeedle: string, endNeedle: string, text = source): string {
  const start = text.indexOf(startNeedle);
  const end = text.indexOf(endNeedle, start + startNeedle.length);
  expect(start).toBeGreaterThan(-1);
  expect(end).toBeGreaterThan(start);
  return text.slice(start, end);
}

describe("whitelist roster", () => {
  it("opens from the existing whitelist control without changing the +/- semantics", () => {
    const controls = body("function nativeDiscordHeaderControls()", "function trustedHeader()");
    expect(controls).toContain('id="discord-qa-whitelist-roster"');
    expect(controls).toContain('aria-haspopup="dialog"');
    // The approve / revoke pair keeps exactly its shipped disabled rules.
    expect(source).toContain('let whitelistRosterOpen = false;');
    expect(body("function bindWorkspace", "function showToast")).toContain('document.querySelector<HTMLButtonElement>("#discord-qa-whitelist-roster")?.addEventListener("click", () => {');
  });

  it("lists every whitelisted person, where they are trusted, and their reach", () => {
    const roster = body("function whitelistRosterMarkup()", "function nativeDiscordProtectPickerMarkup()");
    expect(roster).toContain('<dialog class="friends-dialog whitelist-roster-dialog" id="whitelist-roster-dialog" aria-labelledby="whitelist-roster-title">');
    expect(roster).toContain('id="whitelist-roster-close"');
    // Every whitelisted person, including anyone who only has scopes taken back.
    expect(roster).toContain("hubPeople.filter((person) => person.whitelistCount > 0 || person.reachNarrowedScopes.length > 0 || person.personId === activePersonId)");
    expect(roster).toContain("Nobody is whitelisted yet");

    const row = body("function whitelistRosterPersonMarkup(", "function whitelistRosterMarkup()");
    // DM / group / channel / space labels come from the shared scope labeller.
    expect(row).toContain("friendScopeLabel(scope)");
    expect(source).toContain('const kind = scope.kind === "dm" ? "Direct messages" : scope.kind === "group" ? "Group" : scope.kind === "channel" ? "Channel" : "Space";');
    expect(row).toContain("scope.userSpecific");
    expect(row).toContain("whitelistReachLine(person)");
    const reach = body("function whitelistReachLine(", "function whitelistRosterPersonMarkup(");
    expect(reach).toContain("Trusted only in the chats you approved");
    expect(reach).toContain("Reach extended to the chats you share · recorded ${reachTimestampLabel(person.reachBroadenedAt)}");
  });

  it("keeps the roster's disabled states honest", () => {
    const row = body("function whitelistRosterPersonMarkup(", "function whitelistRosterMarkup()");
    // + is disabled for a scope that is already approved.
    // - is offered only for the verified friend behind the live context.
    expect(row).toContain('data-whitelist-scope-remove="${escapeHtml(person.personId)}" data-whitelist-scope-key="${escapeHtml(scope.storageKey)}"');
    expect(row).toContain('${!isActive || busy ? "disabled" : ""}');
    // Reach cannot be widened for somebody with nothing approved yet.
    expect(row).toContain("const reachDisabled = !isActive || busy || (!person.reachBroadened && person.whitelistCount === 0 && !activeScopeApproved);");
    expect(hubSecurity).toContain("if peer.outgoing_whitelists.is_empty() && !approved_here {");
    expect(row).toContain('aria-pressed="${person.reachBroadened}"');
    expect(row).toContain("Limit reach");
    expect(row).toContain("Extend reach");
    expect(styles).toContain(".whitelist-roster-scope.narrowed .friend-scope");
  });

  it("says when approvals are not all listed instead of truncating silently", () => {
    const row = body("function whitelistRosterPersonMarkup(", "function whitelistRosterMarkup()");
    expect(row).toContain("const hiddenScopeCount = Math.max(0, person.whitelistCount - visibleScopes.length);");
    expect(row).toContain("hiddenScopeCount > 0 || person.whitelistedScopesTruncated");
    expect(row).toContain("stored locally and not listed here.");
  });

  it("changes reach and revokes one scope through their own audited commands", () => {
    const reach = body("async function toggleWhitelistRosterReach(", "async function revokeWhitelistRosterScope(");
    expect(reach).toContain("const active = activeVerifiedDiscordQaPeer();");
    expect(reach).toContain("if (!active || active.person.personId !== personId || discordQaHeaderBusy) {");
    expect(reach).toContain("await setActiveHubFriendReach(active.context.contextToken, personId, broadened)");
    expect(reach).toContain("Reach change failed closed");

    const revoke = body("async function revokeWhitelistRosterScope(", "async function toggleDiscordQaTranscriptVisibility(");
    expect(revoke).toContain("await revokeActiveHubFriendScope(active.context.contextToken, personId, storageKey)");
    expect(revoke).toContain("Revoke failed closed");

    // The ordinary approve path still sends broadened: false.
    expect(body("async function setDiscordQaWhitelistPermission(", "// Widening reach is deliberate")).toContain("enabled,\n    false,\n  );");
    expect(adapters).toContain('export async function setActiveHubFriendReach(contextToken: string, personId: string, broadened: boolean)');
    expect(adapters).toContain('export async function revokeActiveHubFriendScope(contextToken: string, personId: string, storageKey: string)');
    expect(adapters).toContain('"reachBroadened", "reachBroadenedAt", "reachNarrowedScopes"');
    expect(adapters).toContain('exact(raw, ["kind", "contextId", "storageKey", "userSpecific"])');
  });

  it("proves a revoked friend request is reflected in the Bulk Whitelist", () => {
    const friendRequestRevoke = body(
      "pub fn cmd_osl_decline_or_revoke_friend_request(",
      "/// 9-C2: boot.js pushes the user's guild-list snapshot here",
      ipcCommands,
    );
    expect(friendRequestRevoke).toContain("let accepted_grant_exists = {");
    expect(friendRequestRevoke).toContain(".any(|w| whitelist_entry_matches(w, &scope))");
    expect(friendRequestRevoke).toContain("decision: FriendRequestDecision::RevokedAcceptedGrant");
    expect(friendRequestRevoke).toContain("local_unwhitelist_apply(\n        state,\n        peer_discord_id,\n        crate::scope::ScopeInput::from(&scope),\n        revoke_broadened,\n        /* wipe_local_decrypt */ false,\n    )?;");

    const whitelistRows = body(
      "pub fn cmd_osl_list_all_whitelists(",
      "// =====================================================================\n// Phase 7d-B1",
      ipcCommands,
    );
    expect(whitelistRows).toContain("for w in &entry.outgoing_whitelists");
    expect(whitelistRows).toContain("crate::peer_map::WhitelistEntry::Dm { broadened, .. }");
    expect(whitelistRows).toContain("out.push(WhitelistRowDto {");

    const bulkApply = body(
      "pub fn cmd_osl_bulk_set_dm_whitelist(",
      "// ---- Phase 9-C3: server-wide channel-encryption defaults ----",
      ipcCommands,
    );
    expect(bulkApply).toContain("let pe = pm_guard.entry(did.clone()).or_default();");
    expect(bulkApply).toContain(".any(|w| matches!(w, crate::peer_map::WhitelistEntry::Dm { .. }))");
    expect(bulkApply).toContain("affected += 1;");

    const renderWhitelist = body("async function renderWhitelist()", "function wlToggle", settingsWindow);
    expect(renderWhitelist).toContain('const result = await oslInvoke("osl_list_all_whitelists", {});');
    expect(renderWhitelist).toContain('if (wlTab === "by_user") body.appendChild(renderWlByUser(wlData));');
    expect(renderWhitelist).toContain("else body.appendChild(renderWlByScope(wlData));");

    const bulkModal = body("function oslBulkWhitelistModal()", "// 9-C3: Server Defaults modal.", settingsWindow);
    expect(bulkModal).toContain('r = await oslInvoke("osl_bulk_set_dm_whitelist", {');
    expect(bulkModal).toContain("wlData = null;\n    renderWhitelist();");
  });

  it("keeps broadening a separate, verified, recorded backend action", () => {
    // Ordinary approval still refuses a broadened request outright.
    expect(hubBroker).toContain("if manual.person_id != requested_person_id || requested_broadened {");
    // The reach path proves the same identity, context and host as approval.
    expect(hubBroker).toContain("pub fn manual_reach_target(");
    expect(hubBroker).toContain('return Err("OSL manual peer reach target does not match the active friend".to_owned());');
    const reachCommand = body("async fn set_active_hub_friend_reach(", "/// Revoke exactly one approval", hubMain);
    expect(reachCommand).toContain("require_current_context_host(&app, &core, &broker_state, &context_token)?");
    expect(reachCommand).toContain("broker_state.manual_reach_target(&context_token, &person_id)?");
    expect(reachCommand).toContain("security::set_friend_scope_reach(");
    expect(reachCommand).toContain("&active.service_id,");
    expect(reachCommand).toContain("let _still_active = require_current_context_host(&app, &core, &broker_state, &context_token)?;");
    const revokeCommand = body("async fn revoke_active_hub_friend_scope(", "async fn get_active_hub_context_security(", hubMain);
    expect(revokeCommand).toContain("broker_state.manual_reach_target(&context_token, &person_id)?");
    expect(revokeCommand).toContain("security::revoke_friend_scope_entry(");
    // A5-F4: the revoke has to be scoped to the live context's service +
    // account, because that is what the manual grant's namespace is built from.
    expect(revokeCommand).toContain("&active.service_id,");
    expect(revokeCommand).toContain("&active.account_id,");
    expect(hubCommandSurface).toContain("            set_active_hub_friend_reach,");
    expect(hubCommandSurface).toContain("            revoke_active_hub_friend_scope,");
    expect(hubPermissions).toContain('commands.allow = ["set_active_hub_friend_reach"]');
    expect(hubPermissions).toContain('commands.allow = ["revoke_active_hub_friend_scope"]');
    expect(hubCapability).toContain('"allow-set-active-hub-friend-reach"');
    expect(hubCapability).toContain('"allow-revoke-active-hub-friend-scope"');
    // Approving a scope never writes a broadened entry, and the encrypted
    // whitelist files stay the only home for reach and its exclusions.
    expect(hubSecurity).toContain(".push(whitelist_entry(&scope, false));");
    expect(hubSecurity).toContain("reach_narrowed_scopes: BTreeMap<String, BTreeSet<String>>");
    expect(hubSecurity).toContain('write_encrypted_json(&dir.join("peer_map.json"), &peers)');
    expect(hubSecurity).toContain('write_encrypted_json(&dir.join("whitelist_state.json"), &whitelist_document)');
  });
});
