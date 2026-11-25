$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'package-info.ps1')

$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
@{
    version = Get-WindowsProductVersion $root
    filenames = @{
        signed = Split-Path -Leaf (Get-WindowsInstallerPath $root)
        unsigned = Split-Path -Leaf (Get-WindowsInstallerPath $root -Unsigned)
    }
} | ConvertTo-Json -Compress
