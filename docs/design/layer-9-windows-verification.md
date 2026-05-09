# Layer 9 — Windows-host verification

Layer 9 ships the Tauri shell that loads `https://discord.com/app` in
WebView2. None of this layer can be unit-tested in WSL or any
headless CI environment: there is no headless WebView2 runner, and
`SetWindowDisplayAffinity` only takes effect against a real running
compositor. The checks below must be performed by hand on a
Windows 10 build 2004+ or Windows 11 host with WebView2 Runtime
installed.

Run from `C:\Users\<you>\projects\discord-privacy-client` in
PowerShell unless noted.

## Prerequisites

- Windows 10 build 2004+ or Windows 11.
- WebView2 Runtime (preinstalled on Windows 11; on Windows 10, install
  the Evergreen Bootstrapper from
  `https://developer.microsoft.com/microsoft-edge/webview2/`).
- Rust 1.88 toolchain (workspace pin).
- A Discord account for the login-persistence check.

## Build

```powershell
cargo build -p discord-privacy-client --release
```

`cargo check -p discord-privacy-client` should also succeed. (On WSL
this fails because of GTK / WebKit system deps that the Tauri target
pulls in via the Linux-side feature gates — that's expected and
documented since Layer 8.)

## Verification checklist

### (a) Tauri window opens at launch

Run:

```powershell
cargo run -p discord-privacy-client --release
```

A window titled "Discord Privacy Client" should appear at 1280×800,
resizable, with standard Win11 chrome (close / min / max). The
content area should briefly show a blank surface and then begin
rendering the Discord login page.

**Pass criteria:** window appears within ~3 seconds; no Tauri panic
banner, no console error referencing `frontendDist` or `tauri.conf.json`.

### (b) discord.com loads and renders correctly

After the window appears, the WebView2 client should display
`https://discord.com/app`. If you are not yet logged in, Discord
redirects to its login page; if you have a previous session the app
view should appear.

**Pass criteria:**

- The page renders without layout breakage. Sidebar, chat panel,
  member list, message composer all visible and responsive.
- Voice / video doesn't have to work in this layer, but the related
  chrome (call buttons, settings cog) should render.
- Open Discord's developer tools (Right-click → Inspect, or `F12` —
  WebView2's DevTools shortcut). The Console tab should show no
  CSP-related rejections originating from a meta-tag CSP. (Discord
  serves its own CSP via response headers; Tauri does **not** inject
  one in Layer 9 because `app.security.csp` is `null`.)

### (c) Login works and persists across restart

1. Log in to Discord through the Tauri window. Use 2FA if your
   account has it; the WebView2 environment is functionally a Chromium
   tab, so authenticator codes / SMS / email codes all work.
2. Navigate to a channel, send a message, confirm receipt on a phone
   or browser session. (This proves WebSocket gateway connectivity to
   `gateway.discord.gg`, CDN reads from `cdn.discordapp.com` /
   `media.discordapp.net`, etc. all work without Tauri interfering.)
3. Close the Tauri window completely (X button — not Ctrl+C in the
   terminal, which on debug builds may leave the WebView2 process
   orphaned).
4. Re-launch with `cargo run -p discord-privacy-client --release`.

**Pass criteria:** the second launch lands directly in the logged-in
app view without a re-login prompt. Cookies persist via WebView2's
default user-data folder at
`%LOCALAPPDATA%\org.discord-privacy.client\EBWebView`.

If login does **not** persist, inspect that folder — it should
contain `Default\Cookies` and `Local State`. If the folder is
missing, the bundle identifier in `tauri.conf.json` likely changed
between launches.

### (d) Capture-protection flag verifiable via screenshot test

This is the load-bearing security check for Layer 9. Two methods —
do at least the first; do the second if you have a separate machine.

#### Method 1: Snipping Tool / `Win+Shift+S`

1. With the Tauri window in focus, press `Win+Shift+S` and select
   "Rectangular snip". Drag a rectangle that covers the Discord chat
   panel inside the Tauri window.
2. Paste the snip into Paint (or any image viewer).

**Pass criteria:** the area inside the Tauri window appears as a
solid black rectangle (or, on some Windows builds, is omitted from
the snip entirely with the desktop showing through). The chat
content must **not** be readable in the snip.

If the chat content is readable, the affinity flag did not propagate
to the WebView2 child render surface — see "Troubleshooting" below.

#### Method 2: External capture (only if you have a second machine)

Open OBS (or any screen recorder) on the same machine and record the
desktop with the Tauri window visible. Stop, scrub the recording.
The Tauri window's chat area should be black throughout.

#### Method 3: Toggle off, re-test

In a debug build with the JS bridge accessible (or after Layer 11
ships the toggle UI), invoke `set_screenshot_protection(false)` and
repeat Method 1. The capture should now succeed and the chat content
should be readable. This proves the protection is actively
exclude-from-capturing rather than the snip just happening to fail.
*Skip this in Layer 9 verification if no JS bridge invocation path
exists yet — the on-by-default behaviour at startup is the load-
bearing test.*

#### Verifying child-HWND propagation specifically

`SetWindowDisplayAffinity` is applied to the parent HWND **and** to
every descendant via `EnumChildWindows`, then re-applied on every
WebView2 page load. If you want to confirm the descendants got the
flag, use Spy++ (ships with Visual Studio):

1. Launch Spy++ (`spyxx_amd64.exe`).
2. Find the "Discord Privacy Client" top-level window in the tree.
3. Expand it. The WebView2 children typically include
   `Chrome_WidgetWin_0` and a `Chrome_RenderWidgetHostHWND` deeper in
   the tree.
4. Right-click each → "Properties" → "Extended Styles" tab.
5. There is no Spy++ field for display-affinity directly, but the
   `tracing` log output from a debug build will show:
   ```
   INFO runtime::screenshot: visited=N succeeded=M
        screenshot protection applied to descendant HWNDs
   ```
   `N` should be ≥ 2 for a normal WebView2 tree (typically 3–6).
   `M` should equal `N` (in-process WebView2 children) or be slightly
   smaller if the render host runs cross-process — the latter is
   logged at `debug` as `first_failure = "Access is denied. ..."`.

### (e) Cookie persistence across restart

Covered as part of (c). If (c) passes, this passes.

## Troubleshooting

### Chat content shows up in the snip

- WebView2 Runtime older than build ~106 silently downgrades
  `WDA_EXCLUDEFROMCAPTURE` for the render-widget-host HWND. Update
  WebView2 Runtime via
  `https://developer.microsoft.com/microsoft-edge/webview2/`.
- Confirm Windows build is 2004+ (`winver`). Earlier builds don't
  support `WDA_EXCLUDEFROMCAPTURE` and silently fall through to
  `WDA_MONITOR` (which blacks the window out for the user too —
  visible misbehaviour).
- Look at the `tracing` output. If `visited=0`, the page-load
  callback hasn't fired yet — give the window a few seconds after
  navigation completes and re-snip.
- Check `set_screenshot_protection(true)` got called: Tauri's
  `app.setup` runs once, so any panic before that point would skip
  the apply. Look for a `tracing::warn!` with
  `screenshot protection unavailable at startup`.

### "Discord wants to update" or browser-version warnings

The `userAgent` in `tauri.conf.json` is pinned to a Chrome 130 / Edge
130 string. Discord may flag this as old in the distant future and
prompt for an update. Bump the UA string when this happens — there
is no protocol consequence, the UA is purely for client-detection.

### Window doesn't appear at all

- If `cargo run` exits silently, look for `frontendDist not found`.
  `webview/dist/index.html` must exist (placeholder is checked in;
  `npm run build` from the webview/ folder is **not** required for
  Layer 9).
- If a console error mentions `WebView2Loader.dll`, install the
  WebView2 Runtime Evergreen Bootstrapper.

### `cargo run` works but `cargo build --release` doesn't

The release profile (`profile.release-deterministic` or `release`)
strips symbols and uses LTO. If link-time fails, run without LTO
(`cargo build -p discord-privacy-client`) to surface the underlying
error before chasing release-only flags.

## What this verification does NOT cover

- Discord injection / encryption overlays (Layer 10–11).
- IPC commands invoked from the Discord page itself (Layer 10 will
  add the IPC-allowlist via `app.security.dangerousRemoteDomainIpcAccess`).
- Hardware-capture-card / HDMI-passthrough capture — `WDA_EXCLUDEFROMCAPTURE`
  cannot block downstream-of-GPU capture paths. Documented in
  `docs/THREAT_MODEL.md`.
- Cameras pointed at the screen.

## Sign-off

When all five checks pass, mark Layer 9 verified in the project
checkpoint log. The CHANGELOG entry for Layer 9 lists this doc as the
deferred verification task.
