$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'production-policy.ps1')

$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$checkout = & git -C $root rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Cannot identify the checked-out commit.' }
$event = Get-Content -LiteralPath $env:GITHUB_EVENT_PATH -Raw | ConvertFrom-Json
$commit = Get-AuthorizedReleaseCommit $event $env:GITHUB_EVENT_NAME $env:GITHUB_REPOSITORY $env:GITHUB_SHA $checkout
$mode = Get-ProductionSigningMode $env:PINGLIDE_SIGNING_THUMBPRINT $env:PINGLIDE_TIMESTAMP_URL
$filename = if ($mode -eq 'signed') { 'Pin-Glide-Setup-x64.exe' } else { 'Pin-Glide-Setup-x64-unsigned.exe' }
$output = Join-Path $root 'target/packaging/windows'
$installer = Join-Path $output $filename
$receiver = Join-Path $root 'target/x86_64-pc-windows-msvc/release/windows-app.exe'

& (Join-Path $PSScriptRoot 'build.ps1') -Unsigned:($mode -eq 'unsigned') -DevelopmentTransport:$false
if (!(Test-Path -LiteralPath $installer -PathType Leaf)) { throw "Missing distributable: $filename" }
& (Join-Path $PSScriptRoot 'installer.test.ps1') -Receiver $receiver -Installer $installer

$expectedSignature = if ($mode -eq 'signed') { 'Valid' } else { 'NotSigned' }
$signature = Get-AuthenticodeSignature -LiteralPath $installer
if ($signature.Status -ne $expectedSignature) {
    throw "Unexpected installer signature status: $($signature.Status), expected $expectedSignature."
}

$metadata = & cargo metadata --locked --no-deps --format-version 1 --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
$version = ($metadata | ConvertFrom-Json).packages |
    Where-Object { $_.name -eq 'windows-app' } | Select-Object -ExpandProperty version
$manifest = [ordered]@{
    commit = $commit
    version = $version
    signing_mode = $mode
    filename = $filename
    sha256 = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
}
$json = $manifest | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $output 'release-candidate.json'), $json)
if ($env:GITHUB_OUTPUT) {
    [IO.File]::AppendAllText($env:GITHUB_OUTPUT, "filename=$filename`n")
}
Write-Output $json
