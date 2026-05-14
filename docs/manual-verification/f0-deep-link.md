# F0 deep-link verification matrix

Manual verification checklist for the Phase 9-F0 fresh-install pipeline
(F0-FIX1: bootstrap mkdir + login-route bail; F0-FIX2: snowflake-driven
identity auto-generation; F0-FIX3: login-shell gate before snowflake
single-shot). Five end-to-end scenarios. Run all five before promoting
an F0-touching build to ship.

Set `$env:OSL_TRACE = "1"` and open DevTools (Ctrl+Shift+I, then enter
`window.__OSL_TRACE__ = true`) before launching to surface the
F0-FIX3-TRACE breadcrumbs that drive the "Verify in logs" rows.

## Scenario 1 — clean install, logged in already

**Preconditions:**

- `%APPDATA%\osl\` does not exist.
- A previous browser session left `discord.com` cookies so OSL's
  embedded Discord already routes to `/channels/@me`.

**Steps:**

1. Launch OSL.
2. Click through the tour to completion (password, recovery phrase).
3. Send one message in a DM after the tour.

**Expected:**

- Tour shows all slides in order, no `Couldn't save change to disk`
  banner.
- `%APPDATA%\osl\identity.json` exists after the password slide.
- `%APPDATA%\osl\peer_map.json` exists after the message send.
- About page shows your own user_id.

**Verify in logs:** `cmd_osl_register_self_snowflake entered`,
`F0-FIX2 auto-gen path entered`, `identity saved successfully`,
`registration succeeded`.

## Scenario 2 — clean install, logged-out Discord on launch

**Preconditions:**

- `%APPDATA%\osl\` does not exist.
- Discord cookies cleared so OSL boots into the inline login form
  at `/app`.

**Steps:**

1. Launch OSL. Verify the login form renders cleanly (no overlapping
   inputs, working pointer hit-tests, no canary banner).
2. Sign in to Discord inside the OSL window.
3. Wait for the channels shell to mount, then complete the tour.

**Expected:**

- Login form is interactive (F0-FIX1's wrapper-interference
  regression does not recur even with the IIFE bail removed).
- Tour does NOT advance past slide 1 until shell is mounted.
- After login, `oslTourWaitForLoggedIn` resolves, snowflake bootstrap
  fires once, identity is generated.

**Verify in logs:** `snowflake bootstrap entered` appears AFTER the
shell mounts (not at boot.js install time); single `snowflake
bootstrap: shell ready, proceeding`.

## Scenario 3 — login mid-tour

**Preconditions:** Same as Scenario 2.

**Steps:**

1. Launch OSL into the logged-out login page.
2. Open the tour (it should be on slide 1, login-waiting state).
3. Sign in to Discord.
4. Complete the tour.

**Expected:**

- Tour transitions out of login-waiting state within ~2s of shell
  mount.
- No double registration: `registration succeeded` appears exactly
  once.

**Verify in logs:** `extracted snowflake=<digits>` followed by a
single `registration succeeded`.

## Scenario 4 — existing install, fresh launch

**Preconditions:**

- `%APPDATA%\osl\` contains a valid `identity.json`,
  `peer_map.json`, `app_preferences.json` from a prior session.
- Discord is logged in (same account as the existing identity).

**Steps:**

1. Launch OSL.
2. Unlock the password gate.
3. Open a DM that previously had encrypted history.

**Expected:**

- Password gate unlocks first try. No re-tour.
- Post-unlock state reload runs (FIX-D2): peer_map / whitelist /
  preferences re-read with the unwrapped storage key.
- Decrypted history renders.

**Verify in logs:** `snowflake bootstrap: already registered (<id>)`
on the F0-FIX3 path; no auto-gen branch entered.

## Scenario 5 — account switch refusal

**Preconditions:**

- `%APPDATA%\osl\` has an `identity.json` bound to snowflake A.
- Sign out of Discord in OSL, then sign back in as a different
  account (snowflake B).

**Steps:**

1. After signing in as account B, watch the OSL console.

**Expected:**

- `cmd_osl_register_self_snowflake` returns
  `snowflake mismatch / refusing to retag`.
- Identity on disk is unchanged.
- The "OSL is bound to a different Discord account" banner appears
  in the main webview.

**Verify in logs:** `extracted snowflake=<B>` followed by
`registration failed: ... snowflake mismatch ...`.
