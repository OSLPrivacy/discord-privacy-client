param(
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')][string]$InvocationId,
  [Parameter(Mandatory)][ValidateRange(1,4096)][int]$ChunkCount,
  [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$GzipSha256,
  [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$ExeSha256,
  [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$LoaderSha256
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$root=Join-Path 'C:\ProgramData\OSL-QA\artifact-publish-v1' $InvocationId
$gzip=Join-Path $root 'OSL Privacy.exe.gz';$exe=Join-Path $root 'OSL Privacy.exe'
$installRoot='C:\Users\osltest\Desktop\OSL Privacy';$loader=Join-Path $installRoot 'WebView2Loader.dll';$loaderPublish=Join-Path $root 'WebView2Loader.dll'
$container='osl-whatsapp-qa-client1';$build="builds/$ExeSha256"
if(-not(Test-Path -LiteralPath $root -PathType Container)){throw 'artifact invocation is unavailable'}
$stream=[IO.File]::Open($gzip,[IO.FileMode]::Create,[IO.FileAccess]::Write,[IO.FileShare]::None)
try{for($i=0;$i-lt $ChunkCount;$i++){$p=Join-Path $root ('{0:D4}.chunk' -f $i);if(-not(Test-Path -LiteralPath $p -PathType Leaf)){throw 'artifact chunk set is incomplete'};$b=[IO.File]::ReadAllBytes($p);try{$stream.Write($b,0,$b.Length)}finally{[Array]::Clear($b,0,$b.Length)}};$stream.Flush($true)}finally{$stream.Dispose()}
if((Get-FileHash -LiteralPath $gzip -Algorithm SHA256).Hash.ToLowerInvariant() -cne $GzipSha256){throw 'assembled artifact hash mismatch'}
$input=[IO.File]::OpenRead($gzip);$output=[IO.File]::Open($exe,[IO.FileMode]::Create,[IO.FileAccess]::Write,[IO.FileShare]::None)
try{$inflate=[IO.Compression.GZipStream]::new($input,[IO.Compression.CompressionMode]::Decompress);try{$inflate.CopyTo($output);$output.Flush($true)}finally{$inflate.Dispose()}}finally{$output.Dispose();$input.Dispose()}
if((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ExeSha256 -or (Get-FileHash -LiteralPath $loader -Algorithm SHA256).Hash.ToLowerInvariant() -cne $LoaderSha256){throw 'publish artifact identity mismatch'}
Copy-Item -LiteralPath $loader -Destination $loaderPublish -Force
if((Get-FileHash -LiteralPath $loaderPublish -Algorithm SHA256).Hash.ToLowerInvariant() -cne $LoaderSha256){throw 'publish loader staging identity mismatch'}
$token=$null
try{
  $token=(Invoke-RestMethod -Headers @{Metadata='true'} -Method GET -Uri 'http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https%3A%2F%2Fstorage.azure.com%2F').access_token
  if(-not $token){throw 'managed identity storage token unavailable'}
  function Publish([string]$path,[string]$name){
    $uri="https://osltestartifactsa7d5.blob.core.windows.net/$container/$build/$name"
    $headers=@{Authorization="Bearer $token";'x-ms-version'='2023-11-03';'x-ms-date'=[DateTime]::UtcNow.ToString('R');'x-ms-blob-type'='BlockBlob'}
    Invoke-RestMethod -Method Put -Uri $uri -Headers $headers -InFile $path -ContentType 'application/octet-stream'|Out-Null
    return $uri
  }
  $exeUri=Publish $exe 'OSL%20Privacy.exe';$loaderUri=Publish $loaderPublish 'WebView2Loader.dll'
  [pscustomobject]@{Schema='whatsapp-artifact-publish/v1';InvocationId=$InvocationId;Status='publishedByManagedIdentity';ExeUri=$exeUri;ExeSha256=$ExeSha256;LoaderUri=$loaderUri;LoaderSha256=$LoaderSha256;ChunkCount=$ChunkCount;Terminal=$true;SecretRead=$false;TokenReturned=$false}|ConvertTo-Json -Compress
}finally{$token=$null}
