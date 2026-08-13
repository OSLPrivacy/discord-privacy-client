# TASK 5181 - hand a released download to Windows Attachment Services.
#
# This script is embedded in the OSL binary (include_str!) and handed to
# powershell.exe with -EncodedCommand, so no zone-handoff helper file exists on
# disk for another process to replace.
#
# It reads ONE base64 line from stdin, decodes it to a UTF-8 "KEY=value" block,
# and hands the named local file to the real Windows Attachment Services COM
# server (CLSID_AttachmentServices {4125DD96-E03A-4103-8F70-E0597D803B9C})
# through IAttachmentExecute ({73DB1241-1E85-4581-8E4F-A81E1D0F8C57}):
# SetClientTitle / SetClientGuid / SetSource / SetReferrer / SetFileName /
# SetLocalPath / CheckPolicy / Save. Save() is what writes the Zone.Identifier
# mark and what lets Windows run its own reputation and antivirus defenses over
# the file. It is called EXACTLY ONCE and the count is reported from here, so a
# caller that never reached the handoff cannot inflate it.
#
# The file contents are never read, hashed or sent anywhere by this script:
# there is deliberately no HTTP client here. The only outputs are the volume's
# filesystem name, the HRESULTs, the save count, and the Zone.Identifier stream
# read back off the destination.
#
# Filesystems that cannot retain an alternate data stream (anything that is not
# NTFS or ReFS) are reported as a platform limitation and the handoff is NOT
# attempted: on exFAT and on the 9P/DrvFs WSL volume Save() returns S_OK and
# leaves no usable mark - on 9P it leaves a stray zero-byte
# "<name>:Zone.Identifier" sidecar file next to the download. Claiming a mark
# there would be a lie, and littering the user's folder for a mark that does not
# exist helps nobody.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Write-Field([string] $key, [string] $value) {
    Write-Output ('OSL5181_' + $key + '=' + ($value -replace "`r|`n", ' '))
}

try {
    $payload = [Console]::In.ReadToEnd().Trim()
    $request = @{}
    foreach ($line in ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($payload)) -split "`n")) {
        $line = $line.Trim()
        if ($line.Length -eq 0) { continue }
        $split = $line.IndexOf('=')
        if ($split -lt 1) { continue }
        $request[$line.Substring(0, $split)] = $line.Substring($split + 1)
    }

    $localPath = $request['LOCAL_PATH']
    $fileName = $request['FILE_NAME']
    $source = $request['SOURCE_URL']
    $referrer = $request['REFERRER_URL']
    $title = $request['CLIENT_TITLE']
    $clientGuid = $request['CLIENT_GUID']

    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
namespace OslZoneHandoff {
  [ComImport, Guid("73DB1241-1E85-4581-8E4F-A81E1D0F8C57"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface IAttachmentExecute {
    [PreserveSig] int SetClientTitle([MarshalAs(UnmanagedType.LPWStr)] string pszTitle);
    [PreserveSig] int SetClientGuid(ref Guid guid);
    [PreserveSig] int SetLocalPath([MarshalAs(UnmanagedType.LPWStr)] string pszLocalPath);
    [PreserveSig] int SetFileName([MarshalAs(UnmanagedType.LPWStr)] string pszFileName);
    [PreserveSig] int SetSource([MarshalAs(UnmanagedType.LPWStr)] string pszSource);
    [PreserveSig] int SetReferrer([MarshalAs(UnmanagedType.LPWStr)] string pszReferrer);
    [PreserveSig] int CheckPolicy();
    [PreserveSig] int Prompt(IntPtr hwnd, int prompt, out int paction);
    [PreserveSig] int Save();
    [PreserveSig] int Execute(IntPtr hwnd, [MarshalAs(UnmanagedType.LPWStr)] string pszVerb, out IntPtr phProcess);
    [PreserveSig] int SaveWithUI(IntPtr hwnd);
    [PreserveSig] int ClearClientState();
  }
  public static class Handoff {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool GetVolumePathNameW(string lpszFileName, StringBuilder lpszVolumePathName, uint cch);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool GetVolumeInformationW(string lpRootPathName, StringBuilder lpVolumeNameBuffer, uint nVolumeNameSize, out uint serial, out uint maxComp, out uint flags, StringBuilder lpFileSystemNameBuffer, uint nFileSystemNameSize);

    // The destination volume's own answer, not a guess from the path shape.
    public static string FileSystemOf(string path) {
      StringBuilder root = new StringBuilder(300);
      if (!GetVolumePathNameW(path, root, 300)) { return ""; }
      StringBuilder fs = new StringBuilder(64);
      StringBuilder vol = new StringBuilder(64);
      uint serial, maxc, flags;
      if (!GetVolumeInformationW(root.ToString(), vol, 64, out serial, out maxc, out flags, fs, 64)) { return ""; }
      return fs.ToString();
    }

    // One CoCreateInstance, one Save(). Returns the HRESULT log; the caller
    // counts nothing itself.
    public static List<string> Save(string localPath, string fileName, string source, string referrer, string title, string clientGuid) {
      List<string> log = new List<string>();
      Type coclass = Type.GetTypeFromCLSID(new Guid("4125DD96-E03A-4103-8F70-E0597D803B9C"));
      if (coclass == null) {
        log.Add("HANDOFF_ABSENT=CLSID_AttachmentServices is not registered on this machine");
        return log;
      }
      object created = Activator.CreateInstance(coclass);
      IAttachmentExecute ae = created as IAttachmentExecute;
      if (ae == null) {
        Marshal.ReleaseComObject(created);
        log.Add("HANDOFF_ABSENT=the Attachment Services object does not implement IAttachmentExecute");
        return log;
      }
      Guid client = new Guid(clientGuid);
      log.Add("HR_CLIENT_TITLE=0x" + ae.SetClientTitle(title).ToString("x8"));
      log.Add("HR_CLIENT_GUID=0x" + ae.SetClientGuid(ref client).ToString("x8"));
      log.Add("HR_SOURCE=0x" + ae.SetSource(source).ToString("x8"));
      log.Add("HR_REFERRER=0x" + ae.SetReferrer(referrer).ToString("x8"));
      log.Add("HR_FILE_NAME=0x" + ae.SetFileName(fileName).ToString("x8"));
      log.Add("HR_LOCAL_PATH=0x" + ae.SetLocalPath(localPath).ToString("x8"));
      log.Add("HR_CHECK_POLICY=0x" + ae.CheckPolicy().ToString("x8"));
      int hr = ae.Save();
      log.Add("SAVE_CALLS=1");
      log.Add("HR_SAVE=0x" + hr.ToString("x8"));
      Marshal.ReleaseComObject(created);
      return log;
    }
  }
}
'@

    $filesystem = [OslZoneHandoff.Handoff]::FileSystemOf($localPath)
    Write-Field 'FILESYSTEM' $filesystem
    Write-Field 'WINDOWS_PATH' $localPath
    Write-Field 'FILE_EXISTS_BEFORE' ([bool](Test-Path -LiteralPath $localPath -PathType Leaf)).ToString().ToLower()

    if (-not (Test-Path -LiteralPath $localPath -PathType Leaf)) {
        Write-Field 'STATUS' 'destination_missing'
        Write-Field 'SAVE_CALLS' '0'
        Write-Field 'DETAIL' ('the destination ' + $localPath + ' does not exist on this Windows session')
        exit 0
    }

    if ($filesystem -ne 'NTFS' -and $filesystem -ne 'ReFS') {
        Write-Field 'STATUS' 'platform_unsupported'
        Write-Field 'SAVE_CALLS' '0'
        Write-Field 'ZONE_PRESENT' 'false'
        $detail = if ($filesystem.Length -eq 0) {
            'the destination volume did not report a filesystem'
        } else {
            'the destination filesystem ' + $filesystem + ' does not carry NTFS alternate data streams'
        }
        Write-Field 'DETAIL' $detail
        exit 0
    }

    foreach ($line in [OslZoneHandoff.Handoff]::Save($localPath, $fileName, $source, $referrer, $title, $clientGuid)) {
        Write-Field ($line.Substring(0, $line.IndexOf('='))) ($line.Substring($line.IndexOf('=') + 1))
    }

    Write-Field 'FILE_EXISTS_AFTER' ([bool](Test-Path -LiteralPath $localPath -PathType Leaf)).ToString().ToLower()
    if (Test-Path -LiteralPath $localPath -PathType Leaf) {
        Write-Field 'FILE_LEN_AFTER' ((Get-Item -LiteralPath $localPath).Length.ToString())
    }

    # Read the mark back off the destination. Nothing downstream trusts the
    # Save() HRESULT: only bytes actually present in the stream count.
    try {
        $zone = Get-Content -LiteralPath $localPath -Stream 'Zone.Identifier' -Raw -ErrorAction Stop
        if ($null -eq $zone) { $zone = '' }
        Write-Field 'ZONE_PRESENT' ($(if ($zone.Length -gt 0) { 'true' } else { 'false' }))
        Write-Field 'ZONE_B64' ([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($zone)))
        Write-Field 'ZONE_LEN' ($zone.Length.ToString())
    }
    catch {
        Write-Field 'ZONE_PRESENT' 'false'
        Write-Field 'ZONE_B64' ''
        Write-Field 'ZONE_LEN' '0'
        Write-Field 'ZONE_READ_ERROR' $_.Exception.Message
    }
    Write-Field 'STATUS' 'handed_off'
}
catch {
    Write-Field 'STATUS' 'handoff_error'
    Write-Field 'DETAIL' $_.Exception.Message
}
