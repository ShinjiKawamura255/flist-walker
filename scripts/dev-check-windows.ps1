[CmdletBinding()]
param(
    [ValidateSet('gnu', 'msvc')]
    [string]$Toolchain = 'gnu',
    [switch]$IncludeFmt,
    [switch]$IncludeClippy,
    [switch]$SkipTest
)

$ErrorActionPreference = 'Stop'

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

function Require-RustTarget {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Target
    )

    $installed = rustup target list --installed
    if ($installed -notcontains $Target) {
        Write-Error "$Target target was not found. Run: rustup target add $Target"
    }

    Write-Host "OK: rustup target $Target"
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

function Resolve-GnuTool {
    param(
        [Parameter(Mandatory = $true)]
        [string]$EnvName,

        [Parameter(Mandatory = $true)]
        [string[]]$Names
    )

    $fromEnv = [Environment]::GetEnvironmentVariable($EnvName)
    if ($fromEnv -and (Test-Path $fromEnv)) {
        Write-Host "OK: $EnvName -> $fromEnv"
        return $fromEnv
    }

    foreach ($name in $Names) {
        $command = Get-Command $name -ErrorAction SilentlyContinue
        if ($command) {
            Write-Host "OK: $name -> $($command.Source)"
            return $command.Source
        }
    }

    Write-Error "Missing GNU tool for $EnvName. Install MSYS2 mingw-w64 toolchain and add C:\msys64\mingw64\bin to PATH. Tried: $($Names -join ', ')"
}

function Add-Msys2PathIfPresent {
    $candidates = @(
        'C:\msys64\mingw64\bin',
        'C:\msys64\usr\bin'
    )

    foreach ($path in $candidates) {
        if ((Test-Path $path) -and ($env:PATH -notlike "*$path*")) {
            $env:PATH = "$path;$env:PATH"
            Write-Host "Added MSYS2 path for this check: $path"
        }
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
    Add-Msys2PathIfPresent
    Require-RustTarget $targetTriple
    $linker = Resolve-GnuTool 'FLISTWALKER_WINDOWS_LINKER' @('x86_64-w64-mingw32-gcc', 'gcc')
    $cxx = Resolve-GnuTool 'FLISTWALKER_WINDOWS_CXX' @('x86_64-w64-mingw32-g++', 'g++')
    $ar = Resolve-GnuTool 'FLISTWALKER_WINDOWS_AR' @('x86_64-w64-mingw32-ar', 'ar')
    $ranlib = Resolve-GnuTool 'FLISTWALKER_WINDOWS_RANLIB' @('x86_64-w64-mingw32-ranlib', 'ranlib')
    $windres = Resolve-GnuTool 'FLISTWALKER_WINDOWS_WINDRES' @('x86_64-w64-mingw32-windres', 'windres')
    $env:FLISTWALKER_WINDOWS_LINKER = $linker
    $env:FLISTWALKER_WINDOWS_CXX = $cxx
    $env:FLISTWALKER_WINDOWS_AR = $ar
    $env:FLISTWALKER_WINDOWS_RANLIB = $ranlib
    $env:FLISTWALKER_WINDOWS_WINDRES = $windres
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = $linker
    $env:CC_x86_64_pc_windows_gnu = $linker
    $env:CXX_x86_64_pc_windows_gnu = $cxx
    $env:AR_x86_64_pc_windows_gnu = $ar
    $env:RANLIB_x86_64_pc_windows_gnu = $ranlib
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
