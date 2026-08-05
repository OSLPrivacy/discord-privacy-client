# Windows code-signing CI identity

## Current status

Code signing is deferred by owner decision D63. The release pipeline ships unsigned for now, and
no Azure app registration, federated credential, or Azure credential is configured in GitHub.

"Signing" elsewhere in the release pipeline means the **minisign updater key**
(`HUB_TAURI_SIGNING_PRIVATE_KEY`), which is unrelated to the Windows publisher identity described
here. See `docs/release/code-signing-decision.md` §1.

## Required identity when signing resumes

Use an Entra application with GitHub Actions workload-identity federation. Its only GitHub subject
must be the exact tag pattern `repo:OSLPrivacy/discord-privacy-client:ref:refs/tags/hub-v*`.
Do not broaden this to a repository-wide subject or a branch ref: only a matching release tag may
request an Azure access token.

The release workflows are audited to reject long-lived Azure credential variables, including
`AZURE_CLIENT_SECRET`, `AZURE_CLIENT_CERTIFICATE`, `AZURE_PASSWORD`, and `ARM_CLIENT_SECRET`.
When D63 is reversed, configure `azure/login` for OIDC using non-secret tenant, subscription, and
client identifiers; do not add any exportable Azure credential to GitHub Secrets.

## If the route is not Azure

Azure Artifact Signing requires a paid (pay-as-you-go or EA) subscription, and its individual
identity-validation path is limited to the US and Canada — so a commercial CA with cloud signing may
be chosen instead (`docs/release/code-signing-decision.md` §4). In that case the same rules apply in a
different shape:

- The signing credential goes in the **`hub-release` GitHub environment**, which is approval-gated and
  already separate from the `hub-vm-qa` promotion environment. Never a repository-wide secret, never a
  variable, never a file in the tree.
- A **physical USB token cannot be used from GitHub-hosted runners**. Only a cloud-HSM signing service
  is viable without standing up a self-hosted runner, and a self-hosted runner holding a code-signing
  token is a materially larger attack surface than the current arrangement.
- The private key is never exported, downloaded, or held by this repository under any route. Azure's
  Authenticode certificate is *"never given to you… accessible only at the time of signing"*; a cloud
  CA's key is held in the CA's HSM. Nothing but an API credential should ever exist on our side.
- `scripts/audit_hub_release.py` must be extended to reject whatever long-lived credential shape the
  chosen vendor uses, matching the existing Azure rule at `:73`.
