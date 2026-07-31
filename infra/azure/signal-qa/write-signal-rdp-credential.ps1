param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('TERMSRV/20.225.137.164', 'TERMSRV/20.114.72.95')]
  [string]$Target,

  [Parameter(Mandatory = $true)]
  [ValidateSet('OSLSIGCLIENT1\osltest', 'OSLSIGCLIENT2\osltest')]
  [string]$Username
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$expected = @{
  'TERMSRV/20.225.137.164' = 'OSLSIGCLIENT1\osltest'
  'TERMSRV/20.114.72.95' = 'OSLSIGCLIENT2\osltest'
}
if ($expected[$Target] -cne $Username) { throw 'RDP target and username do not identify the same dedicated Signal VM' }

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using FILETIME = System.Runtime.InteropServices.ComTypes.FILETIME;
public static class OslSignalCredentialNative {
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  public struct Credential {
    public UInt32 Flags; public UInt32 Type;
    [MarshalAs(UnmanagedType.LPWStr)] public string TargetName;
    [MarshalAs(UnmanagedType.LPWStr)] public string Comment;
    public FILETIME LastWritten; public UInt32 CredentialBlobSize; public IntPtr CredentialBlob;
    public UInt32 Persist; public UInt32 AttributeCount; public IntPtr Attributes;
    [MarshalAs(UnmanagedType.LPWStr)] public string TargetAlias;
    [MarshalAs(UnmanagedType.LPWStr)] public string UserName;
  }
  [DllImport("advapi32.dll", EntryPoint = "CredWriteW", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern bool CredWrite(ref Credential credential, UInt32 flags);
}
'@

$password = [Console]::In.ReadToEnd().TrimEnd("`r", "`n")
if ([string]::IsNullOrWhiteSpace($password) -or $password.Length -gt 512) { throw 'Key Vault returned an invalid RDP credential' }
$bytes = $null; $blob = [IntPtr]::Zero
try {
  $bytes = [Text.Encoding]::Unicode.GetBytes($password)
  $blob = [Runtime.InteropServices.Marshal]::AllocHGlobal($bytes.Length)
  [Runtime.InteropServices.Marshal]::Copy($bytes, 0, $blob, $bytes.Length)
  $credential = [OslSignalCredentialNative+Credential]::new()
  $credential.Type = 1; $credential.TargetName = $Target
  $credential.Comment = 'OSL Signal QA RDP credential mapping'
  $credential.CredentialBlobSize = $bytes.Length; $credential.CredentialBlob = $blob
  $credential.Persist = 2; $credential.UserName = $Username
  if (-not [OslSignalCredentialNative]::CredWrite([ref]$credential, 0)) { throw 'RDP credential write failed' }
  [pscustomobject]@{ Status = 'credential-mapped-without-disclosure'; Target = $Target; Username = $Username } | ConvertTo-Json -Compress
} finally {
  $password = $null
  if ($bytes) { [Array]::Clear($bytes, 0, $bytes.Length) }
  if ($blob -ne [IntPtr]::Zero) {
    for ($index = 0; $index -lt $credential.CredentialBlobSize; $index++) { [Runtime.InteropServices.Marshal]::WriteByte($blob, $index, 0) }
    [Runtime.InteropServices.Marshal]::FreeHGlobal($blob)
  }
}
