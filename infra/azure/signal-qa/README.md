# Signal QA Azure pair

This template creates the isolated Windows pair used only for Signal Desktop QA.
It does not install, launch, inspect, relink, or modify Signal or OSL.

- VM resource group: `OSL-SIGNAL-QA-SCUS`
- Shared Key Vault/storage resource group: `OSL-TWO-CLIENT-LAB-SIGNAL`
- VMs: `OSL-Signal-Client-1`, `OSL-Signal-Client-2`
- Region: South Central US, availability zone 3
- Current VM size: `Standard_B2als_v2` (2 vCPU, 4 GB) in South Central US zone 3. It uses the same pinned Windows image and Trusted Launch pattern as Telegram while fitting the student subscription's available quota.
- Windows image version is pinned to `26100.8875.260711`, matching the proven Telegram QA image rather than using `latest`.
- Trusted Launch and system-assigned managed identity enabled
- RDP restricted to the explicit source prefix in the local parameter file
- A dedicated Signal QA Key Vault holds fresh per-VM admin passwords
- Each VM identity receives only Key Vault Secrets User on the dedicated QA
  vault and Storage Blob Data Contributor on the private screenshot container
- Admin passwords are resolved by Azure Resource Manager from Key Vault references;
  secret values never enter this repository or the CLI argument list
- `azuredeploy.parameters.local.json` is ignored; copy the example and use only
  Key Vault resource references, never literal secret values

Before deployment, create four additional QA-only secrets in the referenced
vault: one OSL primary password and one OSL identity import phrase per VM. Their
values must be entered directly into Key Vault, never a manifest, command line,
log, or repository. The controller manifest names only those secret references.
The identity phrases are for disposable OSL QA identities; they are unrelated
to Signal linking.

The screenshot container must be private. The deployment principal needs
permission to create role assignments at both external scopes. VM identities
can write screenshot blobs but the QA scripts do not grant public read access.

Reconcile the VM pair and all four cross-resource-group role assignments with
one idempotent command:

```bash
python3 infra/azure/signal-qa/provision_signal_qa.py reconcile
```

The wrapper validates the immutable image/size/zone pins and requires both admin
passwords to remain ARM Key Vault references. It suppresses deployment output,
deploys the group-local resources, reads only each system identity object ID,
then creates deterministic assignments at the exact external Key Vault and blob
container scopes. Re-running the command converges on the same deployment and
assignment IDs. It finishes with an audit and emits only resource names and
boolean checks—not secret values, identity object IDs, public IPs, or VM data.

Run the same read-only audit without redeploying:

```bash
python3 infra/azure/signal-qa/provision_signal_qa.py audit
```

The ARM template deliberately contains no `roleAssignments`. A group deployment
cannot reliably apply a top-level extension resource whose scope is in a different
resource group; the reviewed wrapper owns that boundary explicitly. The deployment
principal still needs role-assignment permission at both external scopes.

Provider installation, Signal linking, OSL bootstrap, and QA execution remain
separate audited steps.
