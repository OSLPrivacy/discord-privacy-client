param(
  [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{7,63}$')][string]$InvocationId,
  [Parameter(Mandatory)][ValidateRange(0,4095)][int]$ChunkIndex,
  [Parameter(Mandatory)][ValidateRange(1,4096)][int]$ChunkCount,
  [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$ChunkSha256,
  [Parameter(Mandatory)][string]$ChunkBase64
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
if($ChunkIndex -ge $ChunkCount -or $ChunkBase64.Length -gt 700000){throw 'artifact chunk contract rejected'}
$root=Join-Path 'C:\ProgramData\OSL-QA\artifact-publish-v1' $InvocationId
[void](New-Item -ItemType Directory -Path $root -Force)
$path=Join-Path $root ('{0:D4}.chunk' -f $ChunkIndex)
$bytes=[Convert]::FromBase64String($ChunkBase64)
try{
  $sha=[Security.Cryptography.SHA256]::Create()
  try{$hash=([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
  if($hash -cne $ChunkSha256){throw 'artifact chunk hash mismatch'}
  if(Test-Path -LiteralPath $path){
    if((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $hash){throw 'existing artifact chunk differs'}
    $status='alreadyStaged'
  }else{
    $temporary="$path.tmp"
    [IO.File]::WriteAllBytes($temporary,$bytes)
    [IO.File]::Move($temporary,$path)
    $status='staged'
  }
  [pscustomobject]@{Schema='whatsapp-artifact-chunk/v1';InvocationId=$InvocationId;Status=$status;ChunkIndex=$ChunkIndex;ChunkCount=$ChunkCount;ChunkSha256=$hash;Terminal=$true}|ConvertTo-Json -Compress
}finally{if($bytes){[Array]::Clear($bytes,0,$bytes.Length)}}
