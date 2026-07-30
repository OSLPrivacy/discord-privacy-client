param([Parameter(Mandatory)][string]$Manifest)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$manifestArgument = $Manifest
. (Join-Path $PSScriptRoot 'vmqa-f1-provisioning-plan-validator.ps1') `
    -Manifest $manifestArgument

$script:Passed = 0
$script:Failed = 0

function Pass([string]$Name) {
    $script:Passed += 1
    Write-Output ("ok - " + $Name)
}

function Fail([string]$Name, [string]$Reason) {
    $script:Failed += 1
    Write-Output ("not ok - " + $Name + ": " + $Reason)
}

function Expect-Pass {
    param([string]$Name, [scriptblock]$Action)
    try {
        & $Action
        Pass $Name
    } catch {
        Fail $Name $_.Exception.Message
    }
}

function Expect-Refusal {
    param(
        [string]$Name,
        [string]$Reason,
        [scriptblock]$Action
    )
    try {
        & $Action
        Fail $Name 'unexpected acceptance'
    } catch {
        if ($_.Exception.Message -match $Reason) {
            Pass $Name
        } else {
            Fail $Name $_.Exception.Message
        }
    }
}

function Copy-Plan($Value) {
    return ($Value | ConvertTo-Json -Compress -Depth 40 | ConvertFrom-Json)
}

function Set-PlanHash($Value) {
    $Value.payloadSha256 = Get-CanonicalJsonSha256 -Value $Value.payload
}

$resolved = (Resolve-Path -LiteralPath $manifestArgument).ProviderPath
$original = [IO.File]::ReadAllText(
    $resolved, [Text.UTF8Encoding]::new($false, $true)
) | ConvertFrom-Json

Expect-Pass 'nonempty safe plan accepts without execution' {
    $result = Assert-F1ProvisioningPlan -Value $original
    if (
        $result.status -cne 'valid-plan-only' -or
        $result.executionPermitted -ne $false -or
        [int]$result.writesPerformed -ne 0 -or
        @($original.payload.operatorTransitions).Count -ne 7
    ) {
        throw 'positive plan was empty or claimed execution'
    }
}
Expect-Refusal 'wrong producer account refuses' 'TYPE|ACCOUNT' {
    $candidate = Copy-Plan $original
    $candidate.payload.producerAccount.name = 'caller'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'login-capable producer shell refuses' 'TYPE|ACCOUNT' {
    $candidate = Copy-Plan $original
    $candidate.payload.producerAccount.shell = '/bin/bash'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'root producer uid refuses' 'ACCOUNT' {
    $candidate = Copy-Plan $original
    $candidate.payload.producerAccount.uid = 0
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'wrong program owner refuses' 'PROGRAM' {
    $candidate = Copy-Plan $original
    $candidate.payload.programs[0].owner = 'osl-vmqa-producer'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'wrong program mode refuses' 'PROGRAM' {
    $candidate = Copy-Plan $original
    $candidate.payload.programs[0].mode = '0755'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'numeric program mode refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.programs[0].mode = 555
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'wrong program hash refuses' 'PROGRAM' {
    $candidate = Copy-Plan $original
    $candidate.payload.programs[0].sha256 = ('0' * 64)
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'wrong host directory owner refuses' 'HOST' {
    $candidate = Copy-Plan $original
    $candidate.payload.hostDirectories[0].owner = 'osl-vmqa-producer'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'wrong host directory mode refuses' 'HOST' {
    $candidate = Copy-Plan $original
    $candidate.payload.hostDirectories[0].mode = '0777'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'numeric host directory mode refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.hostDirectories[0].mode = 755
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'secret field in host inventory refuses' 'SECRET' {
    $candidate = Copy-Plan $original
    $candidate.payload.hostDirectories[0] |
        Add-Member -NotePropertyName secret -NotePropertyValue ('9' * 64)
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'wrong toolchain owner refuses' 'TOOLCHAIN' {
    $candidate = Copy-Plan $original
    $candidate.payload.toolchain.owner = 'osl-vmqa-producer'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'numeric toolchain root mode refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.toolchain.mode = 755
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'wrong tool mode refuses' 'TYPE|TOOLCHAIN' {
    $candidate = Copy-Plan $original
    $candidate.payload.toolchain.tools[0].mode = '0755'
    $tree = [pscustomobject]@{
        root = $candidate.payload.toolchain.root
        measurementMethod = $candidate.payload.toolchain.measurementMethod
        tools = $candidate.payload.toolchain.tools
    }
    $candidate.payload.toolchain.treeSha256 = Get-CanonicalJsonSha256 $tree
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'numeric tool mode refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.toolchain.tools[0].mode = 555
    $tree = [pscustomobject]@{
        root = $candidate.payload.toolchain.root
        measurementMethod = $candidate.payload.toolchain.measurementMethod
        tools = $candidate.payload.toolchain.tools
    }
    $candidate.payload.toolchain.treeSha256 = Get-CanonicalJsonSha256 $tree
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'invalid independent tool hash refuses' 'HASH' {
    $candidate = Copy-Plan $original
    $candidate.payload.toolchain.tools[0].sha256 = 'bad'
    $tree = [pscustomobject]@{
        root = $candidate.payload.toolchain.root
        measurementMethod = $candidate.payload.toolchain.measurementMethod
        tools = $candidate.payload.toolchain.tools
    }
    $candidate.payload.toolchain.treeSha256 = Get-CanonicalJsonSha256 $tree
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'array tool hash cannot impersonate digest' 'HASH' {
    $candidate = Copy-Plan $original
    $candidate.payload.toolchain.tools[0].sha256 = @(
        '1111111111111111111111111111111111111111111111111111111111111111'
    )
    $tree = [pscustomobject]@{
        root = $candidate.payload.toolchain.root
        measurementMethod = $candidate.payload.toolchain.measurementMethod
        tools = $candidate.payload.toolchain.tools
    }
    $candidate.payload.toolchain.treeSha256 = Get-CanonicalJsonSha256 $tree
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'wrong SYSTEM ACL owner refuses' 'ACL' {
    $candidate = Copy-Plan $original
    $candidate.payload.guestAcl.template.ownerSid = 'S-1-5-32-544'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'inherited guest ACL refuses' 'ACL' {
    $candidate = Copy-Plan $original
    $candidate.payload.guestAcl.entries[0].inheritanceEnabled = $true
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'extra guest ACL principal refuses' 'ACL' {
    $candidate = Copy-Plan $original
    $extra = Copy-Plan $candidate.payload.guestAcl.entries[0].aces[0]
    $extra.sid = 'S-1-5-32-544'
    $candidate.payload.guestAcl.entries[0].aces = @(
        $candidate.payload.guestAcl.entries[0].aces[0],
        $extra
    )
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'missing key presence refuses' 'KEY' {
    $candidate = Copy-Plan $original
    $candidate.payload.witnessKey.present = $false
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'numeric witness key mode refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.witnessKey.mode = 600
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'key byte exposure field refuses' 'SECRET' {
    $candidate = Copy-Plan $original
    $candidate.payload.witnessKey |
        Add-Member -NotePropertyName keyBytes -NotePropertyValue ('9' * 64)
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'stale coherent predecessor lineage refuses' 'TYPE|LINEAGE' {
    $candidate = Copy-Plan $original
    $candidate.payload.contract.provisioningCommit = ('0' * 40)
    $candidate.payload.contract.provisioningTree = ('1' * 40)
    $candidate.payload.contract.predecessorSnapshotSha256 = ('2' * 64)
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'old seal lineage refuses' 'RELEASE' {
    $candidate = Copy-Plan $original
    $candidate.payload.release.sealGeneration = 2
    $candidate.payload.release.previousSealSha256 = ('3' * 64)
    $candidate.payload.release.transition = 'successor'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'runtime argv injection refuses' 'RUNTIME' {
    $candidate = Copy-Plan $original
    $candidate.payload.runtime.argv += ';Start-AzVM'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'newline string cannot impersonate argv array' 'RUNTIME' {
    $candidate = Copy-Plan $original
    $candidate.payload.runtime.argv = (
        @($candidate.payload.runtime.argv) -join "`n"
    )
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'transition reorder refuses' 'TYPE|TRANSITIONS' {
    $candidate = Copy-Plan $original
    [Array]::Reverse($candidate.payload.operatorTransitions)
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'transition authority rewrite refuses' 'TYPE|TRANSITIONS' {
    $candidate = Copy-Plan $original
    $candidate.payload.operatorTransitions[0].authority = 'caller'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'transition binding rewrite refuses' 'TYPE|TRANSITIONS' {
    $candidate = Copy-Plan $original
    $candidate.payload.operatorTransitions[0].bindsTo = @('/caller')
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'six-transition plan refuses' 'TRANSITIONS' {
    $candidate = Copy-Plan $original
    $candidate.payload.operatorTransitions = @(
        $candidate.payload.operatorTransitions | Select-Object -First 6
    )
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'claimed execution refuses' 'EXECUTION' {
    $candidate = Copy-Plan $original
    $candidate.payload.status = 'executed'
    $candidate.payload.executionPermitted = $true
    $candidate.payload.writesPerformed = 1
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'array status cannot impersonate scalar' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.status = @('planned')
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'array assurance cannot impersonate scalar' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.assurance = @('offline-plan-only')
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'array runtime authority cannot impersonate scalar' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.runtime.authorization = @(
        'separate-live-f1-runtime'
    )
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'array contract pin cannot impersonate scalar' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.contract.releaseCommit = @(
        '1f745c85bb23cf79a956aa87d623905e20f83cf1'
    )
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'array program path cannot impersonate scalar' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.programs[0].path = @(
        '/opt/osl-vmqa/bin/vmqa_f1_producer.py'
    )
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'string writes-performed refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.writesPerformed = '0'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'numeric execution boolean refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.executionPermitted = 0
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'string producer uid refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.producerAccount.uid = '991'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'string seal generation refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.release.sealGeneration = '1'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'string transition ordinal refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.operatorTransitions[0].ordinal = '1'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'string transition writes refuses' 'TYPE' {
    $candidate = Copy-Plan $original
    $candidate.payload.operatorTransitions[0].writesPerformed = '0'
    Set-PlanHash $candidate
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'payload hash mutation refuses' 'HASH' {
    $candidate = Copy-Plan $original
    $candidate.payloadSha256 = ('0' * 64)
    Assert-F1ProvisioningPlan -Value $candidate
}
Expect-Refusal 'array payload hash cannot impersonate digest' 'HASH' {
    $candidate = Copy-Plan $original
    $candidate.payloadSha256 = @($candidate.payloadSha256)
    Assert-F1ProvisioningPlan -Value $candidate
}

Write-Output ("passed={0} failed={1}" -f $script:Passed, $script:Failed)
if ($script:Failed -ne 0) {
    exit 1
}
