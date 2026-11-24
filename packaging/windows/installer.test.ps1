param(
    [Parameter(Mandatory)]
    [string] $Receiver
)

$ErrorActionPreference = 'Stop'
$receiverPath = (Resolve-Path -LiteralPath $Receiver).Path
$target = (New-Item -ItemType Directory -Force (Join-Path $PSScriptRoot '../../target')).FullName
$product = "PinGlidePackageTest-$([guid]::NewGuid().ToString('N'))"
$fixture = (New-Item -ItemType Directory (Join-Path $target $product)).FullName
$installation = Join-Path $fixture "Programs/$product"
$data = Join-Path $fixture $product
$binary = Join-Path $installation 'windows-app.exe'
$uninstaller = Join-Path $installation 'Uninstall.exe'
$shortcut = Join-Path $fixture "$product.lnk"
$uninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$product"
$testRegistry = "Software\$product"
$runKey = "HKCU:\$testRegistry\Run"
$approvalKey = "HKCU:\$testRegistry\StartupApproved"

function Assert($condition, [string] $message) {
    if (!$condition) { throw $message }
}

function Build-Installer($payload, $output) {
    & makensis /V2 /WX "/DPRODUCT_NAME=$product" "/DUSER_FILES=$fixture" "/DMENU_DIR=$fixture" `
        "/DRUN_KEY=$testRegistry\Run" "/DAPPROVAL_KEY=$testRegistry\StartupApproved" `
        "/DRECEIVER=$payload" "/DOUTPUT=$output" (Join-Path $PSScriptRoot 'installer.nsi')
    Assert ($LASTEXITCODE -eq 0) 'MakeNSIS failed'
}

function Invoke-Installer($executable, $arguments = '/S') {
    (Start-Process -FilePath $executable -ArgumentList $arguments -Wait -PassThru).ExitCode
}

function Assert-Data {
    foreach ($file in $sentinels.Keys) {
        $actual = (Get-FileHash -LiteralPath (Join-Path $data $file)).Hash
        Assert ($actual -eq $sentinels[$file]) "$file changed during upgrade or blocked uninstall"
    }
}

try {
    $first = Join-Path $fixture 'first.exe'
    $second = Join-Path $fixture 'second.exe'
    $secondPayload = Join-Path $fixture 'second-receiver.exe'
    Copy-Item -LiteralPath $receiverPath -Destination $secondPayload
    [IO.File]::AppendAllText($secondPayload, 'synthetic upgrade marker')
    $firstHash = (Get-FileHash -LiteralPath $receiverPath).Hash
    $secondHash = (Get-FileHash -LiteralPath $secondPayload).Hash
    Build-Installer $receiverPath $first
    Build-Installer $secondPayload $second

    Assert ((Invoke-Installer $first) -eq 0) 'Fresh install failed'
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Fresh install used the wrong Receiver'
    Assert (Test-Path -LiteralPath $shortcut) 'Start menu shortcut is missing'
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name UninstallString) -eq "`"$uninstaller`"") 'Apps & features entry is missing the uninstaller'
    Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name $product) -eq "`"$binary`" --startup") 'Default startup must be tray-only'
    Assert (!(Test-Path -LiteralPath $data)) 'Install created Receiver data before first use'

    New-Item -ItemType Directory $data | Out-Null
    $sentinels = @{}
    foreach ($file in 'settings.json', 'registration.json', 'registration.pending.json',
        'registration.cleanup.json', 'delivery-rejections.log', '.tmpAb3xYz') {
        $path = Join-Path $data $file
        [IO.File]::WriteAllText($path, "synthetic state for $file")
        $sentinels[$file] = (Get-FileHash -LiteralPath $path).Hash
    }
    Remove-ItemProperty -LiteralPath $runKey -Name $product
    if (!(Test-Path -LiteralPath $approvalKey)) {
        New-Item -Path $approvalKey | Out-Null
    }
    $approval = [byte[]] @(3, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8)
    New-ItemProperty -LiteralPath $approvalKey -Name $product -PropertyType Binary -Value $approval | Out-Null

    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $created = $false
    $mutex = [Threading.Mutex]::new($false, "Global\BuddyLabsCo.PinGlide.$sid.Instance", [ref] $created)
    try {
        Assert $created 'Exit the running Pin Glide Receiver before running the packaging test'
        Assert ((Invoke-Installer $second) -ne 0) 'Upgrade ran while the Receiver owned its instance'
        $uninstallerCopy = Join-Path $fixture 'uninstall-copy.exe'
        Copy-Item -LiteralPath $uninstaller -Destination $uninstallerCopy
        Assert ((Invoke-Installer $uninstallerCopy "/S _?=$installation") -ne 0) 'Uninstall ran while the Receiver owned its instance'
    } finally {
        $mutex.Dispose()
    }
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Blocked maintenance replaced the Receiver'
    Assert-Data

    $locked = [IO.File]::Open($binary, 'Open', 'Read', 'Read')
    try {
        Assert ((Invoke-Installer $second) -ne 0) 'Upgrade reported success with a locked Receiver file'
    } finally {
        $locked.Dispose()
    }
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Failed replacement damaged the existing Receiver'
    Assert-Data

    Assert ((Invoke-Installer $second) -eq 0) 'Upgrade failed'
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $secondHash) 'Upgrade did not replace the Receiver'
    Assert-Data
    Assert ($null -eq (Get-Item -LiteralPath $runKey).GetValue($product)) 'Upgrade re-enabled startup'
    Assert ([Convert]::ToBase64String((Get-ItemPropertyValue -LiteralPath $approvalKey -Name $product)) -eq
        [Convert]::ToBase64String($approval)) 'Upgrade changed Windows Startup Apps approval'

    New-ItemProperty -LiteralPath $runKey -Name $product -Value "`"$binary`" --startup" | Out-Null
    Assert ((Invoke-Installer $uninstaller) -eq 0) 'Uninstall failed'
    foreach ($path in $installation, $data, $shortcut, $uninstallKey) {
        Assert (!(Test-Path -LiteralPath $path)) "Uninstall left $path"
    }
    foreach ($key in $runKey, $approvalKey) {
        Assert ($null -eq (Get-Item -LiteralPath $key).GetValue($product)) "Uninstall left the $product value in $key"
    }

    Assert ((Invoke-Installer $second) -eq 0) 'Reinstall failed'
    Assert (!(Test-Path -LiteralPath $data)) 'Reinstall restored old Receiver data'
    Assert ((Invoke-Installer $uninstaller) -eq 0) 'Reinstall cleanup failed'
    Write-Output 'Passed: fresh install, blocked install and uninstall, failed replacement, upgrade, uninstall, and clean reinstall.'
} finally {
    Remove-Item -LiteralPath $runKey, $approvalKey, "HKCU:\$testRegistry", $uninstallKey -ErrorAction SilentlyContinue
    $resolved = (Resolve-Path -LiteralPath $fixture).Path
    if ([IO.Path]::GetDirectoryName($resolved) -ne $target -or [IO.Path]::GetFileName($resolved) -ne $product) {
        throw "Unexpected fixture path: $resolved"
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
