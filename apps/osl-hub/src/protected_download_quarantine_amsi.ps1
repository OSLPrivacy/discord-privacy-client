# TASK 5166 - local Windows AMSI submission for a protected download.
#
# This script is embedded in the OSL binary (include_str!) and handed to
# powershell.exe with -EncodedCommand, so no scanner helper file exists on disk
# for another process to replace.
#
# It reads ONE base64 line from stdin, decodes it into a process-private byte
# array, and submits that array to amsi.dll through AmsiScanBuffer. The bytes
# are never written to a Windows-visible file and never leave the machine:
# there is deliberately no HTTP client, no Invoke-WebRequest / Invoke-RestMethod,
# no WebClient, no Submit-MpThreat and no sample upload of any kind here. The
# only other call is Get-MpComputerStatus, a local CIM query for the provider
# identity and signature version that carries no content.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
try {
    $payload = [Console]::In.ReadToEnd().Trim()
    $bytes = [Convert]::FromBase64String($payload)

    Add-Type -Namespace OslAmsi -Name Native -MemberDefinition @'
[DllImport("amsi.dll", CharSet=CharSet.Unicode)]
public static extern int AmsiInitialize(string appName, out System.IntPtr amsiContext);
[DllImport("amsi.dll")]
public static extern void AmsiUninitialize(System.IntPtr amsiContext);
[DllImport("amsi.dll")]
public static extern int AmsiOpenSession(System.IntPtr amsiContext, out System.IntPtr session);
[DllImport("amsi.dll")]
public static extern void AmsiCloseSession(System.IntPtr amsiContext, System.IntPtr session);
[DllImport("amsi.dll", CharSet=CharSet.Unicode)]
public static extern int AmsiScanBuffer(System.IntPtr amsiContext, byte[] buffer, uint length, string contentName, System.IntPtr session, out int result);
'@

    $context = [IntPtr]::Zero
    $initialize = [OslAmsi.Native]::AmsiInitialize('OSL Privacy protected download quarantine', [ref]$context)
    if ($initialize -ne 0) {
        Write-Output 'OSL5166_STATUS=provider_absent'
        Write-Output ('OSL5166_DETAIL=AmsiInitialize returned hr=0x{0:x8}' -f $initialize)
        exit 0
    }

    $session = [IntPtr]::Zero
    $opened = [OslAmsi.Native]::AmsiOpenSession($context, [ref]$session)
    if ($opened -ne 0) {
        [OslAmsi.Native]::AmsiUninitialize($context)
        Write-Output 'OSL5166_STATUS=provider_error'
        Write-Output ('OSL5166_DETAIL=AmsiOpenSession returned hr=0x{0:x8}' -f $opened)
        exit 0
    }

    $result = 0
    $scanned = [OslAmsi.Native]::AmsiScanBuffer($context, $bytes, [uint32]$bytes.Length, 'osl-protected-download', $session, [ref]$result)
    [OslAmsi.Native]::AmsiCloseSession($context, $session)
    [OslAmsi.Native]::AmsiUninitialize($context)
    if ($scanned -ne 0) {
        Write-Output 'OSL5166_STATUS=provider_error'
        Write-Output ('OSL5166_DETAIL=AmsiScanBuffer returned hr=0x{0:x8}' -f $scanned)
        exit 0
    }

    # Hashed over the exact array that was handed to AmsiScanBuffer, so the
    # caller can prove the scanner saw the same bytes it is about to release.
    $digest = [System.Security.Cryptography.SHA256]::Create()
    $hash = ($digest.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') }) -join ''

    Write-Output 'OSL5166_STATUS=scanned'
    Write-Output ('OSL5166_AMSI_RESULT=' + $result)
    Write-Output ('OSL5166_SCANNED_LEN=' + $bytes.Length)
    Write-Output ('OSL5166_SCANNED_SHA256=' + $hash)

    $status = Get-MpComputerStatus
    Write-Output ('OSL5166_PROVIDER=Microsoft Defender Antivirus ' + $status.AMProductVersion)
    Write-Output ('OSL5166_ENGINE_VERSION=' + $status.AMEngineVersion)
    Write-Output ('OSL5166_SIGNATURE_VERSION=' + $status.AntivirusSignatureVersion)
    Write-Output ('OSL5166_SIGNATURE_UPDATED_UNIX=' + [int64]([DateTimeOffset]$status.AntivirusSignatureLastUpdated).ToUnixTimeSeconds())
}
catch {
    Write-Output 'OSL5166_STATUS=provider_error'
    Write-Output ('OSL5166_DETAIL=' + ($_.Exception.Message -replace "`r|`n", ' '))
}
