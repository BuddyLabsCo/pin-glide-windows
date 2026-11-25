param(
    [Parameter(Mandatory)]
    [string] $Receiver,
    [string] $Installer = (Join-Path $PSScriptRoot '../../target/packaging/windows/Pin-Glide-Setup-x64-unsigned.exe')
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'resource-check.ps1')
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
    [string] $ExpectedError = 'Set PINGLIDE_SERVICE_URL',
    [string] $ExpectedDiagnostic) {
    $previous = $env:PINGLIDE_SERVICE_URL
    $previousErrorPreference = $ErrorActionPreference
    try {
        $env:PINGLIDE_SERVICE_URL = $origin
        $arguments = @('-NoProfile', '-File', (Join-Path $PSScriptRoot 'build.ps1'))
        if ($Unsigned) { $arguments += '-Unsigned' }
        if ($DevelopmentTransport) { $arguments += '-DevelopmentTransport' }
        $ErrorActionPreference = 'Continue'
        $output = & powershell.exe @arguments 2>&1 | Out-String
        $exitCode = $LASTEXITCODE
        $ErrorActionPreference = $previousErrorPreference
        Assert ($exitCode -ne 0) 'Packaging accepted a forbidden Service origin.'
        Assert ($output.Contains($ExpectedError)) "Unexpected packaging failure: $output"
        if ($ExpectedDiagnostic) {
            Assert ($output -match "(?m)^\s*$([regex]::Escape($ExpectedDiagnostic))\s*$") "Missing origin-validation diagnostic: $output"
        }
    } finally {
        $ErrorActionPreference = $previousErrorPreference
        $env:PINGLIDE_SERVICE_URL = $previous
    }
}

function Build-Installer($payload, $output, $version, $language = 1033) {
    & makensis /V2 /WX "/DPRODUCT_NAME=$product" "/DUSER_FILES=$fixture" "/DMENU_DIR=$fixture" `
        "/DRUN_KEY=$testRegistry\Run" "/DAPPROVAL_KEY=$testRegistry\StartupApproved" `
        "/DPRODUCT_VERSION=$version" "/DFILE_VERSION=$version.0" `
        "/DTEST_UI_LANGUAGE=$language" `
        "/DRECEIVER=$payload" "/DOUTPUT=$output" (Join-Path $PSScriptRoot 'installer.nsi')
    Assert ($LASTEXITCODE -eq 0) 'MakeNSIS failed'
    Assert-InstallerVersion $output $version
}

function Assert-InstallerVersion($output, $version) {
    $metadata = (Get-Item -LiteralPath $output).VersionInfo
    $numeric = [version]($version -split '[-+]', 2)[0]
    $fixed = "$($numeric.Major).$($numeric.Minor).$($numeric.Build).0"
    Assert ($metadata.ProductVersion -eq $version) 'Wrong installer product version'
    Assert ($metadata.FileVersion -eq $version) 'Wrong installer file version'
    Assert ("$($metadata.FileMajorPart).$($metadata.FileMinorPart).$($metadata.FileBuildPart).$($metadata.FilePrivatePart)" -eq $fixed) 'Wrong fixed file version'
    Assert ("$($metadata.ProductMajorPart).$($metadata.ProductMinorPart).$($metadata.ProductBuildPart).$($metadata.ProductPrivatePart)" -eq $fixed) 'Wrong fixed product version'
}

function Invoke-Installer($executable, $arguments = '/S') {
    (Start-Process -FilePath $executable -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru).ExitCode
}

function Invoke-Uninstaller {
    Clear-UninstallMarkers
    Copy-Item -LiteralPath $uninstaller -Destination $uninstallerCopy -Force
    $exitCode = Invoke-Installer $uninstallerCopy "/S _?=$installation"
    Assert (Test-Path -LiteralPath (Join-Path $fixture 'language.txt')) 'Uninstaller did not write a fresh language selection'
    $exitCode
}

function Clear-UninstallMarkers {
    foreach ($name in 'language.txt', 'failure.txt') {
        $marker = Join-Path $fixture $name
        if (Test-Path -LiteralPath $marker) {
            Remove-Item -LiteralPath $marker
        }
    }
}

function Assert-Registered {
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name UninstallString) -eq $uninstallCommand) 'Apps & features entry is missing the uninstaller'
}

function Assert-LocalizedMetadata($language) {
    $expected = if ($language -eq 2052) { [string][char]0x9e4a + [char]0x4fe1 } else { 'Pin Glide' }
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name DisplayName) -ceq $expected) 'Wrong localized fallback display name'
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name DisplayName_Localized) -eq "@$binary,-101") 'Wrong indirect uninstall name'
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name DisplayIcon) -eq "$binary,0") 'Wrong uninstall icon'
    Assert ([BrandResources]::ShortcutResource($shortcut) -eq "$binary,101") 'Wrong localized shortcut name'
    Assert ([BrandResources]::ShortcutIdentity($shortcut) -eq 'BuddyLabsCo.PinGlide') 'Start shortcut and taskbar identity differ'
    Assert (@(Get-ChildItem -LiteralPath $fixture -Filter '*.lnk').Count -eq 1) 'Install created duplicate Start shortcuts'
    Assert ((Get-Content -LiteralPath (Join-Path $fixture 'language.txt') -Raw) -eq [string]$language) 'Wrong selected language'
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($shortcut)
    Assert ($link.TargetPath -eq $binary -and !$link.Arguments) 'Start shortcut does not launch the installed Receiver'
    Assert ($link.IconLocation -eq "$binary,0") 'Start shortcut does not use the Receiver icon'
    Assert-BrandResources $uninstaller Uninstaller
    Assert-InstallerVersion $uninstaller (Get-ItemPropertyValue -LiteralPath $uninstallKey -Name DisplayVersion)
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
    $ini = Join-Path $fixture 'desktop.ini'
    if (Test-Path -LiteralPath $ini) {
        Assert (!(Select-String -LiteralPath $ini -SimpleMatch "$product.lnk=")) 'Uninstall left the localized Shell registration'
    }
}

function Assert-Data {
    foreach ($file in $sentinels.Keys) {
        $actual = (Get-FileHash -LiteralPath (Join-Path $data $file)).Hash
        Assert ($actual -eq $sentinels[$file]) "$file changed during upgrade or blocked uninstall"
    }
}

function Assert-LegacyMetadata {
    Assert ((Get-Item -LiteralPath $uninstallKey).GetValue('DisplayName') -eq 'Pin Glide') 'Failed upgrade changed the old fallback name'
    Assert ($null -eq (Get-Item -LiteralPath $uninstallKey).GetValue('DisplayName_Localized')) 'Failed upgrade left a localized name'
    Assert ($null -eq (Get-Item -LiteralPath $uninstallKey).GetValue('DisplayIcon')) 'Failed upgrade left an icon reference'
    Assert ($null -eq [BrandResources]::ShortcutResource($shortcut)) 'Failed upgrade left localized shortcut metadata'
    Assert ((Get-FileHash -LiteralPath $shortcut).Hash -eq $legacyShortcutHash) 'Failed upgrade modified the existing shortcut'
}

function Get-UninstallMetadata {
    $key = Get-Item -LiteralPath $uninstallKey
    try {
        foreach ($name in ($key.GetValueNames() | Sort-Object)) {
            [pscustomobject]@{ Name = $name; Kind = [string]$key.GetValueKind($name); Value = $key.GetValue($name) }
        }
    } finally {
        $key.Dispose()
    }
}

try {
    $metadata = & cargo metadata --locked --no-deps --format-version 1 --manifest-path (Join-Path $PSScriptRoot '../../Cargo.toml')
    Assert ($LASTEXITCODE -eq 0) 'Cargo metadata failed'
    $cargoVersion = ($metadata | ConvertFrom-Json).packages | Where-Object { $_.name -eq 'windows-app' } | Select-Object -ExpandProperty version
    Assert-InstallerVersion $Installer $cargoVersion
    Assert-InstallerVersion $receiverPath $cargoVersion
    Assert-BrandResources $receiverPath Receiver
    Assert-BrandResources $Installer Setup
    $canonicalInstaller = Join-Path $target 'packaging/windows/Pin-Glide-Setup-x64-unsigned.exe'
    $artifactHash = if (Test-Path -LiteralPath $canonicalInstaller) {
        (Get-FileHash -LiteralPath $canonicalInstaller).Hash
    }
    Assert-RejectedOrigin 'http://localhost'
    Assert-RejectedOrigin 'http://localhost' -Unsigned
    Assert-RejectedOrigin 'http://localhost' -DevelopmentTransport -ExpectedError 'Development transport is only available'
    foreach ($origin in @('', 'ftp://localhost', 'http://user@localhost',
        'http://localhost/path', 'http://localhost?query', 'http://localhost#fragment',
        'https://localhost/path')) {
        Assert-RejectedOrigin $origin -Unsigned -DevelopmentTransport
    }
    Assert-RejectedOrigin 'http://256.256.256.256' -Unsigned -DevelopmentTransport `
        -ExpectedError 'Cargo release build failed.' `
        -ExpectedDiagnostic 'Set PINGLIDE_SERVICE_URL to a valid Service origin for this transport mode.'
    $currentHash = if (Test-Path -LiteralPath $canonicalInstaller) {
        (Get-FileHash -LiteralPath $canonicalInstaller).Hash
    }
    Assert ($currentHash -eq $artifactHash) 'Rejected origin changed the canonical installer'

    $first = Join-Path $fixture 'first.exe'
    $second = Join-Path $fixture 'second.exe'
    $secondPayload = Join-Path $fixture 'second-receiver.exe'
    Copy-Item -LiteralPath $receiverPath -Destination $secondPayload
    [IO.File]::AppendAllText($secondPayload, 'synthetic upgrade marker')
    $firstHash = (Get-FileHash -LiteralPath $receiverPath).Hash
    $secondHash = (Get-FileHash -LiteralPath $secondPayload).Hash
    Build-Installer $receiverPath $first '1.2.3'
    Build-Installer $secondPayload $second '2.0.0' 1028

    Assert ((Invoke-Installer $first) -eq 0) 'Fresh install failed'
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Fresh install used the wrong Receiver'
    Assert (Test-Path -LiteralPath $shortcut) 'Start menu shortcut is missing'
    Assert-Registered
    Assert-LocalizedMetadata 1033
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
    # Simulate the previous release's plain metadata and unlocalized existing shortcut.
    Remove-ItemProperty -LiteralPath $uninstallKey -Name DisplayName_Localized, DisplayIcon
    Assert ([BrandResources]::SHRemoveLocalizedName($shortcut) -eq 0) 'Could not seed legacy shortcut'
    Remove-Item -LiteralPath $shortcut
    $shell = New-Object -ComObject WScript.Shell
    $oldLink = $shell.CreateShortcut($shortcut)
    $oldLink.TargetPath = 'C:\Previous Receiver\windows-app.exe'
    $oldLink.Save()
    $legacyShortcutHash = (Get-FileHash -LiteralPath $shortcut).Hash
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
        $failure = Join-Path $fixture 'failure.txt'
        $chineseName = [string][char]0x9e4a + [char]0x4fe1
        Assert ((Get-Content -LiteralPath $failure -Encoding Unicode -Raw).Contains($chineseName)) 'Setup did not select its Chinese custom error'
        Assert ((Invoke-Uninstaller) -ne 0) 'Uninstall ran while the Receiver owned its instance'
        Assert ((Get-Content -LiteralPath $failure -Encoding Unicode -Raw) -eq 'Exit Pin Glide from its tray menu, then try again.') 'Uninstall did not select its English custom error'
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

    foreach ($value in 'DisplayVersion', 'UninstallString', 'DisplayName', 'DisplayName_Localized', 'DisplayIcon') {
        $previous = (Get-Item -LiteralPath $uninstallKey).GetValue($value)
        New-ItemProperty -LiteralPath $uninstallKey -Name $value -PropertyType DWord -Value 123 -Force | Out-Null
        Assert ((Invoke-Installer $second) -ne 0) "Upgrade accepted unreadable $value"
        Assert ((Get-Item -LiteralPath $uninstallKey).GetValueKind($value) -eq 'DWord') "Failed upgrade changed $value type"
        Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name $value) -eq 123) "Failed upgrade changed $value"
        Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Unreadable metadata replaced the Receiver'
        Assert ((Get-FileHash -LiteralPath $uninstaller).Hash -eq $oldUninstallerHash) 'Unreadable metadata replaced the uninstaller'
        Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name $product) -eq $oldRun) 'Unreadable metadata changed startup'
        Assert-Data
        if ($null -eq $previous) {
            Remove-ItemProperty -LiteralPath $uninstallKey -Name $value
        } else {
            New-ItemProperty -LiteralPath $uninstallKey -Name $value -PropertyType String -Value $previous -Force | Out-Null
        }
    }

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
    Assert-LegacyMetadata

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
    Assert-LegacyMetadata

    Remove-ItemProperty -LiteralPath $uninstallKey -Name DisplayVersion
    $locked = [IO.File]::Open($binary, 'Open', 'Read', 'Read')
    try {
        Assert ((Invoke-Installer $second) -ne 0) 'Upgrade reported success with a locked legacy Receiver'
    } finally {
        $locked.Dispose()
    }
    Assert-Version $null
    Assert-LegacyMetadata

    New-ItemProperty -LiteralPath $uninstallKey -Name DisplayVersion -Value '1.2.3' | Out-Null
    New-ItemProperty -LiteralPath $uninstallKey -Name ExtraMetadata -Value 'keep me' | Out-Null
    Set-ItemProperty -LiteralPath $uninstallKey -Name Publisher -Value 'Previous publisher'
    Set-ItemProperty -LiteralPath $uninstallKey -Name NoModify -Value 0
    Remove-ItemProperty -LiteralPath $uninstallKey -Name NoRepair
    foreach ($command in @($null, '')) {
        if ($null -eq $command) {
            Remove-ItemProperty -LiteralPath $uninstallKey -Name UninstallString
        } else {
            New-ItemProperty -LiteralPath $uninstallKey -Name UninstallString -Value $command -Force | Out-Null
        }
        $previousMetadata = Get-UninstallMetadata | ConvertTo-Json -Compress
        $locked = [IO.File]::Open($binary, 'Open', 'Read', 'Read')
        try {
            Assert ((Invoke-Installer $second) -ne 0) 'Upgrade succeeded with incomplete metadata and a locked Receiver'
        } finally {
            $locked.Dispose()
        }
        Assert ((Get-UninstallMetadata | ConvertTo-Json -Compress) -ceq $previousMetadata) 'Failed upgrade did not restore incomplete uninstall metadata exactly'
        Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $firstHash) 'Failed upgrade replaced the Receiver'
        Assert ((Get-FileHash -LiteralPath $uninstaller).Hash -eq $oldUninstallerHash) 'Failed upgrade replaced the uninstaller'
        Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name $product) -eq $oldRun) 'Failed upgrade changed startup'
        Assert-LegacyMetadata
        Assert-Data
    }

    Assert ((Invoke-Installer $second) -eq 0) 'Upgrade failed'
    Assert ((Get-FileHash -LiteralPath $binary).Hash -eq $secondHash) 'Upgrade did not replace the Receiver'
    Assert ((Get-FileHash -LiteralPath $uninstaller).Hash -ne $oldUninstallerHash) 'Upgrade did not replace the uninstaller'
    Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name $product) -eq "`"$binary`" --startup") 'Upgrade did not migrate startup'
    Assert-Version '2.0.0'
    Assert-Registered
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name Publisher) -eq 'BuddyLabsCo') 'Upgrade did not repair incomplete publisher metadata'
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name NoModify) -eq 1) 'Upgrade did not repair incomplete NoModify metadata'
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name NoRepair) -eq 1) 'Upgrade did not repair incomplete NoRepair metadata'
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name ExtraMetadata) -eq 'keep me') 'Upgrade lost unrelated metadata'
    Assert-LocalizedMetadata 2052
    Assert-Data
    $previousIndirect = "@$receiverPath,-101"
    $previousIcon = "$receiverPath,0"
    $previousShortcutResource = "$receiverPath,101"
    Set-ItemProperty -LiteralPath $uninstallKey -Name DisplayName_Localized -Value $previousIndirect
    Set-ItemProperty -LiteralPath $uninstallKey -Name DisplayIcon -Value $previousIcon
    Assert ([BrandResources]::SHSetLocalizedName($shortcut, $receiverPath, 101) -eq 0) 'Could not seed a distinct previous localized shortcut'
    $previousShortcutHash = (Get-FileHash -LiteralPath $shortcut).Hash
    $locked = [IO.File]::Open($binary, 'Open', 'Read', 'Read')
    try {
        Assert ((Invoke-Installer $first) -ne 0) 'Downgrade reported success with a locked Receiver'
    } finally {
        $locked.Dispose()
    }
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name DisplayName) -ceq ([string][char]0x9e4a + [char]0x4fe1)) 'Failed replacement changed localized fallback name'
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name DisplayName_Localized) -eq $previousIndirect) 'Failed replacement changed localized reference'
    Assert ((Get-ItemPropertyValue -LiteralPath $uninstallKey -Name DisplayIcon) -eq $previousIcon) 'Failed replacement changed icon reference'
    Assert ([BrandResources]::ShortcutResource($shortcut) -eq $previousShortcutResource) 'Failed replacement changed localized shortcut'
    Assert ((Get-FileHash -LiteralPath $shortcut).Hash -eq $previousShortcutHash) 'Failed replacement changed shortcut bytes'
    Assert-Version '2.0.0'
    Assert-Data
    Remove-ItemProperty -LiteralPath $runKey -Name $product
    Assert ((Invoke-Installer $second) -eq 0) 'Upgrade with disabled startup failed'
    Assert ($null -eq (Get-Item -LiteralPath $runKey).GetValue($product)) 'Upgrade re-enabled startup'
    Assert ([Convert]::ToBase64String((Get-ItemPropertyValue -LiteralPath $approvalKey -Name $product)) -eq
        [Convert]::ToBase64String($approval)) 'Upgrade changed Windows Startup Apps approval'
    Assert-LocalizedMetadata 2052

    $desktopIni = Join-Path $fixture 'desktop.ini'
    $locked = [IO.File]::Open($desktopIni, 'Open', 'Read', 'Read')
    try {
        Assert ((Invoke-Uninstaller) -ne 0) 'Uninstall succeeded while localized shortcut cleanup was blocked'
        Assert (Test-Path -LiteralPath $shortcut) 'Blocked localized cleanup removed the shortcut'
        Assert (Test-Path -LiteralPath $uninstaller) 'Blocked localized cleanup removed the uninstaller'
        Assert-Registered
        Assert-Data
    } finally {
        $locked.Dispose()
    }
    Assert ([BrandResources]::ShortcutResource($shortcut) -eq "$binary,101") 'Blocked localized cleanup lost the original resource'

    New-ItemProperty -LiteralPath $runKey -Name $product -Value "`"$binary`" --startup" | Out-Null
    foreach ($name in 'receiver.new', 'uninstall.new', 'uninstall.previous', 'shortcut.previous') {
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
    Assert ($null -eq [BrandResources]::ShortcutResource($shortcut)) 'Uninstall left the localized shortcut registration'
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
    Assert ((Get-Content -LiteralPath (Join-Path $fixture 'language.txt') -Raw) -eq '2052') 'Uninstaller selected the wrong language'
    Assert-Uninstalled
    foreach ($case in @(@(4, 2052), @(2052, 2052), @(3076, 2052), @(4100, 2052), @(5124, 2052), @(2057, 1033), @(1041, 1033))) {
        Build-Installer $receiverPath $first '1.2.3' $case[0]
        Assert ((Invoke-Installer $first) -eq 0) "Install failed for UI language $($case[0])"
        Assert-LocalizedMetadata $case[1]
        Assert ((Invoke-Uninstaller) -eq 0) "Uninstall failed for UI language $($case[0])"
        Assert ((Get-Content -LiteralPath (Join-Path $fixture 'language.txt') -Raw) -eq [string]$case[1]) 'Uninstaller selected the wrong language'
        Assert-Uninstalled
    }

    Assert ((Invoke-Installer $second) -eq 0) 'Reinstall failed'
    Assert (!(Test-Path -LiteralPath $data)) 'Reinstall restored old Receiver data'
    Assert ([BrandResources]::SHRemoveLocalizedName($shortcut) -eq 0) 'Could not seed a shortcut without a localized name'
    New-ItemProperty -LiteralPath $approvalKey -Name $product -PropertyType Binary -Value $approval | Out-Null
    Remove-Item -LiteralPath $uninstallKey
    Assert ((Invoke-Uninstaller) -eq 0) 'Uninstall failed with an already absent Apps & features entry'
    Assert-Uninstalled

    Assert ((Invoke-Installer $second) -eq 0) 'Install for the registered uninstall command failed'
    $registered = (Get-ItemPropertyValue -LiteralPath $uninstallKey -Name UninstallString).Trim('"')
    Clear-UninstallMarkers
    Assert ((Invoke-Installer $registered) -eq 0) 'The registered uninstall command failed to launch'
    Assert ((Get-Content -LiteralPath (Join-Path $fixture 'language.txt') -Raw) -eq '2052') 'Registered uninstaller did not select its language'
    Assert-Uninstalled
    Write-Output 'Passed: origin validation, actual PE brand resources, language selection, localized metadata, install, rollback, upgrade, partial repair, uninstall retry, and clean reinstall.'
} finally {
    Remove-Item -LiteralPath $runKey, $approvalKey, "HKCU:\$testRegistry", $uninstallKey -ErrorAction SilentlyContinue
    $resolved = (Resolve-Path -LiteralPath $fixture).Path
    if ([IO.Path]::GetDirectoryName($resolved) -ne $target -or [IO.Path]::GetFileName($resolved) -ne $product) {
        throw "Unexpected fixture path: $resolved"
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
