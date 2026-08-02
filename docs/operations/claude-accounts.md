# Isolated Claude accounts

`scripts/fleet/verify-claude-config-home.sh` must pass after every Claude Code
update and before account setup. It reads the installed executable’s own,
versioned `CLAUDE_CONFIG_DIR` diagnostic; it does not treat a directory or a
headless prompt as evidence of isolation.

Each launcher selects a distinct configuration home: `claude2` uses
`~/.claude2`, and `claude3` uses `~/.claude3`. Override only the corresponding
`CLAUDE2_CONFIG_HOME` or `CLAUDE3_CONFIG_HOME` for a disposable test. The
configuration root contains that account’s credentials, cache, session and
daemon writers. Neither launcher reads, copies, exports, nor accepts an OAuth
token environment variable.

After verification, authenticate each account separately in a normal browser or
device-login flow:

```bash
scripts/fleet/claude2 auth login
scripts/fleet/claude3 auth login
scripts/fleet/claude2 auth status --text
scripts/fleet/claude3 auth status --text
```

Switch by invoking the desired launcher. Log out only one account with its own
launcher (`claude2 auth logout`); confirm the other with `claude3 auth status`.
For a damaged account home, preserve it for diagnosis, create a new empty home
using that launcher’s override, verify it, and perform a fresh browser login.
After a CLI update, rerun the verifier and each `auth status`; do not migrate an
account by copying credentials, cache, or session files.
