# Branch protection setup (manual GitHub UI step)

CI runs the full quality gate (`rust-test.yml` — fmt, clippy, tests,
node parse, capability audit) on every push and PR. To make those
checks merge-blocking on `main`, configure branch protection once
in the GitHub UI. The CLI route is also supported via
`gh api repos/:owner/:repo/branches/main/protection` but the UI
checklist below is what most contributors will follow.

## One-time setup

1. Open the repo on GitHub → **Settings** → **Branches**.
2. Under "Branch protection rules" click **Add rule**.
3. **Branch name pattern**: `main`
4. Enable:
   - **Require a pull request before merging**
   - **Require status checks to pass before merging**
     - Click **Add checks** and pick:
       - `test` (the rust-test workflow's main job)
       - `quality-checks` (the workflow's boot.js + capability-audit job)
     - **Require branches to be up to date before merging**
   - **Do not allow bypassing the above settings** (so admins can't
     accidentally skip CI in a hurry)
5. **Save changes**.

## Verifying

Open any PR. Both required checks should appear in the merge box.
The **Merge** button is disabled until both pass.

If a check name appears in the UI as `test` but doesn't actually
gate the merge, GitHub may be matching a stale check from an
earlier workflow version. Re-add it from the dropdown after the
next CI run.

## When to update

Add new required checks here every time you add a new CI job in
`.github/workflows/`. Existing rules don't auto-enroll new jobs.
