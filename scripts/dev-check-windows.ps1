[CmdletBinding()]
param(
    [ValidateSet('gnu', 'msvc')]
    [string]$Toolchain = 'gnu',
    [switch]$IncludeFmt,
    [switch]$IncludeClippy,
    [switch]$SkipTest
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common-win-gnu.ps1')

function Require-Command {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$InstallHint
    )

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $command) {
        Write-Error "$Name was not found. $InstallHint"
    }

    Write-Host "OK: $Name -> $($command.Source)"
}

function Require-RustComponent {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    $installed = rustup component list --installed
    $isInstalled = $installed | Where-Object {
        $_ -eq $Name -or $_.StartsWith("$Name-")
    }
    if (-not $isInstalled) {
        Write-Error "$Name component was not found. Run: rustup component add $Name"
    }

    Write-Host "OK: rustup component $Name"
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [string[]]$ArgumentList
    )

    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        $commandLine = @($FilePath) + $ArgumentList
        Write-Error "Command failed with exit code ${LASTEXITCODE}: $($commandLine -join ' ')"
    }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$rustDir = Join-Path $repoRoot 'rust'

if (-not (Test-Path (Join-Path $rustDir 'Cargo.toml'))) {
    Write-Error 'rust/Cargo.toml was not found. Run this script from the repository checkout.'
}

Write-Host '==> Checking Windows development tools'
Require-Command cargo 'Install Rust with rustup and add %USERPROFILE%\.cargo\bin to PATH.'
Require-Command rustc 'Install Rust with rustup and add %USERPROFILE%\.cargo\bin to PATH.'
Require-Command rustup 'Install rustup from https://rustup.rs/.'

$cargoTargetArgs = @()
if ($Toolchain -eq 'gnu') {
    $targetTriple = 'x86_64-pc-windows-gnu'
    [void](Initialize-FlistWalkerWindowsGnu -Mode 'NoInstall')
    $cargoTargetArgs = @('--target', $targetTriple)
} else {
    $targetTriple = 'host-msvc'
    Write-Warning 'Using MSVC fallback. Prefer -Toolchain gnu for development that matches the distributed Windows build.'
}

if ($IncludeFmt) {
    Require-RustComponent rustfmt
}
if ($IncludeClippy) {
    Require-RustComponent clippy
}

Write-Host '==> Toolchain'
Invoke-Checked rustc @('-Vv')
Invoke-Checked cargo @('-V')
Invoke-Checked rustup @('show', 'active-toolchain')
Write-Host "Selected check target: $targetTriple"

Push-Location $rustDir
try {
    if ($IncludeFmt) {
        Write-Host '==> Formatting check'
        Invoke-Checked cargo @('fmt', '--all', '--', '--check')
    }

    if ($IncludeClippy) {
        Write-Host '==> Clippy'
        Invoke-Checked cargo (@('clippy') + $cargoTargetArgs + @('--all-targets', '--', '-D', 'warnings'))
    }

    if (-not $SkipTest) {
        Write-Host '==> Tests'
        Invoke-Checked cargo (@('test') + $cargoTargetArgs)
    }
} finally {
    Pop-Location
}

Write-Host '==> Windows development check passed'
