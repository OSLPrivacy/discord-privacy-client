# Browser-companion accessibility probe

**Task:** T4-E4  
**Status:** not measured — `S2-A11Y-VIABLE` and `S3-L1-ONLY` remain unproven.

`BrowserAccountMode::IsolatedOsl` supplies
`--force-renderer-accessibility=complete` only when it creates an OSL-owned
Chromium profile. `ExistingBrowser` deliberately omits it because OSL cannot
retrofit renderer flags onto an already-running user browser. The source-level
test `isolated_browser_accessibility_is_enabled_without_flagging_existing_browsers`
guards that separation, including the requested sabotage of removing the
isolated flag.

That is not `WEB-E4`: it cannot prove a renderer tree is present or absent on
a particular Windows browser build. This checkout has no Windows VM, Chromium
session, or UI Automation channel, so it makes no accessibility claim.

## Required live run

1. Start Chromium with a normal user profile before OSL starts.
2. Launch via `ExistingBrowser`; enumerate the renderer tree using UI
   Automation and capture that the document sentinel is absent.
3. Launch the same local document via `IsolatedOsl`; after a warm-up walk,
   capture that the sentinel is present.
4. Repeat the isolated-profile check with Firefox and record the actual effect
   of the OSL-owned `accessibility.force_disabled` preference.
5. Remove the isolated Chromium flag temporarily; the present assertion must
   fail, then restore it before recording the result.

Until those captures are attached, no UI or documentation may describe S2 as
accessible or S3 as live-proven L1-only.
