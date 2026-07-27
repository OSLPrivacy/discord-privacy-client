<#
Read-only F1 Windows provisioning preflight.

This file must be installed as root-owned host tooling and invoked locally on
the guest as NT AUTHORITY\SYSTEM. It accepts no paths, hashes, principals,
fixtures, or state documents from the caller and never reads witness-key bytes.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:SchemaVersion = 1
$script:PinnedCommit = '1f745c85bb23cf79a956aa87d623905e20f83cf1'
$script:PinnedTree = '1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a'
$script:SystemSid = 'S-1-5-18'
$script:StageRoot = 'C:\ProgramData\OSL-QA\f1-staging'
$script:ShaPattern = '^[0-9a-f]{64}$'
$script:EmptySha256 = (
    'e3b0c44298fc1c149afbf4c8996fb924' +
    '27ae41e4649b934ca495991b7852b855'
)

function Assert-SystemIdentitySid {
    param([Parameter(Mandatory)][string]$Sid)
    if ($Sid -cne $script:SystemSid) {
        throw 'VMQA_F1_WINDOWS_IDENTITY: preflight must run as NT AUTHORITY\SYSTEM'
    }
}

function Assert-PinnedSource {
    param(
        [Parameter(Mandatory)][string]$Commit,
        [Parameter(Mandatory)][string]$Tree,
        [Parameter(Mandatory)][string]$Label
    )
    if (
        $Commit -cne $script:PinnedCommit -or
        $Tree -cne $script:PinnedTree
    ) {
        throw "VMQA_F1_WINDOWS_SOURCE_PIN: $Label is not the exact pinned source"
    }
}

function ConvertTo-CanonicalJsonString {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Value)
    $builder = [Text.StringBuilder]::new()
    [void]$builder.Append('"')
    foreach ($character in $Value.ToCharArray()) {
        $code = [int]$character
        switch ($code) {
            8 { [void]$builder.Append('\b'); continue }
            9 { [void]$builder.Append('\t'); continue }
            10 { [void]$builder.Append('\n'); continue }
            12 { [void]$builder.Append('\f'); continue }
            13 { [void]$builder.Append('\r'); continue }
            34 { [void]$builder.Append('\"'); continue }
            92 { [void]$builder.Append('\\'); continue }
        }
        if ($code -lt 0x20 -or $code -gt 0x7e) {
            [void]$builder.Append(
                ('\u{0:x4}' -f $code)
            )
        } else {
            [void]$builder.Append($character)
        }
    }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function ConvertTo-CanonicalJsonValue {
    param([AllowNull()]$Value)
    if ($null -eq $Value) {
        return 'null'
    }
    if ($Value -is [bool]) {
        return $(if ($Value) { 'true' } else { 'false' })
    }
    if ($Value -is [string]) {
        return ConvertTo-CanonicalJsonString -Value $Value
    }
    if (
        $Value -is [byte] -or $Value -is [sbyte] -or
        $Value -is [int16] -or $Value -is [uint16] -or
        $Value -is [int32] -or $Value -is [uint32] -or
        $Value -is [int64] -or $Value -is [uint64]
    ) {
        return [Convert]::ToString(
            $Value, [Globalization.CultureInfo]::InvariantCulture
        )
    }
    if ($Value -is [Collections.IDictionary]) {
        $names = @($Value.Keys | ForEach-Object { [string]$_ })
        [Array]::Sort($names, [StringComparer]::Ordinal)
        $members = @(
            foreach ($name in $names) {
                (ConvertTo-CanonicalJsonString -Value $name) + ':' +
                    (ConvertTo-CanonicalJsonValue -Value $Value[$name])
            }
        )
        return '{' + ($members -join ',') + '}'
    }
    if (
        $Value -is [Collections.IEnumerable] -and
        $Value -isnot [Management.Automation.PSCustomObject]
    ) {
        $members = @(
            foreach ($item in $Value) {
                ConvertTo-CanonicalJsonValue -Value $item
            }
        )
        return '[' + ($members -join ',') + ']'
    }
    $properties = @($Value.PSObject.Properties.Name)
    [Array]::Sort($properties, [StringComparer]::Ordinal)
    $objectMembers = @(
        foreach ($name in $properties) {
            (ConvertTo-CanonicalJsonString -Value $name) + ':' +
                (ConvertTo-CanonicalJsonValue -Value $Value.$name)
        }
    )
    return '{' + ($objectMembers -join ',') + '}'
}

function Get-CanonicalJsonSha256 {
    param([Parameter(Mandatory)]$Value)
    $canonical = (ConvertTo-CanonicalJsonValue -Value $Value) + "`n"
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($canonical)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $digest = $sha.ComputeHash($bytes)
    } finally {
        $sha.Dispose()
    }
    return (
        [BitConverter]::ToString($digest).Replace(
            '-', ''
        ).ToLowerInvariant()
    )
}

function Assert-ExactFields {
    param(
        [Parameter(Mandatory)]$Value,
        [Parameter(Mandatory)][string[]]$Names,
        [Parameter(Mandatory)][string]$Label
    )
    if ($null -eq $Value) {
        throw "VMQA_F1_WINDOWS_FIELDS: $Label is null"
    }
    $actual = @($Value.PSObject.Properties.Name | Sort-Object)
    $expected = @($Names | Sort-Object)
    if (($actual -join "`n") -cne ($expected -join "`n")) {
        throw "VMQA_F1_WINDOWS_FIELDS: $Label fields are not exact"
    }
}

function Assert-TerminalSnapshotRecord {
    param(
        [Parameter(Mandatory)]$Record,
        [Parameter(Mandatory)][string]$ExpectedPath,
        [Parameter(Mandatory)][bool]$IsDirectory,
        [string]$ExpectedSha256 = '',
        [int64]$ExpectedSize = -1,
        [Parameter(Mandatory)][string]$Label
    )
    $fields = @('path', 'type', 'device', 'inode', 'mode', 'linkCount')
    if (-not $IsDirectory) {
        $fields += @('sizeBytes', 'sha256')
    }
    Assert-ExactFields -Value $Record -Names $fields -Label $Label
    if (
        $Record.path -cne $ExpectedPath -or
        $Record.type -cne $(if ($IsDirectory) { 'directory' } else { 'file' }) -or
        [int64]$Record.device -lt 0 -or
        [int64]$Record.inode -lt 1 -or
        [int]$Record.mode -lt 0 -or
        [int]$Record.linkCount -lt 1
    ) {
        throw "VMQA_F1_WINDOWS_TERMINAL_SNAPSHOT: $Label metadata differs"
    }
    if (
        -not $IsDirectory -and (
            $ExpectedSha256 -cnotmatch $script:ShaPattern -or
            $Record.sha256 -cne $ExpectedSha256 -or
            [int64]$Record.sizeBytes -ne $ExpectedSize
        )
    ) {
        throw "VMQA_F1_WINDOWS_TERMINAL_SNAPSHOT: $Label bytes differ"
    }
}

function Assert-TerminalSnapshot {
    param(
        [Parameter(Mandatory)]$Receipt,
        [Parameter(Mandatory)]$IdentityRead,
        [Parameter(Mandatory)]$SealRead,
        [Parameter(Mandatory)]$ExeHash,
        [Parameter(Mandatory)]$LoaderHash
    )
    $snapshot = $Receipt.terminalSnapshot
    Assert-ExactFields -Value $snapshot -Names @(
        'stageDirectory', 'bundleDirectory', 'outputsDirectory',
        'identity', 'executable', 'loader', 'producerSeal'
    ) -Label 'terminal snapshot'
    $stagePath = [string]$snapshot.stageDirectory.path
    if (
        $stagePath -cnotmatch (
            '^/var/lib/osl-qa/f1-staging/[0-9a-f]{64}$'
        )
    ) {
        throw 'VMQA_F1_WINDOWS_TERMINAL_SNAPSHOT: stage path is invalid'
    }
    $expectedExecutablePath = (
        $stagePath + '/bundle/outputs/osl-privacy-hub.exe'
    )
    $expectedLoaderPath = (
        $stagePath + '/bundle/outputs/WebView2Loader.dll'
    )
    $expectedSealPath = $stagePath + '/producer-seal.json'
    if (
        $Receipt.executable.path -cne $expectedExecutablePath -or
        $Receipt.loader.path -cne $expectedLoaderPath -or
        $Receipt.producerSeal.path -cne $expectedSealPath
    ) {
        throw (
            'VMQA_F1_WINDOWS_TERMINAL_SNAPSHOT: receipt paths do not ' +
            'derive from the fixed stage'
        )
    }
    Assert-TerminalSnapshotRecord -Record $snapshot.stageDirectory `
        -ExpectedPath $stagePath -IsDirectory $true -Label 'stage directory'
    Assert-TerminalSnapshotRecord -Record $snapshot.bundleDirectory `
        -ExpectedPath ($stagePath + '/bundle') -IsDirectory $true `
        -Label 'bundle directory'
    Assert-TerminalSnapshotRecord -Record $snapshot.outputsDirectory `
        -ExpectedPath ($stagePath + '/bundle/outputs') -IsDirectory $true `
        -Label 'outputs directory'
    Assert-TerminalSnapshotRecord -Record $snapshot.identity `
        -ExpectedPath ($stagePath + '/bundle/build-identity.json') `
        -IsDirectory $false -ExpectedSha256 $IdentityRead.sha256 `
        -ExpectedSize $IdentityRead.sizeBytes -Label 'build identity'
    Assert-TerminalSnapshotRecord -Record $snapshot.executable `
        -ExpectedPath $expectedExecutablePath -IsDirectory $false `
        -ExpectedSha256 $ExeHash.sha256 -ExpectedSize $ExeHash.sizeBytes `
        -Label 'release executable'
    Assert-TerminalSnapshotRecord -Record $snapshot.loader `
        -ExpectedPath $expectedLoaderPath -IsDirectory $false `
        -ExpectedSha256 $LoaderHash.sha256 -ExpectedSize $LoaderHash.sizeBytes `
        -Label 'release loader'
    Assert-TerminalSnapshotRecord -Record $snapshot.producerSeal `
        -ExpectedPath $expectedSealPath -IsDirectory $false `
        -ExpectedSha256 $SealRead.sha256 -ExpectedSize $SealRead.sizeBytes `
        -Label 'producer seal'
    $snapshotSha = Get-CanonicalJsonSha256 -Value $snapshot
    if (
        $Receipt.terminalSnapshotSha256 -cnotmatch $script:ShaPattern -or
        $Receipt.terminalSnapshotSha256 -cne $snapshotSha
    ) {
        throw 'VMQA_F1_WINDOWS_TERMINAL_SNAPSHOT: canonical hash differs'
    }
}

function Assert-SystemOnlyAclRecord {
    param(
        [Parameter(Mandatory)]$Record,
        [Parameter(Mandatory)][bool]$IsDirectory,
        [Parameter(Mandatory)][string]$Label
    )
    Assert-ExactFields -Value $Record -Names @(
        'ownerSid', 'protected', 'rules'
    ) -Label "$Label ACL record"
    if ($Record.ownerSid -cne $script:SystemSid) {
        throw "VMQA_F1_WINDOWS_OWNER: $Label is not owned by SYSTEM"
    }
    if ($Record.protected -ne $true) {
        throw "VMQA_F1_WINDOWS_ACL_INHERITED: $Label DACL is not protected"
    }
    $rules = @($Record.rules)
    if ($rules.Count -ne 1) {
        throw "VMQA_F1_WINDOWS_ACL_COUNT: $Label must have one SYSTEM ACE"
    }
    $rule = $rules[0]
    Assert-ExactFields -Value $rule -Names @(
        'sid', 'type', 'rights', 'inherited', 'inheritance', 'propagation'
    ) -Label "$Label ACE"
    $expectedInheritance = if ($IsDirectory) {
        'ContainerInherit, ObjectInherit'
    } else {
        'None'
    }
    if (
        $rule.sid -cne $script:SystemSid -or
        $rule.type -cne 'Allow' -or
        $rule.rights -cne 'FullControl' -or
        $rule.inherited -ne $false -or
        $rule.inheritance -cne $expectedInheritance -or
        $rule.propagation -cne 'None'
    ) {
        throw "VMQA_F1_WINDOWS_ACL_RIGHTS: $Label ACL is not SYSTEM-only FullControl"
    }
}

function Get-SystemAclRecord {
    param(
        [Parameter(Mandatory)][string]$LiteralPath
    )
    $acl = Get-Acl -LiteralPath $LiteralPath
    $ownerSid = (
        [Security.Principal.NTAccount]::new($acl.Owner)
    ).Translate([Security.Principal.SecurityIdentifier]).Value
    $rules = @(
        $acl.GetAccessRules(
            $true,
            $true,
            [Security.Principal.SecurityIdentifier]
        ) | ForEach-Object {
            [pscustomobject]@{
                sid = $_.IdentityReference.Value
                type = [string]$_.AccessControlType
                rights = [string]$_.FileSystemRights
                inherited = [bool]$_.IsInherited
                inheritance = [string]$_.InheritanceFlags
                propagation = [string]$_.PropagationFlags
            }
        }
    )
    return [pscustomobject]@{
        ownerSid = $ownerSid
        protected = [bool]$acl.AreAccessRulesProtected
        rules = $rules
    }
}

function Assert-FixedItem {
    param(
        [Parameter(Mandatory)][string]$LiteralPath,
        [Parameter(Mandatory)][bool]$IsDirectory,
        [Parameter(Mandatory)][string]$Label
    )
    if (-not (Test-Path -LiteralPath $LiteralPath)) {
        throw "VMQA_F1_WINDOWS_MISSING: $Label"
    }
    $item = Get-Item -LiteralPath $LiteralPath -Force
    if ($IsDirectory -ne [bool]$item.PSIsContainer) {
        throw "VMQA_F1_WINDOWS_TYPE: $Label has the wrong type"
    }
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "VMQA_F1_WINDOWS_REPARSE: $Label is a reparse point"
    }
    Assert-SystemOnlyAclRecord `
        -Record (Get-SystemAclRecord -LiteralPath $LiteralPath) `
        -IsDirectory $IsDirectory `
        -Label $Label
    return $item
}

function Read-JsonFileStable {
    param(
        [Parameter(Mandatory)][string]$LiteralPath,
        [Parameter(Mandatory)][string]$Label
    )
    $before = Get-Item -LiteralPath $LiteralPath -Force
    $bytes = [IO.File]::ReadAllBytes($LiteralPath)
    $after = Get-Item -LiteralPath $LiteralPath -Force
    if (
        $before.Length -ne $after.Length -or
        $before.LastWriteTimeUtc.Ticks -ne $after.LastWriteTimeUtc.Ticks
    ) {
        throw "VMQA_F1_WINDOWS_CHANGED: $Label changed while read"
    }
    try {
        $text = [Text.UTF8Encoding]::new(
            $false, $true
        ).GetString($bytes)
        $value = $text | ConvertFrom-Json
    } catch {
        throw "VMQA_F1_WINDOWS_JSON: $Label is not strict UTF-8 JSON"
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $digest = $sha.ComputeHash($bytes)
    } finally {
        $sha.Dispose()
    }
    return [pscustomobject]@{
        value = $value
        sha256 = (
            [BitConverter]::ToString($digest).Replace(
                '-', ''
            ).ToLowerInvariant()
        )
        sizeBytes = [int64]$bytes.Length
    }
}

function Get-StableFileHash {
    param(
        [Parameter(Mandatory)][string]$LiteralPath,
        [Parameter(Mandatory)][string]$Label
    )
    $before = Get-Item -LiteralPath $LiteralPath -Force
    $hash = (
        Get-FileHash -LiteralPath $LiteralPath -Algorithm SHA256
    ).Hash.ToLowerInvariant()
    $after = Get-Item -LiteralPath $LiteralPath -Force
    if (
        $before.Length -ne $after.Length -or
        $before.LastWriteTimeUtc.Ticks -ne $after.LastWriteTimeUtc.Ticks
    ) {
        throw "VMQA_F1_WINDOWS_CHANGED: $Label changed while hashed"
    }
    return [pscustomobject]@{
        sha256 = $hash
        sizeBytes = [int64]$after.Length
    }
}

function Assert-HashAndSize {
    param(
        [Parameter(Mandatory)]$Observed,
        [Parameter(Mandatory)]$Expected,
        [Parameter(Mandatory)][string]$Label
    )
    Assert-ExactFields -Value $Expected -Names @(
        'path', 'sha256', 'sizeBytes'
    ) -Label "$Label receipt"
    if (
        $Expected.sha256 -cnotmatch $script:ShaPattern -or
        [int64]$Expected.sizeBytes -lt 1 -or
        $Observed.sha256 -cne $Expected.sha256 -or
        [int64]$Observed.sizeBytes -ne [int64]$Expected.sizeBytes
    ) {
        throw "VMQA_F1_WINDOWS_HASH: $Label differs from sealed receipt"
    }
}

function Invoke-WindowsProvisioningPreflight {
    $currentSid = (
        [Security.Principal.WindowsIdentity]::GetCurrent()
    ).User.Value
    Assert-SystemIdentitySid -Sid $currentSid
    [void](Assert-FixedItem -LiteralPath $script:StageRoot `
        -IsDirectory $true -Label 'Windows F1 staging root')
    $rootEntries = @(Get-ChildItem -LiteralPath $script:StageRoot -Force)
    if (
        $rootEntries.Count -ne 1 -or
        -not $rootEntries[0].PSIsContainer -or
        $rootEntries[0].Name -cnotmatch $script:ShaPattern
    ) {
        throw 'VMQA_F1_WINDOWS_LAYOUT: staging root must contain one hash directory'
    }
    $stage = $rootEntries[0].FullName
    $expectedStageEntries = @('bundle', 'producer-seal.json', 'staging-receipt.json')
    $actualStageEntries = @(
        Get-ChildItem -LiteralPath $stage -Force |
            ForEach-Object Name | Sort-Object
    )
    if (($actualStageEntries -join "`n") -cne (($expectedStageEntries | Sort-Object) -join "`n")) {
        throw 'VMQA_F1_WINDOWS_LAYOUT: stage entries are not exact'
    }
    $bundle = Join-Path $stage 'bundle'
    $identityPath = Join-Path $bundle 'build-identity.json'
    $outputs = Join-Path $bundle 'outputs'
    $exePath = Join-Path $outputs 'osl-privacy-hub.exe'
    $loaderPath = Join-Path $outputs 'WebView2Loader.dll'
    $receiptPath = Join-Path $stage 'staging-receipt.json'
    $sealPath = Join-Path $stage 'producer-seal.json'
    $evidence = Join-Path $bundle 'build-evidence'
    $dist = Join-Path $outputs 'dist'
    $expectedBundleEntries = @(
        'build-evidence', 'build-identity.json', 'outputs'
    )
    $actualBundleEntries = @(
        Get-ChildItem -LiteralPath $bundle -Force |
            ForEach-Object Name | Sort-Object
    )
    if (($actualBundleEntries -join "`n") -cne (($expectedBundleEntries | Sort-Object) -join "`n")) {
        throw 'VMQA_F1_WINDOWS_LAYOUT: bundle entries are not exact'
    }
    $expectedOutputEntries = @(
        'dist', 'osl-privacy-hub.exe', 'WebView2Loader.dll'
    )
    $actualOutputEntries = @(
        Get-ChildItem -LiteralPath $outputs -Force |
            ForEach-Object Name | Sort-Object
    )
    if (($actualOutputEntries -join "`n") -cne (($expectedOutputEntries | Sort-Object) -join "`n")) {
        throw 'VMQA_F1_WINDOWS_LAYOUT: output entries are not exact'
    }
    foreach ($directory in @($stage, $bundle, $outputs, $evidence, $dist)) {
        [void](Assert-FixedItem -LiteralPath $directory `
            -IsDirectory $true -Label $directory)
    }
    foreach ($file in @($identityPath, $exePath, $loaderPath, $receiptPath, $sealPath)) {
        [void](Assert-FixedItem -LiteralPath $file `
            -IsDirectory $false -Label $file)
    }
    foreach ($entry in @(Get-ChildItem -LiteralPath $stage -Force -Recurse)) {
        [void](Assert-FixedItem -LiteralPath $entry.FullName `
            -IsDirectory ([bool]$entry.PSIsContainer) -Label $entry.FullName)
    }

    $receiptRead = Read-JsonFileStable -LiteralPath $receiptPath -Label 'staging receipt'
    $identityRead = Read-JsonFileStable -LiteralPath $identityPath -Label 'build identity'
    $sealRead = Read-JsonFileStable -LiteralPath $sealPath -Label 'producer seal'
    $receipt = $receiptRead.value
    $identity = $identityRead.value
    $seal = $sealRead.value
    Assert-ExactFields -Value $receipt -Names @(
        'schemaVersion', 'mode', 'producer', 'source',
        'buildIdentitySha256', 'producerSeal', 'executable', 'loader',
        'nativeWitnessKeyId', 'admissionTransition', 'terminalSnapshot',
        'terminalSnapshotSha256'
    ) -Label 'staging receipt'
    Assert-ExactFields -Value $receipt.source -Names @(
        'commit', 'tree'
    ) -Label 'receipt source'
    Assert-ExactFields -Value $identity -Names @(
        'schemaVersion', 'source', 'ui', 'build', 'artifacts', 'evidence'
    ) -Label 'build identity'
    Assert-ExactFields -Value $identity.source -Names @(
        'commit', 'tree', 'clean', 'dirtyFingerprint'
    ) -Label 'build identity source'
    Assert-ExactFields -Value $identity.artifacts -Names @(
        'executable', 'loader'
    ) -Label 'build identity artifacts'
    Assert-ExactFields -Value $identity.artifacts.executable -Names @(
        'name', 'path', 'sha256', 'sizeBytes'
    ) -Label 'build identity executable'
    Assert-ExactFields -Value $identity.artifacts.loader -Names @(
        'name', 'path', 'sha256', 'sizeBytes'
    ) -Label 'build identity loader'
    Assert-ExactFields -Value $seal -Names @(
        'schemaVersion', 'producer', 'mode', 'identitySha256',
        'sourceCommit', 'sourceTree', 'generation',
        'previousSealSha256', 'transition'
    ) -Label 'producer seal'
    Assert-ExactFields -Value $receipt.producerSeal -Names @(
        'authorityPath', 'path', 'sha256', 'generation',
        'previousSealSha256', 'transition'
    ) -Label 'receipt producer seal'
    Assert-PinnedSource -Commit $receipt.source.commit `
        -Tree $receipt.source.tree -Label 'staging receipt'
    Assert-PinnedSource -Commit $identity.source.commit `
        -Tree $identity.source.tree -Label 'build identity'
    Assert-PinnedSource -Commit $seal.sourceCommit `
        -Tree $seal.sourceTree -Label 'producer seal'

    $exeHash = Get-StableFileHash -LiteralPath $exePath -Label 'release executable'
    $loaderHash = Get-StableFileHash -LiteralPath $loaderPath -Label 'release loader'
    Assert-HashAndSize -Observed $exeHash -Expected $receipt.executable `
        -Label 'release executable'
    Assert-HashAndSize -Observed $loaderHash -Expected $receipt.loader `
        -Label 'release loader'
    Assert-TerminalSnapshot -Receipt $receipt `
        -IdentityRead $identityRead -SealRead $sealRead `
        -ExeHash $exeHash -LoaderHash $loaderHash
    $expectedAuthoritySeal = (
        '/var/lib/osl-vmqa/producer-seals/' +
        $identityRead.sha256 + '.json'
    )
    $initialSeal = [int]$seal.generation -eq 1
    $expectedStageSuffix = '\' + $exeHash.sha256
    if (
        [int]$receipt.schemaVersion -ne 2 -or
        $receipt.mode -cne 'production' -or
        $receipt.producer -cne 'osl-vmqa-producer' -or
        [int]$identity.schemaVersion -ne 2 -or
        $identity.source.clean -ne $true -or
        $identity.source.dirtyFingerprint -cne $script:EmptySha256 -or
        $identity.artifacts.executable.name -cne 'osl-privacy-hub.exe' -or
        $identity.artifacts.executable.path -cne 'outputs/osl-privacy-hub.exe' -or
        $identity.artifacts.executable.sha256 -cne $exeHash.sha256 -or
        [int64]$identity.artifacts.executable.sizeBytes -ne $exeHash.sizeBytes -or
        $identity.artifacts.loader.name -cne 'WebView2Loader.dll' -or
        $identity.artifacts.loader.path -cne 'outputs/WebView2Loader.dll' -or
        $identity.artifacts.loader.sha256 -cne $loaderHash.sha256 -or
        [int64]$identity.artifacts.loader.sizeBytes -ne $loaderHash.sizeBytes -or
        $receipt.executable.path -cnotmatch '/bundle/outputs/osl-privacy-hub\.exe$' -or
        $receipt.loader.path -cnotmatch '/bundle/outputs/WebView2Loader\.dll$' -or
        $receipt.buildIdentitySha256 -cne $identityRead.sha256 -or
        $receipt.producerSeal.sha256 -cne $sealRead.sha256 -or
        $receipt.producerSeal.authorityPath -cne $expectedAuthoritySeal -or
        $receipt.producerSeal.path -cnotmatch '/producer-seal\.json$' -or
        [int]$receipt.producerSeal.generation -ne [int]$seal.generation -or
        $receipt.producerSeal.previousSealSha256 -cne $seal.previousSealSha256 -or
        $receipt.producerSeal.transition -cne $seal.transition -or
        $seal.mode -cne 'production' -or
        $seal.producer -cne 'osl-vmqa-producer' -or
        $seal.identitySha256 -cne $identityRead.sha256 -or
        [int]$seal.generation -lt 1 -or
        $seal.previousSealSha256 -cnotmatch $script:ShaPattern -or
        (
            $initialSeal -and (
                $seal.previousSealSha256 -cne $script:EmptySha256 -or
                $seal.transition -cne 'initial'
            )
        ) -or
        (
            -not $initialSeal -and (
                $seal.previousSealSha256 -ceq $script:EmptySha256 -or
                $seal.transition -cne 'successor'
            )
        ) -or
        $stage.EndsWith(
            $expectedStageSuffix,
            [StringComparison]::OrdinalIgnoreCase
        ) -ne $true
    ) {
        throw 'VMQA_F1_WINDOWS_BINDING: staged bytes are not the exact sealed pinned build'
    }
    # Rehash every receipt-bound file after all semantic checks.
    if (
        (Get-StableFileHash -LiteralPath $identityPath -Label 'terminal identity').sha256 -cne $identityRead.sha256 -or
        (Get-StableFileHash -LiteralPath $sealPath -Label 'terminal seal').sha256 -cne $sealRead.sha256 -or
        (Get-StableFileHash -LiteralPath $exePath -Label 'terminal executable').sha256 -cne $exeHash.sha256 -or
        (Get-StableFileHash -LiteralPath $loaderPath -Label 'terminal loader').sha256 -cne $loaderHash.sha256 -or
        (Get-StableFileHash -LiteralPath $receiptPath -Label 'terminal receipt').sha256 -cne $receiptRead.sha256
    ) {
        throw 'VMQA_F1_WINDOWS_TERMINAL: staged bytes changed before return'
    }
    return [ordered]@{
        schemaVersion = $script:SchemaVersion
        status = 'ready'
        identity = $script:SystemSid
        sourceCommit = $script:PinnedCommit
        sourceTree = $script:PinnedTree
        stage = $stage
        buildIdentitySha256 = $identityRead.sha256
        executableSha256 = $exeHash.sha256
        loaderSha256 = $loaderHash.sha256
        producerSealSha256 = $sealRead.sha256
        writesPerformed = 0
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        Invoke-WindowsProvisioningPreflight |
            ConvertTo-Json -Compress -Depth 8
        exit 0
    } catch {
        [Console]::Error.WriteLine(
            'VMQA F1 WINDOWS PROVISIONING REFUSED: ' + $_.Exception.Message
        )
        exit 9
    }
}
