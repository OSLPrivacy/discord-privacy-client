# Verify a Windows release by rebuilding it

This guide lets an independent verifier determine whether a published OSL Privacy Windows release
was built from its matching public source. It compares the `osl-privacy-hub.exe` that the released
NSIS installer carries with the one made from that tag. It does **not** prove that the source is
safe; it proves whether the published executable corresponds byte-for-byte to that source and this
documented build recipe.

## What this verifies

For a release tag such as `hub-v0.1.0`, a successful comparison establishes all of the following:

- the tag resolves to the source used for the rebuild;
- the public GitHub Release supplies exactly one Windows installer;
- that installer contains exactly one `osl-privacy-hub.exe`; and
- the SHA-256 hash of that executable equals the hash of the executable extracted from the
  independently rebuilt installer.

The updater signature and installer bytes themselves are deliberately not compared here. The
rebuild generates a throwaway updater-signing key, and installer packaging can contain data outside
the application executable. The updater public key and release signatures are verified separately
by the release policy.

## Requirements

Use a clean **Windows** machine or Windows VM with PowerShell, Git, 7-Zip, Node.js **22**, Rustup,
and the MSVC build tools installed. The release currently uses Rust **1.88.0**. Run the commands in
an elevated PowerShell only if your system needs elevation to create `D:\a`; no OSL credential or
release-signing key is required.

The build directory is not cosmetic. Rust embeds the absolute release target path in the executable.
The released and reproducibility builds use
`D:\a\discord-privacy-client\discord-privacy-client\apps\osl-hub\target`; building somewhere
else produces different bytes. Do not substitute a personal clone directory, a temporary directory,
or a junction whose resolved path differs.

## Rebuild and compare

Set the release tag once, then run this PowerShell block. Replace `hub-v0.1.0` only with a published
`hub-v*` release tag. These commands intentionally refuse an existing target directory, an
ambiguous release asset, or an ambiguous executable.

```powershell
$ErrorActionPreference = "Stop"
$tag = "hub-v0.1.0"
$repoUrl = "https://github.com/OSLPrivacy/discord-privacy-client.git"
$repoRoot = "D:\a\discord-privacy-client\discord-privacy-client"

if ($tag -notmatch "^hub-v[0-9A-Za-z.+-]{1,64}$") {
  throw "Use a bounded hub-v* release tag"
}
if (Test-Path $repoRoot) {
  throw "Clean-room path already exists: $repoRoot"
}

New-Item -ItemType Directory -Force -Path (Split-Path $repoRoot) | Out-Null
git clone $repoUrl $repoRoot
Set-Location $repoRoot
git fetch --force --tags origin "refs/tags/$tag`:refs/tags/$tag"
git checkout --detach "$tag^{commit}"

$config = Get-Content apps/osl-hub/tauri.conf.json -Raw | ConvertFrom-Json
if ($tag -cne "hub-v$($config.version)") {
  throw "Release tag does not match the tagged source version"
}

rustup toolchain install 1.88.0 --profile minimal
rustup override set 1.88.0
if ((rustc --version) -notmatch "^rustc 1\.88\.0 ") {
  throw "Expected Rust 1.88.0"
}
if ((node --version) -notmatch "^v22\.") {
  throw "Expected Node.js 22"
}

New-Item -ItemType Directory -Path candidate | Out-Null
gh release download $tag --repo OSLPrivacy/discord-privacy-client --pattern "*.exe" --dir candidate
$releasedInstallers = @(Get-ChildItem candidate -File -Filter "*.exe")
if ($releasedInstallers.Count -ne 1) {
  throw "Release must contain exactly one Windows installer, got $($releasedInstallers.Count)"
}

New-Item -ItemType Directory -Path candidate-extracted | Out-Null
7z x "$($releasedInstallers[0].FullName)" -ocandidate-extracted -y | Out-Null
$releasedExecutables = @(Get-ChildItem candidate-extracted -Recurse -File -Filter "osl-privacy-hub.exe")
if ($releasedExecutables.Count -ne 1) {
  throw "Released installer must contain exactly one OSL Privacy Hub executable"
}

Push-Location apps/osl-hub-ui
npm ci
npm run build
Pop-Location

$target = "apps/osl-hub/target"
if (Test-Path $target) {
  throw "Reproducible build target directory already exists"
}
npm install -g @tauri-apps/cli@2.11.4
$signingKey = Join-Path $env:TEMP "osl-repro-updater.key"
tauri signer generate --ci -p "" -w $signingKey
if ($LASTEXITCODE -ne 0) {
  throw "Could not generate the throwaway updater signing key"
}
$env:TAURI_SIGNING_PRIVATE_KEY = $signingKey
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""

Push-Location apps/osl-hub
tauri build --features desktop
if ($LASTEXITCODE -ne 0) {
  throw "The release build command failed"
}
Pop-Location

$rebuiltInstallers = @(Get-ChildItem apps/osl-hub/target/release/bundle/nsis -File -Filter "*.exe")
if ($rebuiltInstallers.Count -ne 1) {
  throw "Rebuild must produce exactly one Windows installer, got $($rebuiltInstallers.Count)"
}
New-Item -ItemType Directory -Path rebuilt-extracted | Out-Null
7z x "$($rebuiltInstallers[0].FullName)" -orebuilt-extracted -y | Out-Null
$rebuiltExecutables = @(Get-ChildItem rebuilt-extracted -Recurse -File -Filter "osl-privacy-hub.exe")
if ($rebuiltExecutables.Count -ne 1) {
  throw "Rebuilt installer must contain exactly one OSL Privacy Hub executable"
}

$releasedHash = (Get-FileHash $releasedExecutables[0].FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$rebuiltHash = (Get-FileHash $rebuiltExecutables[0].FullName -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host "released executable SHA-256: $releasedHash"
Write-Host "rebuilt executable SHA-256:  $rebuiltHash"
if ($releasedHash -cne $rebuiltHash) {
  throw "FAIL: released executable bytes do not reproduce from exact source"
}
Write-Host "PASS: released executable bytes reproduce from exact source"
```

`gh` must be authenticated only to download the public release if GitHub asks for it; it needs no
repository write permission. You may instead download the one `.exe` release asset by browser into
`candidate`, then omit the `gh release download` command.

## Why the exact recipe matters

The shipped executable is not a bare `cargo build` output. `tauri build --features desktop` enables
Tauri's `custom-protocol` feature and patches the staged executable with NSIS bundle-type
information. The linked output in `target\release` is therefore not the file users receive: it still
has the unpatched bundle-type placeholder. Extract and hash the executable from the NSIS installer
on both sides, exactly as the commands above do.

Do not omit the exact `@tauri-apps/cli@2.11.4` install. A different Tauri CLI can change feature
selection or bundling, turning a byte mismatch into an uninformative toolchain difference. The
throwaway updater key is necessary only because the tagged configuration creates updater artifacts;
it does not sign or alter the executable being compared.

## Interpreting the result

`PASS` means the released executable was reproducible from the matching public tag with this pinned
environment. `FAIL` is meaningful: preserve the tag, the two displayed hashes, Windows version,
Node version, `rustc -vV`, Tauri CLI version, and command output, then report them in the release
issue tracker. Do not treat a mismatch as proof of malicious source or a failed comparison as a
reason to silently replace a published release.

If no published `hub-v*` release exists yet, there is no shipped installer to verify. The procedure
is intentionally unproven until the first published release provides an artifact to compare.
