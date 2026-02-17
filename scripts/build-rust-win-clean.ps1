[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoDir = Split-Path -Parent $ScriptDir
$RustDir = Join-Path $RepoDir 'rust'
$Target = 'x86_64-pc-windows-msvc'
$BuiltExePath = Join-Path $RustDir "target\$Target\release\flistwalker.exe"
$ExePath = Join-Path $RustDir "target\$Target\release\FlistWalker.exe"
$CargoBin = Join-Path $env:USERPROFILE '.cargo\bin'

if (Test-Path -LiteralPath $CargoBin) {
    $env:PATH = "$CargoBin;$env:PATH"
}

if (-not (Test-Path -LiteralPath $RustDir)) {
    Write-Error "rust directory not found: $RustDir"
    exit 1
}

try {
    & rustup --version *> $null
    & cargo --version *> $null
}
catch {
    Write-Error "rustup/cargo not found. Install Rust with rustup on Windows, or run scripts/build-rust-win-clean.sh from WSL."
    exit 1
}

try {
    & cargo xwin --version *> $null
}
catch {
    Write-Error "cargo-xwin not found. Run: cargo install cargo-xwin"
    exit 1
}

Write-Host "==> Ensure target: $Target"
& rustup target add $Target *> $null

Write-Host "==> Clean: rust target directory"
Set-Location -LiteralPath $RustDir
& cargo clean
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

Write-Host "==> Build (release): $Target"
& cargo xwin build --release --target $Target
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

if ((Test-Path -LiteralPath $BuiltExePath) -and ($BuiltExePath.ToLowerInvariant() -ne $ExePath.ToLowerInvariant())) {
    Copy-Item -LiteralPath $BuiltExePath -Destination $ExePath -Force
}

if (-not (Test-Path -LiteralPath $ExePath)) {
    Write-Error "Build finished but artifact not found: $ExePath"
    exit 1
}

Write-Host "==> Built: $ExePath"
