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
       - The `test` check from the **Rust Test** workflow. This is
         mandatory. Do not substitute the TypeScript workflow's `test`
         job or any stale unqualified `test` context.
       - The `quality-checks` check from the **Rust Test** workflow.
     - **Require branches to be up to date before merging**
   - **Do not allow bypassing the above settings** (so admins can't
     accidentally skip CI in a hurry)
   - Include administrators in the rule. Admin bypass is a refusal, not an
     emergency permission.
5. **Save changes**.

## Verifying

Verify the live `main` rule with the GitHub API, not only the
merge box:

```sh
gh api repos/OSLPrivacy/discord-privacy-client/branches/main/protection \
  --jq '.required_status_checks.contexts[], .enforce_admins.enabled'
```

The output must include both required contexts, plus admin
enforcement:

```text
test
quality-checks
true
```

Then open any PR. Both required checks should appear in the merge
box, and the **Merge** button is disabled until both pass.

If a check name appears in the UI as `test` but doesn't actually gate the
merge, GitHub may be matching a stale check from an earlier workflow version
or the TypeScript workflow's job with the same name. Re-add the `test` check
whose source is **Rust Test** after the next Rust Test run.

The equivalent API contract is:

```json
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "test",
      "quality-checks"
    ],
    "checks": [
      {
        "context": "test",
        "workflow": "Rust Test"
      },
      {
        "context": "quality-checks",
        "workflow": "Rust Test"
      }
    ]
  },
  "enforce_admins": true,
  "admin_bypass": "forbidden"
}
```

When configuring by API, verify the `test` context came from the Rust Test
workflow run. Absence of that Rust source means refusal, not permission.

Source contract checks for this file:

```sh
grep -F "Rust Test" .github/BRANCH_PROTECTION.md
grep -F "test" .github/BRANCH_PROTECTION.md
grep -F "quality-checks" .github/BRANCH_PROTECTION.md
grep -F "Do not allow bypassing the above settings" .github/BRANCH_PROTECTION.md
grep -F "Admin bypass is a refusal" .github/BRANCH_PROTECTION.md
```

## When to update

Add new required checks here every time you add a new CI job in
`.github/workflows/`. Existing rules don't auto-enroll new jobs.
