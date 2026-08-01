# Windows code-signing decision

**Decision date:** 2026-07-31

**Status:** deferred by owner decision D63; ship the current first-usable build unsigned.

## Recommendation

Do not buy or configure a code-signing service now. The first-year signing cost for the
first-usable release is **$0**. Accept the Windows unknown-publisher / SmartScreen warning,
explain it plainly at download time, and keep the existing minisign updater key as the build
integrity anchor. This is not a claim that the executable is Authenticode-signed.

Re-open this decision only when a release is ready to fund signing. The then-recommended route
is **Azure Artifact Signing Basic, Public Trust**, tested with its Public Trust Test profile
before production use, with GitHub OIDC rather than an exportable CI secret.

## Numbers and constraints (checked 2026-07-31)

| Item                   |              Current figure | What it means                                                                                                                                   |
| ---------------------- | --------------------------: | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Artifact Signing Basic |           **US$9.99/month** | **US$119.88** for 12 months, before any applicable tax; includes 5,000 signatures/month and then US$0.005/signature.                            |
| Validation lead time   |      **1–20 business days** | Microsoft says it can take longer when more documents are needed. It is not a launch-date guarantee.                                            |
| New signed download    |    Warning can still appear | A valid OV/EV-style signature displays a verified publisher but SmartScreen reputation must still accumulate. Microsoft publishes no threshold. |
| Unsigned download      |            Warning expected | Users must choose “Run anyway”; organisational policy can forbid continuation.                                                                  |
| EV premium             | **No SmartScreen shortcut** | Microsoft says EV no longer bypasses SmartScreen, so paying extra solely for this result is unjustified.                                        |

Sources: [Azure Artifact Signing SKU and quota](https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-change-sku),
[Microsoft’s identity-validation timeline](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart), and
[Microsoft SmartScreen guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).

## Privacy consequence — deliberate, not incidental

Public Trust signing is identity-validated. The verified identity supplies the certificate
subject, and the public certificate is included in every signed binary. That makes the signing
identity available to anyone who obtains a copy. An individual route therefore links the project
to the owner’s legal identity; an organisation route links it to the registered entity. Azure’s
subject preview must be reviewed before enrolment: its documented subject attributes include the
verified name and can include locality, state/province, country, street address, and postal code.
There is no anonymous Public Trust route. Individual Public Trust validation is currently limited
to the US and Canada; organisations are additionally eligible in the EU and UK.

That disclosure is durable: revoking or changing a future certificate does not remove identity
data already embedded in distributed installers. Do not enrol under an individual identity by
accident. Sources: [certificate-subject attributes](https://learn.microsoft.com/en-us/azure/artifact-signing/concept-resources-roles),
[profile subject preview and address controls](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart), and
[current Public Trust eligibility](https://learn.microsoft.com/en-us/azure/artifact-signing/faq).

## Later acceptance gate

Before spending the **US$119.88/year** baseline, record whether the public subject should name an
individual or a legal entity, capture a fresh certificate-subject preview, and prove a test-signed
EXE and NSIS installer fail the release if signing is unavailable. Until then, the honest route is
unsigned distribution with an explicit warning.
