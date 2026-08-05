# Windows code-signing decision

**Decision date:** 2026-07-31 (owner decision D63)
**Re-verified and corrected:** 2026-08-04 — **two recorded facts were wrong**; see §2.

**Status:** still deferred by D63; ship unsigned. Nothing here changes what ships today. This document
exists so the owner can decide from current facts instead of from a stale blocker.

## Recommendation

Do not buy or configure a code-signing service yet. The first-year signing cost for the first-usable
release is **$0**. Accept the Windows unknown-publisher / SmartScreen warning, explain it plainly at
download time, and keep the existing minisign updater key as the build-integrity anchor. This is not a
claim that the executable is Authenticode-signed.

Re-open this decision when a release is ready to fund signing. The route then depends on one owner
answer (§5), and the vendor follows from it (§4). Whatever is chosen, prove it with a test profile
before a production release, and use GitHub OIDC or an environment-scoped credential rather than an
exportable repository secret.

---

## 1. What signs what today

Two different things are called "signing" in this repository, and conflating them has already produced
one wrong critical path. They are unrelated.

| | **Update-payload signing** — exists | **Executable signing (Authenticode)** — absent |
|---|---|---|
| Key | minisign (Ed25519) | none |
| Public key | `apps/osl-hub/tauri.conf.json:52` | — |
| Private key | GitHub secret `HUB_TAURI_SIGNING_PRIVATE_KEY` in the `hub-release` environment (`.github/workflows/osl-hub-release.yml:109-110`, `:160-161`) | — |
| What it protects | the bytes an installed client is about to self-install | nothing |
| Verified by | `tauri_plugin_updater` in the shipped binary (`apps/osl-hub/src/main.rs:9439`, `:1860-1891`); in CI by `scripts/verify_update_feed_acceptance.py` | — |
| Who sees it | nobody — invisible to the user and to Windows | — |

**There is no Authenticode path anywhere in the build or release.** `bundle.windows`
(`apps/osl-hub/tauri.conf.json:37-42`) sets only `webviewInstallMode`; neither `certificateThumbprint`
nor `signCommand` appears in either Tauri config, and no workflow invokes `signtool`.

Two traps that yield the wrong answer on a fast read:

- **The release workflow is named "Build signed OSL Privacy candidate"**, and its bundling step is
  "Build signed draft installer and updater manifest" (`.github/workflows/osl-hub-release.yml:1`,
  `:105`). Both mean *minisign*; `scripts/audit_hub_release.py` uses "signing" in the same sense
  throughout. Nothing in that pipeline touches Authenticode.
- **The string `Authenticode` does appear in the tree** — `apps/osl-hub/src/windows_executable_trust.rs:4`,
  `:142`, `:173`, and `apps/osl-hub/src/mullvad_window_host.rs:10`. That is OSL **verifying other
  people's** executables (browsers, Discord, Mullvad) before attaching to them: inbound verification,
  not self-signing. Likewise `code_signature_valid` (`apps/osl-hub/src/security.rs:105`) is the **OSL
  friend-code** signature, not a code signature.

**Consequence:** the auto-updater does not depend on Authenticode. It depends on a minisign key that
already exists plus a manifest published to
`https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-latest/latest.json`
(`apps/osl-hub/tauri.conf.json:49-51`), which `.github/workflows/osl-hub-promote.yml:96-107` uploads as
a **GitHub release asset** — not an R2 upload; `docs/release/distribution-topology.md:43-48` says
explicitly not to make an R2 bucket canonical. Code signing gates exactly one thing: *installs without
a security warning*.

---

## 2. The Azure block — the recorded root cause was wrong

Previously recorded (`16-RESOURCES.md` §5, `QUEUE.md`, `START-HERE.md`, tasklog B0-09): *the
subscription is policy-pinned to `spaincentral`, which `Microsoft.CodeSigning` does not serve; not
quota, not permissions; retrying cannot fix it.*

The conclusion (Azure is unavailable) holds. **Both stated reasons are wrong**, verified by command on
2026-08-04:

| As recorded | Verified today |
|---|---|
| "policy permits only `spaincentral`" | The `Allowed resource deployment regions` assignment on subscription `8d63ea78…` permits **five** regions: `spaincentral`, `francecentral`, **`polandcentral`**, `italynorth`, `norwayeast`. The `osl-signing` resource group is itself in `northeurope`. |
| "the one region the account may use is the one region the service does not serve" | `az provider show -n Microsoft.CodeSigning` lists **Poland Central** among the supported `codeSigningAccounts` regions, and Microsoft's own region table lists it too. **The intersection is non-empty.** The five regions tried in B0-09 (northeurope, westeurope, eastus, westus2, westcentralus) were all *outside* the policy; Poland Central was never tried. |

**The real blocker is the subscription type, and it is harder than a region.** Both subscriptions are
`AzureForStudents_2018-01-01`, `spendingLimit: On`, with a `freetier` promotion. Microsoft's FAQ
(ms.date 2026-05-14) is explicit:

> "Can I use Artifact Signing with a free, trial, or sponsored Azure subscription? **No.** Artifact
> Signing doesn't support free, trial, or sponsored Azure subscriptions. To create an Artifact Signing
> account and certificate profiles, you must have a paid Azure subscription… upgrade your subscription
> to a pay-as-you-go or enterprise agreement."

That is what `RequestDisallowedByAzure` was reporting. No region change can fix it; only a
pay-as-you-go subscription can. **Do not re-litigate the region.**

**Renamed service:** Trusted Signing is now **Azure Artifact Signing**. The resource provider is still
`Microsoft.CodeSigning`; the CLI is `az artifact-signing` (the `trustedsigning` extension was the
preview name).

**The plan's tail is understated.** Microsoft's stated identity-validation processing time is **1–20
business days**, "possibly longer if we need to request more documentation" — not the 1–3 days
`PLAN.md` r4-5 schedules against. Up to four working weeks, and adding lanes cannot compress it.

---

## 3. What signing does and does not buy

Sources: [SmartScreen reputation for Windows app developers](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
(ms.date 2026-05-04); [Artifact Signing FAQ](https://learn.microsoft.com/en-us/azure/artifact-signing/faq)
(ms.date 2026-05-14).

**Immediately, on the first signed download:**

- The **UAC elevation prompt** stops reading *Unknown Publisher* and shows the verified publisher name.
  That is a different dialog from SmartScreen — it fires on elevation, not on download.
- When SmartScreen does appear, it **names the publisher** rather than presenting an anonymous binary.

**Not immediately — this is the part that has been oversold:**

- **The SmartScreen warning does not go away.** Microsoft, for a valid OV **or EV** certificate:
  *"Warning — app flagged as unrecognized until reputation accumulates."* Reputation *"can take several
  weeks and hundreds of clean installs from a wide audience."* No threshold is published, and there is
  *"no need (or mechanism) to manually submit a file for SmartScreen reputation review for consumer
  endpoints."*
- **EV buys nothing here.** Microsoft: *"EV certificates no longer bypass SmartScreen… Paying a premium
  for EV solely to avoid SmartScreen warnings is no longer justified."* The bypass was removed in 2024.
  Artifact Signing does not issue EV certificates at all, and has no plan to.
- What signing *does* buy over time is **transferable reputation**: reputation attaches to the
  certificate as well as the file hash, so once one release earns it, later releases signed by the same
  identity can inherit it. Unsigned builds start from zero **every release** — the real compounding
  cost of staying unsigned.

**Unaffected by signing:** the minisign updater chain, the checksum list, build attestation, antivirus
detection, and every functional defect on the board. Signing changes one dialog and starts a
reputation clock.

**Two project-specific risks, before any money is spent:**

1. **Smart App Control.** On Windows 11, *"Smart App Control will block execution of unsigned files
   unless the file has a positive reputation"*, and its checks apply to all executables, not only
   downloaded ones. For SAC-enabled machines this is a hard block, not a warning — the strongest
   argument for signing eventually.
2. **Negative certificate reputation.** Microsoft: *"Do not sign potentially unwanted applications…
   or the certificate may develop negative reputation."* OSL injects global input, drives other
   applications through accessibility APIs, and enables capture protection — behaviours antivirus
   heuristics routinely score as PUA. The certificate that would accrue that reputation carries the
   owner's **legal name**, and it is not separable from him afterwards. Sign once the binary is stable
   enough that a false-positive wave is unlikely, not before.

---

## 4. Options, with current (2026-08-04) figures

| Option | Annual cost | Identity documents the owner must supply personally | Elapsed time | Warning on day 1 | Warning later |
|---|---|---|---|---|---|
| **Stay unsigned** (today) | $0 | none | — | Full "Windows protected your PC" block; *Unknown Publisher* on UAC; **blocked outright under Smart App Control** | never improves; resets every release |
| **Azure Artifact Signing, Basic** | ~US$120 (US$9.99/mo, not pro-rated; 5,000 signatures/mo, then US$0.005 each) | **A pay-as-you-go subscription must exist first.** *Individual path (US/Canada only):* government photo ID, an address document (utility bill or bank statement), live face check via AU10TIX + Microsoft Authenticator Verified ID; details are read from the Azure **billing account**, which must be of type Individual. *Organisation path (US, CA, EU, UK, AU, NZ, JP, KR, SG, CH, NO, IL):* legal entity name, business identifier, business address, website, two emails on the entity's domain, plus a named representative's ID + face check | **1–20 business days** after the subscription exists | publisher named; SmartScreen still warns | reputation accrues; transferable across releases |
| **OV/IV certificate from a commercial CA** (e.g. SSL.com Individual Validation) | US$129/yr for 1 year, down to US$96.75/yr at 5 years. Cloud signing (eSigner) is a separate subscription; a hardware token is **+US$379** one-off | Government-issued photo ID plus a successful callback to a listed phone number. **No business registration, no stated country restriction.** | 3–5 business days validation (plus shipping if a token) | identical to Artifact Signing | identical |
| **EV certificate** | ~US$290–625/yr | full legal-entity and authorised-signer verification | 1–2 weeks | **identical to OV** since 2024 | identical |
| **Microsoft Store / MSIX** | $0 (individual and company registration are both free as of 2026) | Store account identity checks | days, plus certification | **none, ever** — Microsoft re-signs | none |
| **Self-signed** | $0 | none | minutes | **identical to unsigned** | never improves |

Notes that change how those rows read:

- **Every code-signing private key must live in FIPS 140-2 L2 / CC-equivalent hardware** (CA/Browser
  Forum Code Signing Baseline Requirements, in force since 2023-06-01, unchanged in v3.11.0). In
  practice that means either a shipped USB token or a cloud-HSM signing service. Artifact Signing is
  inherently cloud (FIPS 140-3 Level 3) with no token — its main practical advantage over a CA.
- **Certificate validity is capped at 460 days** for certificates issued on or after 2026-03-01
  (CA/B ballot CSC-31), so "multi-year" purchases are re-issuance contracts, not multi-year
  certificates.
- **The Microsoft Store row is a trap for this product, not a shortcut.** It is the only option that
  removes the warning outright, but OSL self-updates through its own Tauri updater, is designed to
  fetch a separately downloadable closed-source module (AutoScrub), and automates third-party
  applications — all of which sit badly against MSIX packaging and Store policy. Treat it as
  *investigate before believing*. Submitting an EXE/MSI through the Store does **not** get it
  re-signed; only a native MSIX does.

---

## 5. The one decision the owner must make

**Everything else is blocked on one thing: whether OSL signs under a *personal legal identity* or a
*registered legal entity*.** It is not a cost decision — the two cheapest routes differ by about
US$10/year. It decides:

- **Which routes are open at all.** Azure Artifact Signing's individual path is **US/Canada only**; its
  organisation path covers the EU and UK. If the owner is not in the US or Canada and has no registered
  entity or DBA, Azure is closed regardless of subscription type, and a commercial IV certificate
  (ID + phone callback, no company) becomes the only working route.
- **Whose name is permanently embedded in every installer.** The public certificate ships inside every
  signed binary; the subject carries the validated name and can carry locality, state and country.
  Anyone who downloads OSL can read it, and revoking a later certificate does not retract what is
  already distributed. For a privacy product, publishing the author's home locality is a threat-model
  decision, not paperwork. See §8.

Vendor, region and CI wiring all follow mechanically once that is answered.

---

## 6. Ordered action list

**Only the owner can do steps 1–5.** Identity validation cannot be delegated: it needs his government
ID, his phone, and a live face check on his own device.

1. **Decide §5: personal identity or registered entity.** If entity, register it (or a DBA) first —
   nothing downstream can start until a business identifier and a domain-matching email exist.
2. **Pick the route from §4** given that answer:
   - personal identity, not US/Canada → **commercial IV certificate with cloud signing** (no company,
     no token, ~US$129/yr);
   - personal identity, US/Canada → **Azure Artifact Signing, individual path** (cheapest, no token);
   - registered entity → **Azure Artifact Signing, organisation path**.
3. **If and only if the route is Azure:** move to a **pay-as-you-go** subscription. Choose the
   permanent one now — Artifact Signing resources cannot be migrated between subscriptions, tenants or
   resource groups afterwards. Do **not** retry on either `Azure for Students` subscription; it fails
   with `RequestDisallowedByAzure` in every region. Create the account in **`polandcentral`**
   (policy-allowed and service-supported) or amend the allowed-locations policy first.
4. **Submit identity validation and complete it personally.** Budget **1–20 business days**. A
   submitted request cannot be edited — a mistake means creating a new one — and extra-document
   requests are capped at three attempts.
5. **Review the certificate-subject preview before anything is issued**, and leave the optional street
   address and postal code checkboxes off.
6. — automatable from here —
7. **Wire signing into the pipeline** (§7). Prove it first against a **Public Trust Test** profile, or
   the CA's test certificate, so no production release is cut against an unproven signing step.
8. **Fix the reproducible-build comparison before the first signed release** (§7), or that job starts
   failing on every signed build for a reason that looks exactly like tampering.
9. **Update `docs/download.html` and `docs/release/verify-your-download.md`**, which currently state
   plainly that OSL is not Authenticode-signed. Both become wrong the moment step 7 lands.
10. **Tell early users the warning will persist.** Reputation takes weeks and hundreds of installs; the
    friends round will still see SmartScreen. Do not promise otherwise.

---

## 7. How it wires in, and the two things it touches

**Where the signature is produced.** Set `bundle.windows.signCommand` in
`apps/osl-hub/tauri.conf.json` (or `certificateThumbprint` for a locally installed certificate). It
must happen **inside bundling**, not as a post-processing step:

> `bundle.createUpdaterArtifacts` is `true` (`apps/osl-hub/tauri.conf.json:36`), so Tauri computes the
> **minisign signature over the installer bytes**. Authenticode-signing the installer *after* bundling
> changes those bytes, so the shipped `.sig`, the `latest.json` signature and `SHA256SUMS.txt` would
> all describe a file that no longer exists, and every client would refuse the update.
> `scripts/verify_update_feed_acceptance.py` would catch it — but only after a wasted release cycle.
> Sign during bundling, so minisign signs already-Authenticode-signed bytes.

**Where the credentials live.** Never in the tree. For Azure, `docs/release/signing-ci-identity.md`
already fixes the shape: an Entra application with GitHub workload-identity federation whose only
subject is `repo:OSLPrivacy/discord-privacy-client:ref:refs/tags/hub-v*`, used through `azure/login`
with non-secret tenant/subscription/client identifiers. `scripts/audit_hub_release.py:73` already
**fails the build** if a long-lived Azure secret appears in a release workflow. For a commercial CA's
cloud signing, the API credential belongs in the existing approval-gated `hub-release` GitHub
**environment** — never a repository-wide secret. A physical USB token cannot be used from
GitHub-hosted runners at all, which is why cloud signing is the only realistic commercial variant here.

**What it breaks — reproducible builds.** `.github/workflows/reproducible-build.yml:224-240` extracts
`osl-privacy-hub.exe` from the *released* installer and compares its SHA-256 with a fresh rebuild. An
Authenticode signature embeds a certificate and an RFC-3161 timestamp, so a released signed executable
can never match an unsigned rebuild, and the job throws *"released executable bytes do not reproduce
from exact source"* — which reads exactly like tampering. Before the first signed release, that
comparison must strip the PE certificate table on both sides (for example `osslsigncode remove-signature`)
or compare an unsigned intermediate. *This lane does not own `.github/workflows/**`; this is a
description of the required change, not the change.*

**What it does not break:** the minisign chain, the promotion gate, the provenance attestation and the
checksum publication are all indifferent to Authenticode, provided the ordering rule above holds.

---

## 8. Privacy consequence — deliberate, not incidental

Public Trust signing is identity-validated. The verified identity supplies the certificate subject, and
the public certificate is included in every signed binary. That makes the signing identity available to
anyone who obtains a copy. An individual route therefore links the project to the owner's legal
identity; an organisation route links it to the registered entity. Azure's subject preview must be
reviewed before enrolment: its documented subject attributes include the verified name and can include
locality, state/province, country, street address, and postal code — the last two are opt-in and should
be left off. There is no anonymous Public Trust route. Individual Public Trust validation is limited to
the US and Canada; organisations are additionally eligible in the EU, UK, Australia, New Zealand,
Japan, South Korea, Singapore, Switzerland, Norway and Israel.

That disclosure is durable: revoking or changing a future certificate does not remove identity data
already embedded in distributed installers. Do not enrol under an individual identity by accident.

Sources: [certificate-subject attributes](https://learn.microsoft.com/en-us/azure/artifact-signing/concept-resources-roles),
[profile subject preview, region table, eligibility and validation timeline](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart),
[subscription-type restriction and EV policy](https://learn.microsoft.com/en-us/azure/artifact-signing/faq),
[SmartScreen behaviour](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation),
[SKU and quota](https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-change-sku),
[CA/Browser Forum Code Signing Baseline Requirements](https://cabforum.org/working-groups/code-signing/requirements/).

## 9. Acceptance gate before any money is spent

Before spending the **US$119.88/year** baseline (or the ~US$129/year commercial equivalent): record the
§5 answer in writing, capture a fresh certificate-subject preview, prove a test-signed EXE and NSIS
installer, prove the release **fails** when signing is unavailable, and prove the reproducible-build
comparison still passes on a signed artifact. Until all five hold, the honest route is unsigned
distribution with an explicit warning — which is what ships today.
