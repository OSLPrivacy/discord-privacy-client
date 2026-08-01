# Comparison dataset quarantine

## Decision

Do not publish `assets/data/apps.json` from the collaborator comparison as authored.  It
must remain absent from `data/public-surface-manifest.json`, so it is not a public claim
channel.  This is a hold on the data, not a rejection of the comparison's visual work:
the logos, layout, and `compare.css` may be reused when an evidence-backed dataset is ready.

Owner decision D68 is controlling: build the capabilities so the comparison can become true,
but no capability claim ships before the capability does.  A future comparison must use the
H7 sourcing schema and bind the OSL row to `capability_registry`; it may land individual rows
only after their capability has earned an allowlist row.

## OSL self-row: nine unshippable fields

| Field in authored row | Authored assertion | Why it cannot ship | Required recovery condition |
|---|---|---|---|
| `video_calls` | `true` | OSL has no video-calling capability. | Deliver it, record supporting evidence, then earn a registry/allowlist row. |
| `disappearing` | `true` | Expiry, view-once, and burn are `Planned`, not current capabilities. | Ship and evidence the specific disappearing behaviour claimed. |
| `group_chat` | `true` | Group protection is `Planned`; the status guidance says group/channel content is not protected. | Ship protected groups and update the capability registry. |
| `file_sharing` | `true` | File and image sending remain `Planned`; treating them as shipping would also contradict the attachment-claim restrictions. | Prove the release path and earn its status row. |
| `audit` | `"partial"` | Section D forbids audit or independent-verification claims.  No third-party cryptographic audit has been commissioned. | Do not express an audit score; any narrow future review must use the exact permitted limitation. |
| `min_metadata` | `true` | It contradicts the required limitation: OSL protects content, not metadata. | No affirmative metadata-minimisation claim without a dedicated, evidenced rule and its required limitation. |
| `cross_platform` | `true` | The current packaged path is Windows-only; multi-device import is not evidence of cross-platform support. | Ship and evidence each claimed platform. |
| `e2ee` | `true` (presented as “by default”) | Per-message encryption is Beta and opt-in for one connector; “by default” overstates that narrow capability. | Use only wording and status earned by the registry and allowlist. |
| `scores.overall`, `scores.privacy`, `scores.features` | `90`, `88`, `92` | A numeric OSL protection score on the website is forbidden; scores cannot turn unearned claims into facts. | Do not publish a website score.  Only the installed app may show a timestamped live score with its failures listed. |

## Competitor rows

The 50 non-OSL rows also stay on hold.  They make claims about named products (including audit
and metadata behaviour) but carry no publisher, URL, effective date, access date, or refresh
evidence.  A rebuilt comparison must give every displayed cell a source id and retain the H7
evidence fields (`publisher`, `url`, `published_or_effective_on`, `accessed_on`,
`refresh_evidence`, and `comparability`).  Evidence older than 90 days must fail refresh.

## Enforcement and handoff

`docs/design/comparison-dataset-quarantine.test.mjs` is the T11-T36 regression: it fails if
`assets/data/apps.json` is added to the public-surface manifest.  If a future task intentionally
introduces the dataset, it must first meet the structured-data gate in
`scripts/check-claims-data.mjs`; that gate rejects unbound public JSON, unearned OSL capability
assertions, and numeric OSL scores.

This record is the handoff for T11-J6.  The contribution is recoverable, but the authored data is
not publishable now.
