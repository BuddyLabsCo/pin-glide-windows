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
$uninstallerCopy = Join-Path $fixture 'uninstall-copy.exe'
$uninstallCommand = "`"$uninstaller`""

function Assert($condition, [string] $message) {
    if (!$condition) { throw $message }
}

function Assert-RejectedOrigin($origin, [switch] $DevelopmentTransport, [switch] $Unsigned,
    [string] $ExpectedError = 'Set PINGLIDE_SERVICE_URL*') {
    $previous = $env:PINGLIDE_SERVICE_URL
    try {
        $env:PINGLIDE_SERVICE_URL = $origin
        try {
            & (Join-Path $PSScriptRoot 'build.ps1') -Unsigned:$Unsigned -DevelopmentTransport:$DevelopmentTransport
            throw 'Packaging accepted a forbidden Service origin.'
        } catch {
            if ($_.Exception.Message -notlike $ExpectedError) { throw }
        }
    } finally {
        $env:PINGLIDE_SERVICE_URL = $previous
    }
}

function Build-Installer($payload, $output, $version) {
    & makensis /V2 /WX "/DPRODUCT_NAME=$product" "/DUSER_FILES=$fixture" "/DMENU_DIR=$fixture" `
        "/DRUN_KEY=$testRegistry\Run" "/DAPPROVAL_KEY=$testRegistry\StartupApproved" `
        "/DPRODUCT_VERSION=$version" "/DFILE_VERSION=$version.0" `
        "/DRECEIVER=$payload" "/DOUTPUT=$output" (Join-Path $PSScriptRoot 'installer.nsi')
    Assert ($LASTEXITCODE -eq 0) 'MakeNSIS failed'
    $metadata = (Get-Item -LiteralPath $output).VersionInfo
    Assert ($metadata.ProductVersion -eq $version) 'Wrong installer product version'
    Assert ($metadata.FileVersion -eq $version) 'Wrong installer file version'
    Assert ("$($metadata.FileMajorPart).$($metadata.FileMinorPart).$($metadata.FileBuildPart).$($metadata.FilePrivatePart)" -eq "$version.0") 'Wrong fixed file version'
    Assert ("$($metadata.ProductMajorPart).$($metadata.ProductMinorPart).$($metadata.ProductBuildPart).$($metadata.ProductPrivatePart)" -eq "$version.0") 'Wrong fixed product version'
}

function Invoke-Installer($executable, $arguments = '/S') {
    (Start-Process -FilePath $executable -ArgumentList $arguments -Wait -PassThru).ExitCode
}

function Invoke-Uninstaller {
    Copy-Item -LiteralPath $uninstaller -Destination $uninstallerCopy -Force
    Invoke-Installer $uninstallerCopy "/S _?=$installation"
}

function Assert-Registered {
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name UninstallString) -eq $uninstallCommand) 'Apps & features entry is missing the uninstaller'
}

function Assert-Version($version) {
    Assert ((Get-Item -LiteralPath $uninstallKey).GetValue('DisplayVersion') -eq $version) 'Wrong installed version'
}

function Assert-Uninstalled {
    foreach ($path in $installation, $data, $shortcut, $uninstallKey) {
        Assert (!(Test-Path -LiteralPath $path)) "Uninstall left $path"
    }
    foreach ($key in $runKey, $approvalKey) {
        Assert ($null -eq (Get-Item -LiteralPath $key).GetValue($product)) "Uninstall left the $product value in $key"
    }
}

function Assert-Data {
    foreach ($file in $sentinels.Keys) {
        $actual = (Get-FileHash -LiteralPath (Join-Path $data $file)).Hash
        Assert ($actual -eq $sentinels[$file]) "$file changed during upgrade or blocked uninstall"
    }
}

try {
    Assert-RejectedOrigin 'http://localhost'
    Assert-RejectedOrigin 'http://localhost' -Unsigned
    Assert-RejectedOrigin 'http://localhost' -DevelopmentTransport -ExpectedError 'Development transport is only available*'
    foreach ($origin in @('', 'ftp://localhost', 'http://user@localhost',
        'http://localhost/path', 'http://localhost?query', 'http://localhost#fragment',
        'https://localhost/path')) {
        Assert-RejectedOrigin $origin -Unsigned -DevelopmentTransport
    }

    $first = Join-Path $fixture 'first.exe'
    $second = Join-Path $fixture 'second.exe'
    $secondPayload = Join-Path $fixture 'second-receiver.exe'
    Copy-Item -LiteralPath $receiverPath -Destination $secondPayload
    [IO.File]::AppendAllText($secondPayload, 'synthetic upgrade marker')
    $firstHash = (Get-FileHash -LiteralPath $receiverPath).Hash
    $secondHash = (Get-FileHash -LiteralPath $secondPayload).Hash
    Build-Installer $receiverPath $first '1.2.3'
    Build-Installer $secondPayload $second '2.0.0'

    Assert ((Invoke-Installer $first) -eq 0) 'Fresh install failed'
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Fresh install used the wrong Receiver'
    Assert (Test-Path -LiteralPath $shortcut) 'Start menu shortcut is missing'
    Assert-Registered
    Assert-Version '1.2.3'
    Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name $product) -eq "`"$binary`" --startup") 'Default startup must be tray-only'
    Assert (!(Test-Path -LiteralPath $data)) 'Install created Receiver data before first use'

    New-Item -ItemType Directory $data | Out-Null
    $sentinels = @{}
    foreach ($file in 'settings.json', 'registration.json', 'future-receiver-state.bin') {
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
        Assert ((Invoke-Uninstaller) -ne 0) 'Uninstall ran while the Receiver owned its instance'
    } finally {
        $mutex.Dispose()
    }
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Blocked maintenance replaced the Receiver'
    Assert-Version '1.2.3'
    Assert-Data

    $oldRun = '"C:\Previous Receiver\windows-app.exe" --startup'
    New-ItemProperty -LiteralPath $runKey -Name $product -Value $oldRun | Out-Null
    [IO.File]::AppendAllText($uninstaller, 'synthetic previous uninstaller marker')
    $oldUninstallerHash = (Get-FileHash -LiteralPath $uninstaller).Hash

    $runAccess = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey(
        "$testRegistry\Run", 'ReadWriteSubTree', 'ReadPermissions, ChangePermissions')
    try {
        $denyWrites = $runAccess.GetAccessControl()
        $rule = [Security.AccessControl.RegistryAccessRule]::new(
            [Security.Principal.SecurityIdentifier]::new($sid), 'SetValue', 'Deny')
        $denyWrites.AddAccessRule($rule)
        $runAccess.SetAccessControl($denyWrites)
        try {
            $readOnlyRun = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("$testRegistry\Run")
            try {
                Assert ($readOnlyRun.GetValue($product) -eq $oldRun) 'The startup fixture must remain readable'
                Assert ((Invoke-Installer $second) -ne 0) 'Upgrade reported success when startup migration could not write'
            } finally {
                $readOnlyRun.Dispose()
            }
        } finally {
            $denyWrites.RemoveAccessRuleSpecific($rule)
            $runAccess.SetAccessControl($denyWrites)
        }
    } finally {
        $runAccess.Dispose()
    }
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Denied startup migration replaced the Receiver'
    Assert ((Get-FileHash -LiteralPath $uninstaller).Hash -eq $oldUninstallerHash) 'Denied startup migration replaced the uninstaller'
    Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name $product) -eq $oldRun) 'Denied startup migration changed the previous startup command'
    Assert-Version '1.2.3'
    Assert-Data

    $locked = [IO.File]::Open($binary, 'Open', 'Read', 'Read')
    try {
        Assert ((Invoke-Installer $second) -ne 0) 'Upgrade reported success with a locked Receiver file'
    } finally {
        $locked.Dispose()
    }
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Failed replacement damaged the existing Receiver'
    Assert ((Get-FileHash -LiteralPath $uninstaller).Hash -eq $oldUninstallerHash) 'Failed replacement changed the uninstaller'
    Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name $product) -eq $oldRun) 'Failed replacement changed the previous startup command'
    Assert-Version '1.2.3'
    Assert-Data

    Remove-ItemProperty -LiteralPath $uninstallKey -Name DisplayVersion
    $locked = [IO.File]::Open($binary, 'Open', 'Read', 'Read')
    try {
        Assert ((Invoke-Installer $second) -ne 0) 'Upgrade reported success with a locked legacy Receiver'
    } finally {
        $locked.Dispose()
    }
    Assert-Version $null

    Assert ((Invoke-Installer $second) -eq 0) 'Upgrade failed'
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $secondHash) 'Upgrade did not replace the Receiver'
    Assert ((Get-FileHash -LiteralPath $uninstaller).Hash -ne $oldUninstallerHash) 'Upgrade did not replace the uninstaller'
    Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name $product) -eq "`"$binary`" --startup") 'Upgrade did not migrate startup'
    Assert-Version '2.0.0'
    Assert-Data
    Remove-ItemProperty -LiteralPath $runKey -Name $product
    Assert ((Invoke-Installer $second) -eq 0) 'Upgrade with disabled startup failed'
    Assert ($null -eq (Get-Item -LiteralPath $runKey).GetValue($product)) 'Upgrade re-enabled startup'
    Assert ([Convert]::ToBase64String((Get-ItemPropertyValue -LiteralPath $approvalKey -Name $product)) -eq
        [Convert]::ToBase64String($approval)) 'Upgrade changed Windows Startup Apps approval'

    New-ItemProperty -LiteralPath $runKey -Name $product -Value "`"$binary`" --startup" | Out-Null
    foreach ($name in 'receiver.new', 'uninstall.new', 'uninstall.previous') {
        [IO.File]::WriteAllText((Join-Path $installation $name), 'interrupted upgrade')
    }
    $locked = [IO.File]::Open((Join-Path $data 'future-receiver-state.bin'), 'Open', 'Read', 'Read')
    try {
        Assert ((Invoke-Uninstaller) -ne 0) 'Uninstall reported success with locked Receiver data'
        Assert (Test-Path -LiteralPath $uninstallKey) 'Failed data cleanup unregistered the uninstaller'
    } finally {
        $locked.Dispose()
    }
    $registryBlocker = Join-Path $uninstallKey 'BlockDeletion'
    New-Item -Path $registryBlocker | Out-Null
    try {
        Assert ((Invoke-Uninstaller) -ne 0) 'Uninstall reported success with a blocked Apps & features entry'
        Assert (Test-Path -LiteralPath $uninstaller) 'Registry cleanup failure removed the uninstaller'
        Assert-Registered
        Assert-Version '2.0.0'
    } finally {
        Remove-Item -LiteralPath $registryBlocker -ErrorAction SilentlyContinue
    }
    Assert (!(Test-Path -LiteralPath $shortcut)) 'Blocked uninstall did not exercise a missing shortcut'
    Assert ((Invoke-Installer $second) -eq 0) 'Setup could not repair the partial uninstall'
    Assert (Test-Path -LiteralPath $shortcut) 'Setup did not restore the Start menu shortcut'
    Assert-Registered
    Assert-Version '2.0.0'
    Remove-Item -LiteralPath $uninstallKey
    Assert ((Invoke-Installer $second) -eq 0) 'Setup could not repair a missing Apps & features entry'
    Assert-Registered
    Assert-Version '2.0.0'

    $locked = [IO.File]::Open($uninstaller, 'Open', 'Read', 'Read')
    try {
        Assert ((Invoke-Uninstaller) -ne 0) 'Uninstall reported success with a locked uninstaller'
        Assert-Registered
        Assert-Version '2.0.0'
    } finally {
        $locked.Dispose()
    }
    $uninstallerFile = Get-Item -LiteralPath $uninstaller
    $uninstallerFile.IsReadOnly = $true
    try {
        Assert ((Invoke-Uninstaller) -ne 0) 'Uninstall reported success with a read-only uninstaller'
        Assert-Registered
        Assert-Version '2.0.0'
    } finally {
        $uninstallerFile.IsReadOnly = $false
    }
    Assert ((Invoke-Uninstaller) -eq 0) 'Uninstall failed after the uninstaller became available'
    Assert-Uninstalled

    Assert ((Invoke-Installer $second) -eq 0) 'Reinstall failed'
    Assert (!(Test-Path -LiteralPath $data)) 'Reinstall restored old Receiver data'
    New-ItemProperty -LiteralPath $approvalKey -Name $product -PropertyType Binary -Value $approval | Out-Null
    Remove-Item -LiteralPath $uninstallKey
    Assert ((Invoke-Uninstaller) -eq 0) 'Uninstall failed with an already absent Apps & features entry'
    Assert-Uninstalled

    Assert ((Invoke-Installer $second) -eq 0) 'Install for the registered uninstall command failed'
    $registered = (Get-ItemPropertyValue -LiteralPath $uninstallKey -Name UninstallString).Trim('"')
    Assert ((Invoke-Installer $registered) -eq 0) 'The registered uninstall command failed to launch'
    Assert-Uninstalled
    Write-Output 'Passed: origin validation, version metadata, install, rollback, upgrade, partial repair, uninstall retry, and clean reinstall.'
} finally {
    Remove-Item -LiteralPath $runKey, $approvalKey, "HKCU:\$testRegistry", $uninstallKey -ErrorAction SilentlyContinue
    $resolved = (Resolve-Path -LiteralPath $fixture).Path
    if ([IO.Path]::GetDirectoryName($resolved) -ne $target -or [IO.Path]::GetFileName($resolved) -ne $product) {
        throw "Unexpected fixture path: $resolved"
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
