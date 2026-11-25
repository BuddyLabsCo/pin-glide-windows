function Get-WindowsInstallerPath([string] $root, [switch] $Unsigned) {
    $name = if ($Unsigned) { 'Pin-Glide-Setup-x64-unsigned.exe' } else { 'Pin-Glide-Setup-x64.exe' }
    Join-Path $root "target/packaging/windows/$name"
}

function Get-WindowsProductVersion([string] $root) {
    $metadata = & cargo metadata --locked --no-deps --format-version 1 --manifest-path (Join-Path $root 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    ($metadata | ConvertFrom-Json).packages |
        Where-Object { $_.name -eq 'windows-app' } | Select-Object -ExpandProperty version
}
