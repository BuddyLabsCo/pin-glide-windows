param(
    [switch] $Unsigned,
    [string] $SignTool = 'signtool.exe'
)

$ErrorActionPreference = 'Stop'
$origin = $null
if (![Uri]::TryCreate($env:PINGLIDE_SERVICE_URL, [UriKind]::Absolute, [ref] $origin) -or
    $origin.Scheme -ne 'https' -or !$origin.Host -or $origin.UserInfo -or
    $origin.AbsolutePath -ne '/' -or $origin.Query -or $origin.Fragment) {
    throw 'Set PINGLIDE_SERVICE_URL to the HTTPS Service origin.'
}

$makensis = (Get-Command makensis.exe -CommandType Application).Source
$signingDefines = @()
if (!$Unsigned) {
    $thumbprint = $env:PINGLIDE_SIGNING_THUMBPRINT
    $timestamp = $env:PINGLIDE_TIMESTAMP_URL
    if ($thumbprint -notmatch '^[0-9a-f]{40}$' -or
        $timestamp -notmatch '^https?://[a-z0-9.-]+(?::[0-9]+)?(?:/[a-z0-9._/-]*)?$') {
        throw 'Set PINGLIDE_SIGNING_THUMBPRINT and PINGLIDE_TIMESTAMP_URL for a signed release.'
    }
    $signer = (Get-Command $SignTool -CommandType Application).Source
    $signArguments = @('sign', '/sha1', $thumbprint, '/fd', 'SHA256', '/tr', $timestamp, '/td', 'SHA256')
    $verifyArguments = @('verify', '/pa', '/tw')
    $signingDefines = @(
        "/DSIGNTOOL=$signer"
        "/DSIGN_ARGS=$($signArguments -join ' ')"
        "/DVERIFY_ARGS=$($verifyArguments -join ' ')"
    )
}

function Sign-Executable([string] $path) {
    & $signer @signArguments $path
    if ($LASTEXITCODE -ne 0) { throw "Signing failed for $path" }
    & $signer @verifyArguments $path
    if ($LASTEXITCODE -ne 0) { throw "Signature verification failed for $path" }
}

$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$target = Join-Path $root 'target'
$output = Join-Path $target 'packaging/windows'
$name = if ($Unsigned) { 'Pin-Glide-Setup-x64-unsigned.exe' } else { 'Pin-Glide-Setup-x64.exe' }
$architecture = 'x86_64-pc-windows-msvc'

$previousRustFlags = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location -LiteralPath $root
try {
    $env:CARGO_ENCODED_RUSTFLAGS = @('-C', 'target-feature=+crt-static') -join [char]31
    & cargo build --locked -p windows-app --release --target $architecture --target-dir $target
    if ($LASTEXITCODE -ne 0) { throw 'Cargo release build failed.' }
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousRustFlags
    Pop-Location
}

New-Item -ItemType Directory -Force $output | Out-Null
$stageName = [guid]::NewGuid().ToString('N')
$stage = (New-Item -ItemType Directory (Join-Path $output $stageName)).FullName
$receiver = Join-Path $stage 'windows-app.exe'
$installer = Join-Path $stage $name
try {
    Copy-Item -LiteralPath (Join-Path $target "$architecture/release/windows-app.exe") -Destination $receiver
    if (!$Unsigned) { Sign-Executable $receiver }
    & $makensis /V2 /WX "/DRECEIVER=$receiver" "/DOUTPUT=$installer" @signingDefines (Join-Path $PSScriptRoot 'installer.nsi')
    if ($LASTEXITCODE -ne 0) { throw 'MakeNSIS failed.' }
    if (!$Unsigned) { Sign-Executable $installer }
    $artifact = Join-Path $output $name
    Move-Item -LiteralPath $installer -Destination $artifact -Force
    Write-Output $artifact
} finally {
    Remove-Item -LiteralPath $receiver, $installer -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stage
}
