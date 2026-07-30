<#
Read-only cross-platform validator for the offline F1 provisioning plan.
It validates declarative JSON only and has no mutation or execution operation.
#>
param([string]$Manifest)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$script:PlanKind = 'vmqa-f1-provisioning-plan'
$script:ProvisioningCommit = '2279be3b789f4aadbcc35db57d6105ab820e3bf6'
$script:ProvisioningTree = 'a9b9d5027f6123a4a1c4b012641ae2f7f43ec299'
$script:PredecessorSnapshotSha256 = (
    '2d305361dabf7217c86ce20ac987bdd19fe0bc70194d7f3ad122756228f3de97'
)
$script:ReleaseCommit = '1f745c85bb23cf79a956aa87d623905e20f83cf1'
$script:ReleaseTree = '1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a'
$script:Producer = 'osl-vmqa-producer'
$script:ProducerHome = '/var/lib/osl-vmqa'
$script:ProducerShell = '/usr/sbin/nologin'
$script:BinRoot = '/opt/osl-vmqa/bin'
$script:ToolchainRoot = '/opt/osl-vmqa/toolchain'
$script:ToolchainBin = '/opt/osl-vmqa/toolchain/bin'
$script:HostStageRoot = '/var/lib/osl-qa/f1-staging'
$script:HostKeyPath = '/var/lib/osl-qa/private/f1-native-witness.key'
$script:WindowsRoot = 'C:\ProgramData\OSL-QA'
$script:WindowsStageRoot = 'C:\ProgramData\OSL-QA\f1-staging'
$script:WindowsKeyPath = (
    'C:\ProgramData\OSL-QA\private\f1-native-witness.key'
)
$script:SystemSid = 'S-1-5-18'
$script:EmptySha256 = (
    'e3b0c44298fc1c149afbf4c8996fb924' +
    '27ae41e4649b934ca495991b7852b855'
)
$script:ShaPattern = '^[0-9a-f]{64}$'
$script:ToolNames = @('cargo', 'git', 'node', 'npm', 'osl-cargo', 'rustc')
$script:TransitionIds = @(
    'host-producer-account',
    'root-program-installation',
    'protected-toolchain-installation',
    'producer-witness-key',
    'pinned-release-stage',
    'guest-system-provisioning',
    'separately-authorized-runtime'
)
$script:ProgramPins = @(
    [pscustomobject]@{
        name = 'vmqa_f1_producer.py'
        sha256 = '65751666b2160eb654e32c0288f054404684ba26a222aeabea86ed3b6a8f988c'
    },
    [pscustomobject]@{
        name = 'vmqa_build_evidence.py'
        sha256 = 'fc8041443f1841ccf0173e598fb1fcdd80ed03afc24f32608fb4de29137ac625'
    },
    [pscustomobject]@{
        name = 'vmqa_f1_provisioning_preflight.py'
        sha256 = '78a71701b9ea169ea621303863a5f57129e0da01d457f40004e9447c7e259b91'
    },
    [pscustomobject]@{
        name = 'vmqa-f1-windows-provisioning-preflight.ps1'
        sha256 = '7fcc04012c31a6c1e7ab9bde28f57927ccd3886e4cbc91ff2aef1f246f0dde88'
    },
    [pscustomobject]@{
        name = 'vmqa-run.sh'
        sha256 = '0c6df93bb55a17ce04140a142d553e004c733629599b700e02fadb551c4ac216'
    }
)

function ConvertTo-CanonicalJsonString {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Value)
    $builder = [Text.StringBuilder]::new()
    [void]$builder.Append('"')
    foreach ($character in $Value.ToCharArray()) {
        $code = [int]$character
        $escaped = $true
        switch ($code) {
            8 { [void]$builder.Append('\b'); break }
            9 { [void]$builder.Append('\t'); break }
            10 { [void]$builder.Append('\n'); break }
            12 { [void]$builder.Append('\f'); break }
            13 { [void]$builder.Append('\r'); break }
            34 { [void]$builder.Append('\"'); break }
            92 { [void]$builder.Append('\\'); break }
            default { $escaped = $false }
        }
        if ($escaped) {
            continue
        }
        if ($code -lt 0x20 -or $code -gt 0x7e) {
            [void]$builder.Append(('\u{0:x4}' -f $code))
        } else {
            [void]$builder.Append($character)
        }
    }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function ConvertTo-CanonicalJsonValue {
    param([AllowNull()]$Value)
    if ($null -eq $Value) { return 'null' }
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
    return [BitConverter]::ToString($digest).Replace('-', '').ToLowerInvariant()
}

function Assert-ExactFields {
    param(
        [Parameter(Mandatory)]$Value,
        [Parameter(Mandatory)][string[]]$Names,
        [Parameter(Mandatory)][string]$Label
    )
    if ($null -eq $Value) {
        throw "VMQA_F1_PLAN_FIELDS: $Label is null"
    }
    $actual = @($Value.PSObject.Properties.Name | Sort-Object)
    $expected = @($Names | Sort-Object)
    if (($actual -join "`n") -cne ($expected -join "`n")) {
        throw "VMQA_F1_PLAN_FIELDS: $Label fields are not exact"
    }
}

function Assert-Sha256 {
    param([AllowNull()]$Value, [string]$Label)
    if (
        $Value -isnot [string] -or
        $Value -cnotmatch $script:ShaPattern
    ) {
        throw "VMQA_F1_PLAN_HASH: $Label is not a SHA-256 digest"
    }
}

function Assert-JsonInteger {
    param(
        [AllowNull()]$Value,
        [long]$Expected,
        [string]$Label
    )
    if (
        ($Value -isnot [int] -and $Value -isnot [long]) -or
        [long]$Value -ne $Expected
    ) {
        throw "VMQA_F1_PLAN_TYPE: $Label is not the exact JSON integer"
    }
}

function Assert-JsonBoolean {
    param(
        [AllowNull()]$Value,
        [bool]$Expected,
        [string]$Label
    )
    if ($Value -isnot [bool] -or $Value -ne $Expected) {
        throw "VMQA_F1_PLAN_TYPE: $Label is not the exact JSON boolean"
    }
}

function Assert-JsonStringExact {
    param(
        [AllowNull()]$Value,
        [string]$Expected,
        [string]$Label
    )
    if ($Value -isnot [string] -or $Value -cne $Expected) {
        throw "VMQA_F1_PLAN_TYPE: $Label is not the exact JSON string"
    }
}

function Assert-NoSecretFields {
    param(
        [AllowNull()]$Value,
        [string]$Label = 'manifest'
    )
    $forbidden = @(
        'key', 'keyBytes', 'keyMaterial', 'rawKey', 'secret',
        'secretBytes', 'seed', 'pem', 'privateKey', 'environment',
        'stdinPayload', 'importPayload'
    )
    if ($null -eq $Value -or $Value -is [string]) {
        return
    }
    if ($Value -is [Collections.IDictionary]) {
        foreach ($name in $Value.Keys) {
            if ($forbidden -ccontains [string]$name) {
                throw "VMQA_F1_PLAN_SECRET: $Label contains $name"
            }
            Assert-NoSecretFields -Value $Value[$name] `
                -Label ($Label + '.' + [string]$name)
        }
        return
    }
    if ($Value -is [Management.Automation.PSCustomObject]) {
        foreach ($property in $Value.PSObject.Properties) {
            if ($forbidden -ccontains $property.Name) {
                throw "VMQA_F1_PLAN_SECRET: $Label contains $($property.Name)"
            }
            Assert-NoSecretFields -Value $property.Value `
                -Label ($Label + '.' + $property.Name)
        }
        return
    }
    if ($Value -is [Collections.IEnumerable]) {
        $index = 0
        foreach ($item in $Value) {
            Assert-NoSecretFields -Value $item `
                -Label ($Label + '[' + $index + ']')
            $index += 1
        }
    }
}

function Get-ExpectedHostDirectories {
    param(
        [Parameter(Mandatory)][int]$ProducerUid,
        [Parameter(Mandatory)][int]$ProducerGid
    )
    $rootPaths = @(
        '/opt',
        '/opt/osl-vmqa',
        '/opt/osl-vmqa/bin',
        '/opt/osl-vmqa/toolchain',
        '/opt/osl-vmqa/toolchain/bin',
        '/var',
        '/var/lib',
        '/var/lib/osl-vmqa',
        '/var/lib/osl-qa'
    )
    $producerPaths = @(
        '/var/lib/osl-qa/private',
        '/var/lib/osl-qa/f1-staging',
        '/var/lib/osl-vmqa/producer-seals'
    )
    return @(
        foreach ($path in $rootPaths) {
            [pscustomobject]@{
                path = $path
                owner = 'root'
                uid = 0
                group = 'root'
                gid = 0
                mode = '0755'
                kind = 'directory'
            }
        }
        foreach ($path in $producerPaths) {
            [pscustomobject]@{
                path = $path
                owner = $script:Producer
                uid = $ProducerUid
                group = $script:Producer
                gid = $ProducerGid
                mode = '0700'
                kind = 'directory'
            }
        }
    )
}

function Assert-HostDirectories {
    param(
        [Parameter(Mandatory)]$Value,
        [Parameter(Mandatory)][int]$ProducerUid,
        [Parameter(Mandatory)][int]$ProducerGid
    )
    if ($Value -isnot [System.Array]) {
        throw 'VMQA_F1_PLAN_HOST: directory inventory is not an array'
    }
    $actual = @($Value)
    $expected = @(
        Get-ExpectedHostDirectories `
            -ProducerUid $ProducerUid -ProducerGid $ProducerGid
    )
    if ($actual.Count -ne $expected.Count) {
        throw 'VMQA_F1_PLAN_HOST: directory inventory is not exact'
    }
    for ($index = 0; $index -lt $expected.Count; $index += 1) {
        $record = $actual[$index]
        Assert-ExactFields -Value $record -Names @(
            'path', 'owner', 'uid', 'group', 'gid', 'mode', 'kind'
        ) -Label "host directory $index"
        Assert-JsonInteger -Value $record.uid `
            -Expected ([long]$expected[$index].uid) `
            -Label "host directory $index uid"
        Assert-JsonInteger -Value $record.gid `
            -Expected ([long]$expected[$index].gid) `
            -Label "host directory $index gid"
        Assert-JsonStringExact -Value $record.mode `
            -Expected $expected[$index].mode `
            -Label "host directory $index mode"
        foreach ($field in @('path', 'owner', 'group', 'kind')) {
            Assert-JsonStringExact -Value $record.$field `
                -Expected $expected[$index].$field `
                -Label "host directory $index $field"
        }
        if (
            $record.kind -cne 'directory'
        ) {
            throw 'VMQA_F1_PLAN_HOST: directory path, owner, or mode differs'
        }
    }
}

function Get-ExpectedGuestPaths {
    param([Parameter(Mandatory)][string]$ExecutableSha256)
    $stage = $script:WindowsStageRoot + '\' + $ExecutableSha256
    return @(
        [pscustomobject]@{ path = $script:WindowsRoot; kind = 'directory' },
        [pscustomobject]@{ path = $script:WindowsRoot + '\bin'; kind = 'directory' },
        [pscustomobject]@{ path = $script:WindowsStageRoot; kind = 'directory' },
        [pscustomobject]@{ path = $stage; kind = 'directory' },
        [pscustomobject]@{ path = $stage + '\bundle'; kind = 'directory' },
        [pscustomobject]@{ path = $stage + '\bundle\outputs'; kind = 'directory' },
        [pscustomobject]@{ path = $stage + '\bundle\build-evidence'; kind = 'directory' },
        [pscustomobject]@{ path = $stage + '\bundle\outputs\dist'; kind = 'directory' },
        [pscustomobject]@{ path = $stage + '\bundle\build-identity.json'; kind = 'file' },
        [pscustomobject]@{ path = $stage + '\bundle\outputs\osl-privacy-hub.exe'; kind = 'file' },
        [pscustomobject]@{ path = $stage + '\bundle\outputs\WebView2Loader.dll'; kind = 'file' },
        [pscustomobject]@{ path = $stage + '\producer-seal.json'; kind = 'file' },
        [pscustomobject]@{ path = $stage + '\staging-receipt.json'; kind = 'file' },
        [pscustomobject]@{ path = $script:WindowsRoot + '\private'; kind = 'directory' },
        [pscustomobject]@{ path = $script:WindowsKeyPath; kind = 'file' }
    )
}

function Assert-GuestAclPlan {
    param(
        [Parameter(Mandatory)]$GuestAcl,
        [Parameter(Mandatory)][string]$ExecutableSha256
    )
    Assert-ExactFields -Value $GuestAcl -Names @('template', 'entries') `
        -Label 'guest ACL plan'
    $template = $GuestAcl.template
    Assert-ExactFields -Value $template -Names @(
        'ownerSid', 'daclProtected', 'inheritanceEnabled', 'ace'
    ) -Label 'guest ACL template'
    Assert-ExactFields -Value $template.ace -Names @(
        'sid', 'type', 'rights', 'inherited', 'propagation'
    ) -Label 'guest ACL template ACE'
    Assert-JsonBoolean -Value $template.daclProtected `
        -Expected $true -Label 'guest ACL protected'
    Assert-JsonBoolean -Value $template.inheritanceEnabled `
        -Expected $false -Label 'guest ACL inheritance'
    Assert-JsonBoolean -Value $template.ace.inherited `
        -Expected $false -Label 'guest ACL ACE inherited'
    Assert-JsonStringExact -Value $template.ownerSid `
        -Expected $script:SystemSid -Label 'guest ACL owner SID'
    Assert-JsonStringExact -Value $template.ace.sid `
        -Expected $script:SystemSid -Label 'guest ACL ACE SID'
    Assert-JsonStringExact -Value $template.ace.type `
        -Expected 'Allow' -Label 'guest ACL ACE type'
    Assert-JsonStringExact -Value $template.ace.rights `
        -Expected 'FullControl' -Label 'guest ACL ACE rights'
    Assert-JsonStringExact -Value $template.ace.propagation `
        -Expected 'None' -Label 'guest ACL ACE propagation'
    if (
        $template.ace.propagation -cne 'None'
    ) {
        throw 'VMQA_F1_PLAN_ACL: template is not protected SYSTEM-only FullControl'
    }
    $expected = @(Get-ExpectedGuestPaths -ExecutableSha256 $ExecutableSha256)
    $entries = @($GuestAcl.entries)
    if ($entries.Count -ne $expected.Count) {
        throw 'VMQA_F1_PLAN_ACL: guest path inventory is not exact'
    }
    for ($index = 0; $index -lt $expected.Count; $index += 1) {
        $entry = $entries[$index]
        Assert-ExactFields -Value $entry -Names @(
            'path', 'kind', 'ownerSid', 'daclProtected',
            'inheritanceEnabled', 'aces'
        ) -Label "guest ACL entry $index"
        $aces = @($entry.aces)
        if ($aces.Count -ne 1) {
            throw 'VMQA_F1_PLAN_ACL: each path must have exactly one ACE'
        }
        Assert-ExactFields -Value $aces[0] -Names @(
            'sid', 'type', 'rights', 'inherited',
            'propagation', 'inheritance'
        ) -Label "guest ACL ACE $index"
        Assert-JsonBoolean -Value $entry.daclProtected `
            -Expected $true -Label "guest ACL entry $index protected"
        Assert-JsonBoolean -Value $entry.inheritanceEnabled `
            -Expected $false -Label "guest ACL entry $index inheritance"
        Assert-JsonBoolean -Value $aces[0].inherited `
            -Expected $false -Label "guest ACL ACE $index inherited"
        $expectedInheritance = $(if ($expected[$index].kind -ceq 'directory') {
            'ContainerInherit,ObjectInherit'
        } else {
            'None'
        })
        Assert-JsonStringExact -Value $entry.path `
            -Expected $expected[$index].path `
            -Label "guest ACL entry $index path"
        Assert-JsonStringExact -Value $entry.kind `
            -Expected $expected[$index].kind `
            -Label "guest ACL entry $index kind"
        Assert-JsonStringExact -Value $entry.ownerSid `
            -Expected $script:SystemSid `
            -Label "guest ACL entry $index owner SID"
        Assert-JsonStringExact -Value $aces[0].sid `
            -Expected $script:SystemSid -Label "guest ACL ACE $index SID"
        Assert-JsonStringExact -Value $aces[0].type `
            -Expected 'Allow' -Label "guest ACL ACE $index type"
        Assert-JsonStringExact -Value $aces[0].rights `
            -Expected 'FullControl' -Label "guest ACL ACE $index rights"
        Assert-JsonStringExact -Value $aces[0].propagation `
            -Expected 'None' -Label "guest ACL ACE $index propagation"
        Assert-JsonStringExact -Value $aces[0].inheritance `
            -Expected $expectedInheritance `
            -Label "guest ACL ACE $index inheritance"
        if (
            $aces[0].inheritance -cne $expectedInheritance
        ) {
            throw 'VMQA_F1_PLAN_ACL: guest path rule differs'
        }
    }
}

function Assert-F1ProvisioningPlan {
    param([Parameter(Mandatory)]$Value)
    Assert-NoSecretFields -Value $Value
    Assert-ExactFields -Value $Value -Names @(
        'schemaVersion', 'kind', 'payload', 'payloadSha256'
    ) -Label 'manifest'
    Assert-JsonInteger -Value $Value.schemaVersion -Expected 1 `
        -Label 'manifest schemaVersion'
    Assert-JsonStringExact -Value $Value.kind -Expected $script:PlanKind `
        -Label 'manifest kind'
    $payload = $Value.payload
    Assert-ExactFields -Value $payload -Names @(
        'status', 'assurance', 'executionPermitted', 'writesPerformed',
        'contract', 'producerAccount', 'hostDirectories', 'programs',
        'toolchain', 'witnessKey', 'release', 'guestAcl', 'runtime',
        'operatorTransitions'
    ) -Label 'payload'
    Assert-JsonBoolean -Value $payload.executionPermitted `
        -Expected $false -Label 'payload executionPermitted'
    Assert-JsonInteger -Value $payload.writesPerformed -Expected 0 `
        -Label 'payload writesPerformed'
    Assert-JsonStringExact -Value $payload.status -Expected 'planned' `
        -Label 'payload status'
    Assert-JsonStringExact -Value $payload.assurance `
        -Expected 'offline-plan-only' -Label 'payload assurance'
    Assert-ExactFields -Value $payload.contract -Names @(
        'provisioningCommit', 'provisioningTree',
        'predecessorSnapshotSha256', 'releaseCommit', 'releaseTree'
    ) -Label 'contract'
    $contractPins = @{
        provisioningCommit = $script:ProvisioningCommit
        provisioningTree = $script:ProvisioningTree
        predecessorSnapshotSha256 = $script:PredecessorSnapshotSha256
        releaseCommit = $script:ReleaseCommit
        releaseTree = $script:ReleaseTree
    }
    foreach ($field in $contractPins.Keys) {
        Assert-JsonStringExact -Value $payload.contract.$field `
            -Expected $contractPins[$field] -Label "contract $field"
    }
    $account = $payload.producerAccount
    Assert-ExactFields -Value $account -Names @(
        'name', 'uid', 'gid', 'home', 'shell', 'locked'
    ) -Label 'producer account'
    Assert-JsonInteger -Value $account.uid -Expected ([long]$account.uid) `
        -Label 'producer uid'
    Assert-JsonInteger -Value $account.gid -Expected ([long]$account.gid) `
        -Label 'producer gid'
    Assert-JsonBoolean -Value $account.locked -Expected $true `
        -Label 'producer locked'
    Assert-JsonStringExact -Value $account.name -Expected $script:Producer `
        -Label 'producer name'
    Assert-JsonStringExact -Value $account.home `
        -Expected $script:ProducerHome -Label 'producer home'
    Assert-JsonStringExact -Value $account.shell `
        -Expected $script:ProducerShell -Label 'producer shell'
    if (
        [long]$account.uid -le 0 -or [long]$account.gid -le 0 -or
        $account.locked -ne $true
    ) {
        throw 'VMQA_F1_PLAN_ACCOUNT: producer account differs'
    }
    Assert-HostDirectories -Value $payload.hostDirectories `
        -ProducerUid ([int]$account.uid) -ProducerGid ([int]$account.gid)
    $programs = @($payload.programs)
    if ($programs.Count -ne $script:ProgramPins.Count) {
        throw 'VMQA_F1_PLAN_PROGRAM: program inventory differs'
    }
    for ($index = 0; $index -lt $programs.Count; $index += 1) {
        $program = $programs[$index]
        $pin = $script:ProgramPins[$index]
        Assert-ExactFields -Value $program -Names @(
            'name', 'path', 'owner', 'group', 'mode', 'sha256'
        ) -Label "program $index"
        Assert-JsonStringExact -Value $program.mode -Expected '0555' `
            -Label "program $index mode"
        foreach ($field in @('name', 'path', 'owner', 'group', 'sha256')) {
            $expectedValue = $(switch ($field) {
                'name' { $pin.name }
                'path' { $script:BinRoot + '/' + $pin.name }
                'owner' { 'root' }
                'group' { 'root' }
                'sha256' { $pin.sha256 }
            })
            Assert-JsonStringExact -Value $program.$field `
                -Expected $expectedValue -Label "program $index $field"
        }
        if (
            $program.sha256 -cne $pin.sha256
        ) {
            throw 'VMQA_F1_PLAN_PROGRAM: fixed program binding differs'
        }
    }
    $toolchain = $payload.toolchain
    Assert-ExactFields -Value $toolchain -Names @(
        'root', 'owner', 'group', 'mode', 'measurementMethod',
        'tools', 'treeSha256'
    ) -Label 'toolchain'
    Assert-JsonStringExact -Value $toolchain.mode -Expected '0755' `
        -Label 'toolchain mode'
    Assert-JsonStringExact -Value $toolchain.root `
        -Expected $script:ToolchainRoot -Label 'toolchain root'
    Assert-JsonStringExact -Value $toolchain.owner -Expected 'root' `
        -Label 'toolchain owner'
    Assert-JsonStringExact -Value $toolchain.group -Expected 'root' `
        -Label 'toolchain group'
    Assert-JsonStringExact -Value $toolchain.measurementMethod `
        -Expected 'independent-offline-sha256' `
        -Label 'toolchain measurement method'
    if (
        $toolchain.measurementMethod -cne 'independent-offline-sha256'
    ) {
        throw 'VMQA_F1_PLAN_TOOLCHAIN: protected root differs'
    }
    $tools = @($toolchain.tools)
    if ($tools.Count -ne $script:ToolNames.Count) {
        throw 'VMQA_F1_PLAN_TOOLCHAIN: tool inventory differs'
    }
    for ($index = 0; $index -lt $tools.Count; $index += 1) {
        $tool = $tools[$index]
        $name = $script:ToolNames[$index]
        Assert-ExactFields -Value $tool -Names @(
            'name', 'path', 'owner', 'group', 'mode', 'sha256'
        ) -Label "tool $index"
        Assert-JsonStringExact -Value $tool.mode -Expected '0555' `
            -Label "tool $name mode"
        Assert-Sha256 -Value $tool.sha256 -Label "tool $name"
        Assert-JsonStringExact -Value $tool.name -Expected $name `
            -Label "tool $index name"
        Assert-JsonStringExact -Value $tool.path `
            -Expected ($script:ToolchainBin + '/' + $name) `
            -Label "tool $name path"
        Assert-JsonStringExact -Value $tool.owner -Expected 'root' `
            -Label "tool $name owner"
        Assert-JsonStringExact -Value $tool.group -Expected 'root' `
            -Label "tool $name group"
        if (
            $tool.mode -cne '0555'
        ) {
            throw 'VMQA_F1_PLAN_TOOLCHAIN: tool path, owner, or mode differs'
        }
    }
    $treeValue = [pscustomobject]@{
        root = $toolchain.root
        measurementMethod = $toolchain.measurementMethod
        tools = $toolchain.tools
    }
    if (
        $toolchain.treeSha256 -cne
            (Get-CanonicalJsonSha256 -Value $treeValue)
    ) {
        throw 'VMQA_F1_PLAN_TOOLCHAIN: tree hash differs'
    }
    $key = $payload.witnessKey
    Assert-ExactFields -Value $key -Names @(
        'path', 'present', 'owner', 'uid', 'group', 'gid',
        'mode', 'keyId', 'keyBytesRead'
    ) -Label 'witness key'
    Assert-JsonBoolean -Value $key.present -Expected $true `
        -Label 'witness key present'
    Assert-JsonInteger -Value $key.uid -Expected ([long]$account.uid) `
        -Label 'witness key uid'
    Assert-JsonInteger -Value $key.gid -Expected ([long]$account.gid) `
        -Label 'witness key gid'
    Assert-JsonBoolean -Value $key.keyBytesRead -Expected $false `
        -Label 'witness key bytes-read marker'
    Assert-JsonStringExact -Value $key.mode -Expected '0600' `
        -Label 'witness key mode'
    Assert-Sha256 -Value $key.keyId -Label 'witness key ID'
    Assert-JsonStringExact -Value $key.path -Expected $script:HostKeyPath `
        -Label 'witness key path'
    Assert-JsonStringExact -Value $key.owner -Expected $script:Producer `
        -Label 'witness key owner'
    Assert-JsonStringExact -Value $key.group -Expected $script:Producer `
        -Label 'witness key group'
    if (
        $key.mode -cne '0600'
    ) {
        throw 'VMQA_F1_PLAN_KEY: metadata differs or exposes bytes'
    }
    $release = $payload.release
    Assert-ExactFields -Value $release -Names @(
        'sourceCommit', 'sourceTree', 'bundleManifestSha256',
        'buildIdentitySha256', 'executableSha256', 'loaderSha256',
        'producerSealSha256', 'terminalSnapshotSha256',
        'sealGeneration', 'previousSealSha256', 'transition', 'stagePath'
    ) -Label 'release'
    foreach ($field in @(
        'bundleManifestSha256', 'buildIdentitySha256', 'executableSha256',
        'loaderSha256', 'producerSealSha256', 'terminalSnapshotSha256',
        'previousSealSha256'
    )) {
        Assert-Sha256 -Value $release.$field -Label "release $field"
    }
    Assert-JsonStringExact -Value $release.sourceCommit `
        -Expected $script:ReleaseCommit -Label 'release source commit'
    Assert-JsonStringExact -Value $release.sourceTree `
        -Expected $script:ReleaseTree -Label 'release source tree'
    Assert-JsonStringExact -Value $release.previousSealSha256 `
        -Expected $script:EmptySha256 -Label 'release predecessor seal'
    Assert-JsonStringExact -Value $release.transition `
        -Expected 'initial' -Label 'release transition'
    Assert-JsonInteger -Value $release.sealGeneration -Expected 1 `
        -Label 'release sealGeneration'
    $expectedStage = $script:HostStageRoot + '/' + $release.executableSha256
    Assert-JsonStringExact -Value $release.stagePath `
        -Expected $expectedStage -Label 'release stage path'
    if (
        $release.stagePath -cne $expectedStage
    ) {
        throw 'VMQA_F1_PLAN_RELEASE: source, hash path, or lineage differs'
    }
    Assert-GuestAclPlan -GuestAcl $payload.guestAcl `
        -ExecutableSha256 $release.executableSha256
    $runtime = $payload.runtime
    Assert-ExactFields -Value $runtime -Names @(
        'authorization', 'authorizationRequired', 'automaticExecution',
        'commandEncoding', 'argv'
    ) -Label 'runtime'
    Assert-JsonBoolean -Value $runtime.authorizationRequired `
        -Expected $true -Label 'runtime authorizationRequired'
    Assert-JsonBoolean -Value $runtime.automaticExecution `
        -Expected $false -Label 'runtime automaticExecution'
    Assert-JsonStringExact -Value $runtime.authorization `
        -Expected 'separate-live-f1-runtime' -Label 'runtime authorization'
    Assert-JsonStringExact -Value $runtime.commandEncoding `
        -Expected 'argv' -Label 'runtime command encoding'
    $expectedArgv = @(
        ($script:BinRoot + '/vmqa-run.sh'),
        'selftest',
        '--vm',
        'OSL-Independent-Client-1',
        '--bundle-dir',
        ($release.stagePath + '/bundle')
    )
    if (
        $runtime.commandEncoding -cne 'argv' -or
        $runtime.argv -isnot [System.Array] -or
        @($runtime.argv).Count -ne $expectedArgv.Count
    ) {
        throw 'VMQA_F1_PLAN_RUNTIME: fixed separately authorized argv differs'
    }
    for ($index = 0; $index -lt $expectedArgv.Count; $index += 1) {
        if (
            $runtime.argv[$index] -isnot [string] -or
            $runtime.argv[$index] -cne $expectedArgv[$index]
        ) {
            throw 'VMQA_F1_PLAN_RUNTIME: argv token differs'
        }
    }
    $transitions = @($payload.operatorTransitions)
    if ($transitions.Count -ne 7) {
        throw 'VMQA_F1_PLAN_TRANSITIONS: exactly seven are required'
    }
    $expectedAuthorities = @(
        'human-host-administrator',
        'human-host-administrator',
        'human-host-administrator',
        'dedicated-producer',
        'dedicated-producer',
        'authorized-guest-provisioner',
        'separate-live-runtime-authority'
    )
    $expectedBindings = @(
        @('/producerAccount'),
        @('/hostDirectories', '/programs'),
        @('/toolchain'),
        @('/witnessKey'),
        @('/release', '/contract'),
        @('/guestAcl', '/release/executableSha256'),
        @('/runtime', '/release', '/guestAcl')
    )
    for ($index = 0; $index -lt 7; $index += 1) {
        $transition = $transitions[$index]
        Assert-ExactFields -Value $transition -Names @(
            'ordinal', 'id', 'authority', 'status',
            'writesPerformed', 'bindsTo'
        ) -Label "transition $index"
        Assert-JsonInteger -Value $transition.ordinal `
            -Expected ([long]($index + 1)) -Label "transition $index ordinal"
        Assert-JsonInteger -Value $transition.writesPerformed `
            -Expected 0 -Label "transition $index writesPerformed"
        Assert-JsonStringExact -Value $transition.id `
            -Expected $script:TransitionIds[$index] `
            -Label "transition $index id"
        Assert-JsonStringExact -Value $transition.authority `
            -Expected $expectedAuthorities[$index] `
            -Label "transition $index authority"
        Assert-JsonStringExact -Value $transition.status `
            -Expected 'planned' -Label "transition $index status"
        $actualBindings = @($transition.bindsTo)
        $expectedTransitionBindings = @($expectedBindings[$index])
        if (
            $transition.bindsTo -isnot [System.Array] -or
            $actualBindings.Count -ne $expectedTransitionBindings.Count
        ) {
            throw 'VMQA_F1_PLAN_TRANSITIONS: binding array differs'
        }
        for (
            $bindingIndex = 0;
            $bindingIndex -lt $expectedTransitionBindings.Count;
            $bindingIndex += 1
        ) {
            Assert-JsonStringExact `
                -Value $actualBindings[$bindingIndex] `
                -Expected $expectedTransitionBindings[$bindingIndex] `
                -Label "transition $index binding $bindingIndex"
        }
        if (
            $transition.status -cne 'planned'
        ) {
            throw (
                'VMQA_F1_PLAN_TRANSITIONS: order, authority, binding, ' +
                'or plan-only status differs'
            )
        }
    }
    Assert-Sha256 -Value $Value.payloadSha256 -Label 'payload'
    if (
        $Value.payloadSha256 -cne
            (Get-CanonicalJsonSha256 -Value $payload)
    ) {
        throw 'VMQA_F1_PLAN_HASH: payload hash differs'
    }
    return [ordered]@{
        schemaVersion = 1
        status = 'valid-plan-only'
        payloadSha256 = $Value.payloadSha256
        writesPerformed = 0
        executionPermitted = $false
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        if ([string]::IsNullOrWhiteSpace($Manifest)) {
            throw 'VMQA_F1_PLAN_ARGUMENT: -Manifest is required'
        }
        $resolved = (
            Resolve-Path -LiteralPath $Manifest -ErrorAction Stop
        ).ProviderPath
        $item = Get-Item -LiteralPath $resolved -Force
        if (
            $item.PSIsContainer -or
            ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)
        ) {
            throw 'VMQA_F1_PLAN_ARGUMENT: manifest must be a real file'
        }
        $raw = [IO.File]::ReadAllText(
            $resolved, [Text.UTF8Encoding]::new($false, $true)
        )
        $value = $raw | ConvertFrom-Json
        Assert-F1ProvisioningPlan -Value $value |
            ConvertTo-Json -Compress
        exit 0
    } catch {
        [Console]::Error.WriteLine(
            'VMQA F1 PLAN VALIDATOR REFUSED: ' + $_.Exception.Message
        )
        exit 9
    }
}
