[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Assert-Equal {
    param(
        [Parameter(Mandatory = $true)]$Actual,
        [Parameter(Mandatory = $true)]$Expected,
        [Parameter(Mandatory = $true)][string]$Message
    )

    if ($Actual -ne $Expected) {
        throw "$Message Expected '$Expected', got '$Actual'."
    }
}

function Assert-Throws {
    param(
        [Parameter(Mandatory = $true)][scriptblock]$Action,
        [Parameter(Mandatory = $true)][string]$Message
    )

    try {
        & $Action
    } catch {
        return
    }
    throw $Message
}

$scripts = @(
    (Join-Path $PSScriptRoot 'common-win-gnu.ps1'),
    (Join-Path $PSScriptRoot 'build-rust-win.ps1'),
    (Join-Path $PSScriptRoot 'build-rust-win-clean.ps1'),
    (Join-Path $PSScriptRoot 'dev-check-windows.ps1')
    (Join-Path $PSScriptRoot 'test-windows-build-artifact.ps1')
)

foreach ($script in $scripts) {
    $tokens = $null
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile(
        $script,
        [ref]$tokens,
        [ref]$errors
    )
    if ($errors.Count -gt 0) {
        throw "PowerShell parser errors in ${script}: $($errors -join '; ')"
    }
}

. (Join-Path $PSScriptRoot 'common-win-gnu.ps1')

Assert-Equal (Get-FlistWalkerInstallMode -CheckOnly -NoInstall) 'CheckOnly' 'TC-145 check-only mode failed.'
Assert-Equal (Get-FlistWalkerInstallMode -NoInstall) 'NoInstall' 'TC-145 no-install mode failed.'
Assert-Equal (Get-FlistWalkerInstallMode -InstallMissing) 'InstallMissing' 'TC-146 install approval mode failed.'
Assert-Throws {
    Get-FlistWalkerInstallMode -NoInstall -InstallMissing
} 'TC-145 conflicting install modes must fail.'

$noInstallPlan = [pscustomobject]@{
    Name = 'Fixture dependency'
    FilePath = 'fixture-missing-installer.exe'
    ArgumentList = @()
    ManualCommand = 'install fixture manually'
    Description = 'fixture dependency'
}
Assert-Throws {
    Install-FlistWalkerDependency -Plan $noInstallPlan -Mode 'NoInstall'
} 'TC-145 no-install mode must reject installation.'

function Test-FlistWalkerCanPrompt { return $false }
Assert-Throws {
    Install-FlistWalkerDependency -Plan $noInstallPlan -Mode 'Interactive'
} 'TC-145 noninteractive mode must reject installation.'
Assert-Throws {
    Install-FlistWalkerDependency -Plan $noInstallPlan -Mode 'InstallMissing'
} 'TC-146 missing installer must fail before build.'

$failurePlan = [pscustomobject]@{
    Name = 'Fixture failing installer'
    FilePath = 'cmd.exe'
    ArgumentList = @('/d', '/c', 'exit 7')
    ManualCommand = 'fixture'
    Description = 'fixture failing installer'
}
Assert-Throws {
    Install-FlistWalkerDependency -Plan $failurePlan -Mode 'InstallMissing'
} 'TC-146 installer failure must stop the build.'

$lockedArtifact = Join-Path ([System.IO.Path]::GetTempPath()) "flistwalker-locked-$([Guid]::NewGuid().ToString('N')).exe"
$lockStream = $null
try {
    [System.IO.File]::WriteAllText($lockedArtifact, 'stale')
    $lockStream = [System.IO.File]::Open(
        $lockedArtifact,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::None
    )
    Assert-Throws {
        Remove-FlistWalkerBuildArtifact -Path $lockedArtifact
    } 'TC-147 locked stale artifact must stop the build.'
} finally {
    if ($lockStream) {
        $lockStream.Dispose()
    }
    Remove-Item -LiteralPath $lockedArtifact -Force -ErrorAction SilentlyContinue
}

$fakeRustup = Join-Path ([System.IO.Path]::GetTempPath()) "flistwalker-rustup-$([Guid]::NewGuid().ToString('N')).cmd"
try {
    [System.IO.File]::WriteAllText($fakeRustup, "@echo off`r`nexit /b 0`r`n")
    Assert-Throws {
        Install-FlistWalkerRustTarget `
            -RustupPath $fakeRustup `
            -Target 'x86_64-pc-windows-gnu' `
            -Mode 'InstallMissing'
    } 'TC-146 target re-detection failure must stop the build.'
} finally {
    Remove-Item -LiteralPath $fakeRustup -ErrorAction SilentlyContinue
}

$rustupPlan = Get-FlistWalkerDependencyInstallPlan -Dependency Rustup
Assert-Equal $rustupPlan.FilePath 'winget.exe' 'TC-146 Rustup bootstrap executable changed.'
Assert-Equal ($rustupPlan.ArgumentList -join ' ') 'install --exact --id Rustlang.Rustup --source winget --accept-package-agreements --accept-source-agreements' 'TC-146 Rustup bootstrap arguments changed.'

$msys2Plan = Get-FlistWalkerDependencyInstallPlan -Dependency MSYS2
Assert-Equal ($msys2Plan.ArgumentList -join ' ') 'install --exact --id MSYS2.MSYS2 --source winget --accept-package-agreements --accept-source-agreements' 'TC-146 MSYS2 bootstrap arguments changed.'

$gnuPlan = Get-FlistWalkerDependencyInstallPlan -Dependency GnuTools -PacmanPath 'C:\msys64\usr\bin\pacman.exe'
Assert-Equal $gnuPlan.FilePath 'C:\msys64\usr\bin\pacman.exe' 'TC-146 pacman path changed.'
Assert-Equal ($gnuPlan.ArgumentList -join ' ') '-S --needed --noconfirm mingw-w64-x86_64-gcc' 'TC-146 pacman arguments changed.'

$fixture = Join-Path ([System.IO.Path]::GetTempPath()) "flistwalker-tool-$([Guid]::NewGuid().ToString('N')).exe"
try {
    [System.IO.File]::WriteAllText($fixture, '')
    $old = $env:FLISTWALKER_WINDOWS_LINKER
    $env:FLISTWALKER_WINDOWS_LINKER = $fixture
    $resolved = Resolve-FlistWalkerGnuTool `
        -EnvName 'FLISTWALKER_WINDOWS_LINKER' `
        -ToolNames @('gcc.exe') `
        -MingwBin 'Z:\missing'
    Assert-Equal $resolved $fixture 'TC-145 environment override precedence failed.'
} finally {
    $env:FLISTWALKER_WINDOWS_LINKER = $old
    Remove-Item -LiteralPath $fixture -ErrorAction SilentlyContinue
}

Write-Host 'PowerShell Windows build regression tests passed.'
