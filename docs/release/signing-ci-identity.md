# Windows code-signing CI identity

## Current status

Code signing is deferred by owner decision D63. The release pipeline ships unsigned for now, and
no Azure app registration, federated credential, or Azure credential is configured in GitHub.

## Required identity when signing resumes

Use an Entra application with GitHub Actions workload-identity federation. Its only GitHub subject
must be the exact tag pattern `repo:OSLPrivacy/discord-privacy-client:ref:refs/tags/hub-v*`.
Do not broaden this to a repository-wide subject or a branch ref: only a matching release tag may
request an Azure access token.

The release workflows are audited to reject long-lived Azure credential variables, including
`AZURE_CLIENT_SECRET`, `AZURE_CLIENT_CERTIFICATE`, `AZURE_PASSWORD`, and `ARM_CLIENT_SECRET`.
When D63 is reversed, configure `azure/login` for OIDC using non-secret tenant, subscription, and
client identifiers; do not add any exportable Azure credential to GitHub Secrets.
