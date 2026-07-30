$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot 'vmqa-f1-windows-provisioning-preflight.ps1')

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

function New-Rule {
    param(
        [string]$Sid = 'S-1-5-18',
        [string]$Type = 'Allow',
        [string]$Rights = 'FullControl',
        [bool]$Inherited = $false,
        [string]$Inheritance = 'ContainerInherit, ObjectInherit',
        [string]$Propagation = 'None'
    )
    return [pscustomobject]@{
        sid = $Sid
        type = $Type
        rights = $Rights
        inherited = $Inherited
        inheritance = $Inheritance
        propagation = $Propagation
    }
}

function New-AclRecord {
    param(
        [string]$Owner = 'S-1-5-18',
        [bool]$Protected = $true,
        [object[]]$Rules = @((New-Rule))
    )
    return [pscustomobject]@{
        ownerSid = $Owner
        protected = $Protected
        rules = $Rules
    }
}

function New-SnapshotDirectory {
    param([string]$Path, [int64]$Inode)
    return [pscustomobject]@{
        path = $Path
        type = 'directory'
        device = 1
        inode = $Inode
        mode = 448
        linkCount = 2
    }
}

function New-SnapshotFile {
    param(
        [string]$Path,
        [int64]$Inode,
        [int64]$Size,
        [string]$Sha
    )
    return [pscustomobject]@{
        path = $Path
        type = 'file'
        device = 1
        inode = $Inode
        mode = 420
        linkCount = 1
        sizeBytes = $Size
        sha256 = $Sha
    }
}

function New-TerminalFixture {
    $stage = '/var/lib/osl-qa/f1-staging/' + ('b' * 64)
    $snapshot = [pscustomobject]@{
        stageDirectory = New-SnapshotDirectory -Path $stage -Inode 10
        bundleDirectory = New-SnapshotDirectory `
            -Path ($stage + '/bundle') -Inode 11
        outputsDirectory = New-SnapshotDirectory `
            -Path ($stage + '/bundle/outputs') -Inode 12
        identity = New-SnapshotFile `
            -Path ($stage + '/bundle/build-identity.json') `
            -Inode 20 -Size 100 -Sha ('a' * 64)
        executable = New-SnapshotFile `
            -Path ($stage + '/bundle/outputs/osl-privacy-hub.exe') `
            -Inode 21 -Size 200 -Sha ('b' * 64)
        loader = New-SnapshotFile `
            -Path ($stage + '/bundle/outputs/WebView2Loader.dll') `
            -Inode 22 -Size 300 -Sha ('c' * 64)
        producerSeal = New-SnapshotFile `
            -Path ($stage + '/producer-seal.json') `
            -Inode 23 -Size 400 -Sha ('d' * 64)
    }
    return [pscustomobject]@{
        receipt = [pscustomobject]@{
            executable = [pscustomobject]@{
                path = $snapshot.executable.path
                sha256 = ('b' * 64)
                sizeBytes = 200
            }
            loader = [pscustomobject]@{
                path = $snapshot.loader.path
                sha256 = ('c' * 64)
                sizeBytes = 300
            }
            producerSeal = [pscustomobject]@{
                path = $snapshot.producerSeal.path
            }
            terminalSnapshot = $snapshot
            terminalSnapshotSha256 = (
                '0bbefcb40b6fb4231b645d9f75ed48fe' +
                '7ae40c6202c2365a916cdcc9d885549e'
            )
        }
        identityRead = [pscustomobject]@{
            sha256 = ('a' * 64)
            sizeBytes = 100
        }
        sealRead = [pscustomobject]@{
            sha256 = ('d' * 64)
            sizeBytes = 400
        }
        exeHash = [pscustomobject]@{
            sha256 = ('b' * 64)
            sizeBytes = 200
        }
        loaderHash = [pscustomobject]@{
            sha256 = ('c' * 64)
            sizeBytes = 300
        }
    }
}

Expect-Pass 'directory SYSTEM-only ACL accepts' {
    Assert-SystemOnlyAclRecord -Record (New-AclRecord) `
        -IsDirectory $true -Label 'fixture directory'
}
Expect-Pass 'file SYSTEM-only ACL accepts' {
    Assert-SystemOnlyAclRecord -Record (
        New-AclRecord -Rules @((New-Rule -Inheritance 'None'))
    ) -IsDirectory $false -Label 'fixture file'
}
Expect-Refusal 'wrong owner refuses' 'OWNER' {
    Assert-SystemOnlyAclRecord -Record (
        New-AclRecord -Owner 'S-1-5-32-544'
    ) -IsDirectory $true -Label 'fixture'
}
Expect-Refusal 'inherited DACL refuses' 'ACL_INHERITED' {
    Assert-SystemOnlyAclRecord -Record (
        New-AclRecord -Protected $false
    ) -IsDirectory $true -Label 'fixture'
}
Expect-Refusal 'extra principal refuses' 'ACL_COUNT' {
    Assert-SystemOnlyAclRecord -Record (
        New-AclRecord -Rules @(
            (New-Rule),
            (New-Rule -Sid 'S-1-5-32-544')
        )
    ) -IsDirectory $true -Label 'fixture'
}
Expect-Refusal 'wrong rights refuse' 'ACL_RIGHTS' {
    Assert-SystemOnlyAclRecord -Record (
        New-AclRecord -Rules @((New-Rule -Rights 'ReadAndExecute'))
    ) -IsDirectory $true -Label 'fixture'
}
Expect-Refusal 'inherited ACE refuses' 'ACL_RIGHTS' {
    Assert-SystemOnlyAclRecord -Record (
        New-AclRecord -Rules @((New-Rule -Inherited $true))
    ) -IsDirectory $true -Label 'fixture'
}
Expect-Refusal 'wrong directory inheritance refuses' 'ACL_RIGHTS' {
    Assert-SystemOnlyAclRecord -Record (
        New-AclRecord -Rules @((New-Rule -Inheritance 'None'))
    ) -IsDirectory $true -Label 'fixture'
}
Expect-Refusal 'unknown ACL field refuses' 'FIELDS' {
    $record = New-AclRecord
    $record | Add-Member -NotePropertyName caller -NotePropertyValue $true
    Assert-SystemOnlyAclRecord -Record $record `
        -IsDirectory $true -Label 'fixture'
}

$observed = [pscustomobject]@{
    sha256 = ('a' * 64)
    sizeBytes = 123
}
$expected = [pscustomobject]@{
    path = 'fixed'
    sha256 = ('a' * 64)
    sizeBytes = 123
}
Expect-Pass 'exact receipt hash and size accept' {
    Assert-HashAndSize -Observed $observed -Expected $expected -Label 'fixture'
}
Expect-Refusal 'wrong receipt hash refuses' 'HASH' {
    $wrong = [pscustomobject]@{
        path = 'fixed'
        sha256 = ('b' * 64)
        sizeBytes = 123
    }
    Assert-HashAndSize -Observed $observed -Expected $wrong -Label 'fixture'
}
Expect-Refusal 'wrong receipt size refuses' 'HASH' {
    $wrong = [pscustomobject]@{
        path = 'fixed'
        sha256 = ('a' * 64)
        sizeBytes = 124
    }
    Assert-HashAndSize -Observed $observed -Expected $wrong -Label 'fixture'
}
Expect-Refusal 'missing fixed item refuses' 'MISSING' {
    Assert-FixedItem `
        -LiteralPath 'Z:\VMQA-this-path-must-not-exist\f1' `
        -IsDirectory $true -Label 'missing fixture'
}
Expect-Pass 'canonical snapshot hash matches independent Python fixture' {
    $fixture = New-TerminalFixture
    $actual = Get-CanonicalJsonSha256 `
        -Value $fixture.receipt.terminalSnapshot
    if ($actual -cne $fixture.receipt.terminalSnapshotSha256) {
        throw "canonical hash mismatch: $actual"
    }
}
Expect-Pass 'complete terminal snapshot accepts' {
    $fixture = New-TerminalFixture
    Assert-TerminalSnapshot -Receipt $fixture.receipt `
        -IdentityRead $fixture.identityRead -SealRead $fixture.sealRead `
        -ExeHash $fixture.exeHash -LoaderHash $fixture.loaderHash
}
Expect-Refusal 'terminal snapshot hash mutation refuses' 'TERMINAL_SNAPSHOT' {
    $fixture = New-TerminalFixture
    $fixture.receipt.terminalSnapshotSha256 = ('0' * 64)
    Assert-TerminalSnapshot -Receipt $fixture.receipt `
        -IdentityRead $fixture.identityRead -SealRead $fixture.sealRead `
        -ExeHash $fixture.exeHash -LoaderHash $fixture.loaderHash
}
Expect-Refusal 'coherent terminal executable rewrite refuses' 'TERMINAL_SNAPSHOT' {
    $fixture = New-TerminalFixture
    $fixture.receipt.terminalSnapshot.executable.sha256 = ('e' * 64)
    $fixture.receipt.terminalSnapshotSha256 = Get-CanonicalJsonSha256 `
        -Value $fixture.receipt.terminalSnapshot
    Assert-TerminalSnapshot -Receipt $fixture.receipt `
        -IdentityRead $fixture.identityRead -SealRead $fixture.sealRead `
        -ExeHash $fixture.exeHash -LoaderHash $fixture.loaderHash
}
Expect-Refusal 'coherent caller stage prefix rewrite refuses' 'TERMINAL_SNAPSHOT' {
    $fixture = New-TerminalFixture
    $snapshot = $fixture.receipt.terminalSnapshot
    $fixedStage = [string]$snapshot.stageDirectory.path
    $callerStage = '/caller/f1-staging/' + ('b' * 64)
    foreach ($record in @(
        $snapshot.stageDirectory,
        $snapshot.bundleDirectory,
        $snapshot.outputsDirectory,
        $snapshot.identity,
        $snapshot.executable,
        $snapshot.loader,
        $snapshot.producerSeal
    )) {
        $record.path = ([string]$record.path).Replace(
            $fixedStage, $callerStage
        )
    }
    $fixture.receipt.executable.path = $snapshot.executable.path
    $fixture.receipt.loader.path = $snapshot.loader.path
    $fixture.receipt.producerSeal.path = $snapshot.producerSeal.path
    $fixture.receipt.terminalSnapshotSha256 = Get-CanonicalJsonSha256 `
        -Value $snapshot
    Assert-TerminalSnapshot -Receipt $fixture.receipt `
        -IdentityRead $fixture.identityRead -SealRead $fixture.sealRead `
        -ExeHash $fixture.exeHash -LoaderHash $fixture.loaderHash
}
Expect-Refusal 'unknown terminal snapshot field refuses' 'FIELDS' {
    $fixture = New-TerminalFixture
    $fixture.receipt.terminalSnapshot |
        Add-Member -NotePropertyName caller -NotePropertyValue $true
    Assert-TerminalSnapshot -Receipt $fixture.receipt `
        -IdentityRead $fixture.identityRead -SealRead $fixture.sealRead `
        -ExeHash $fixture.exeHash -LoaderHash $fixture.loaderHash
}
Expect-Pass 'source pins and fixed root are exact' {
    if (
        $script:PinnedCommit -cne '1f745c85bb23cf79a956aa87d623905e20f83cf1' -or
        $script:PinnedTree -cne '1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a' -or
        $script:StageRoot -cne 'C:\ProgramData\OSL-QA\f1-staging' -or
        $script:SystemSid -cne 'S-1-5-18'
    ) {
        throw 'fixed source, root, or identity pin drifted'
    }
}
Expect-Pass 'SYSTEM identity accepts' {
    Assert-SystemIdentitySid -Sid 'S-1-5-18'
}
Expect-Refusal 'wrong Windows identity refuses' 'IDENTITY' {
    Assert-SystemIdentitySid -Sid 'S-1-5-32-544'
}
Expect-Pass 'exact commit and tree accept' {
    Assert-PinnedSource `
        -Commit '1f745c85bb23cf79a956aa87d623905e20f83cf1' `
        -Tree '1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a' `
        -Label 'fixture'
}
Expect-Refusal 'wrong commit pin refuses' 'SOURCE_PIN' {
    Assert-PinnedSource -Commit ('0' * 40) `
        -Tree '1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a' `
        -Label 'fixture'
}
Expect-Refusal 'wrong tree pin refuses' 'SOURCE_PIN' {
    Assert-PinnedSource `
        -Commit '1f745c85bb23cf79a956aa87d623905e20f83cf1' `
        -Tree ('0' * 40) -Label 'fixture'
}
Expect-Pass 'production script exposes no caller parameter block' {
    $program = Join-Path $PSScriptRoot `
        'vmqa-f1-windows-provisioning-preflight.ps1'
    $source = [IO.File]::ReadAllText($program)
    $tokens = $null
    $parseErrors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile(
        $program, [ref]$tokens, [ref]$parseErrors
    )
    if (
        $parseErrors.Count -ne 0 -or
        $null -ne $ast.ParamBlock -or
        $source -notmatch 'WindowsIdentity\]::GetCurrent\(\)' -or
        $source -notmatch 'Assert-SystemIdentitySid -Sid \$currentSid' -or
        $source -match 'f1-native-witness\.key' -or
        $source -notmatch 'ReparsePoint' -or
        $source -notmatch 'bundle entries are not exact' -or
        $source -notmatch 'output entries are not exact'
    ) {
        throw 'caller authority or SYSTEM identity guard drifted'
    }
}

Write-Output ("passed={0} failed={1}" -f $script:Passed, $script:Failed)
if ($script:Failed -ne 0) {
    exit 1
}
