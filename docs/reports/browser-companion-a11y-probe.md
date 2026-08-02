# Browser-companion accessibility probe

**Task:** T4-E4
**Status:** not measured — `S2-A11Y-VIABLE` and `S3-L1-ONLY` remain unproven.

`BrowserAccountMode::IsolatedOsl` already passes
`--force-renderer-accessibility=complete` only when it creates a Chromium
profile.  `ExistingBrowser` deliberately omits the switch, since it cannot
retrofit renderer flags onto an already-running user browser.  The unit test
`isolated_browser_accessibility_is_enabled_without_flagging_existing_browsers`
guards that source-level separation, including the specified sabotage of
removing the isolated Chromium flag.

That is not the live proof required by `WEB-E4`.  It cannot establish whether
the renderer tree is actually present or absent in a particular Windows browser
build.  The requested Windows VM with UI Automation and a Chromium browser is
not available in this Linux authoring environment, so no result is claimed.

## Required live run

On a Windows VM:

1. Start Chromium with a normal user profile before OSL starts.
2. Launch a companion service using `BrowserAccountMode::ExistingBrowser` and
   use UI Automation to enumerate its renderer tree. Record that the expected
   document sentinel is absent.
3. Launch the same fixed local document via `BrowserAccountMode::IsolatedOsl`.
   Enumerate after a warm-up pass and record that its sentinel text is present.
4. Repeat the isolated profile run in Firefox and record the actual result of
   the OSL-owned `accessibility.force_disabled` preference; do not assume it is
   equivalent to Chromium.
5. Perform the task sabotage by removing the isolated Chromium flag. The
   isolated present assertion must fail. Restore the source before committing
   the measured result.

Until those measurements are attached to this report, product copy and
substrate selection must not say that S2 is accessible or that S3's L1-only
limit has been live-proven.
