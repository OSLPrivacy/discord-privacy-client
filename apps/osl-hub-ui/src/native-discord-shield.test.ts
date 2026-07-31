import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const uiRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));
const shieldHtml = readFileSync(`${uiRoot}/shield.html`, "utf8");
const viteSource = readFileSync(`${uiRoot}/vite.config.ts`, "utf8");
const nativeSource = readFileSync(`${repoRoot}/apps/osl-hub/src/native_discord_overlay.rs`, "utf8");
const capability = JSON.parse(
  readFileSync(`${repoRoot}/apps/osl-hub/capabilities/native-discord-shield.json`, "utf8"),
) as { local?: boolean; webviews?: string[]; permissions?: string[] };

describe("native Discord capture shield", () => {
  it("is a bundled opaque black document with no script or remote content", () => {
    expect(shieldHtml).toContain("background: #000");
    expect(shieldHtml).not.toMatch(/<script\b/i);
    expect(shieldHtml).not.toMatch(/https?:\/\//i);
    expect(viteSource).toContain('shield: fileURLToPath(new URL("./shield.html", import.meta.url))');
  });

  it("has no IPC permissions and is scoped to only the shield webview", () => {
    expect(capability.local).toBe(true);
    expect(capability.webviews).toEqual(["native-discord-shield"]);
    expect(capability.permissions).toEqual([]);
  });

  it("cannot focus, open content, download, or outrank the protected overlay", () => {
    expect(nativeSource).toContain(".focusable(false)");
    expect(nativeSource).toContain(".skip_taskbar(true)");
    expect(nativeSource).toContain(".devtools(false)");
    expect(nativeSource).toContain("NewWindowResponse::Deny");
    expect(nativeSource).toContain(".on_download(|_, _| false)");
    expect(nativeSource).toContain(
      "active_ensure_carrier_stack(&window, &shield, discord_window, shielded)",
    );
    // Not on a plain geometry change: the placement is issued SWP_NOZORDER, so
    // a move or a resize cannot change the order this call corrects, and running
    // it per WM_MOVE (a blocking UI-thread round trip, a SetWindowPos, a style
    // rewrite and two DWM calls) is what made dragging lag.
    expect(nativeSource).toContain(
      "if !ready || overlay_scale_changed || composer_restored || stack_drifted {",
    );
    // (The negative half of this -- that no geometry event re-asserts the stack
    // -- is asserted Rust-side in
    // `the_protected_stack_is_re_asserted_only_when_it_actually_drifted`, which
    // strips the test module off the source first and so cannot be satisfied by
    // its own needle the way a whole-file read here would be.)
    expect(nativeSource).toContain("ensure_shield_stack(overlay, shield, shielded)");
    // The shield exists only while OSL is actually displaying decrypted text,
    // and only over the rows it is painting. A shield tied to the lock instead
    // blacked out the operator's real conversation.
    expect(nativeSource).toContain("if !shielded {");
    expect(nativeSource).toContain("let shielded = !painted_rows.is_empty();");
    expect(nativeSource).toContain("clip_capture_shield_to_painted_rows(shield, shield_rect, painted)");
    // The composer and shield move in ONE atomic batch so they can never be
    // observed apart. The count is now derived rather than literal -- 2 when a
    // shield rect exists, 1 when it does not -- because the empty-rows case
    // (every production session, since painted rows start empty) previously
    // bailed out of the batch entirely into Tauri set_size/set_position, which
    // are blocking round trips on the event-loop thread the operator's drag is
    // already holding. That was the 630ms composer follow-lag.
    expect(nativeSource).toContain(
      "BeginDeferWindowPos(if shield_rect.is_some() { 2 } else { 1 })",
    );
    // Exactly one batch call site in PRODUCTION code: a second would reintroduce
    // the split placement this replaced, and the composer and its capture shield
    // must never be observable apart. (The import on the `use` line carries no
    // paren.)
    //
    // Counted against production source only, with the `#[cfg(test)]` module
    // sliced off -- which is exactly what the note above says the Rust-side check
    // has to do, and for the same reason. This read was over the whole file and
    // started matching three the moment that module grew needles of its own: one
    // simulated batch, and the string literal inside
    // `the_drag_path_writes_through_the_same_single_batch` that pins this very
    // count. The needle was right and the scope was wrong, so nothing about the
    // batch changed here -- and the expected count stays 1, because raising it to
    // 3 would make a safety guard satisfiable by adding a test.
    const nativeProduction = nativeSource.slice(0, nativeSource.indexOf("#[cfg(test)]\nmod tests {"));
    // Not `split(...)[0]`: a marker that moves must fail loudly here rather than
    // silently hand the whole file back and restore the contamination.
    expect(nativeProduction).not.toHaveLength(0);
    expect(nativeProduction.match(/BeginDeferWindowPos\(/gu)).toHaveLength(1);
    expect(nativeSource).toContain("clear_and_hide(&app)");
  });
});
