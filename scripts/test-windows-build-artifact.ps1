[CmdletBinding()]
param(
    [string]$ExePath
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common-win-gnu.ps1')

if (-not $ExePath) {
    $ExePath = Join-Path $PSScriptRoot '..\rust\target\x86_64-pc-windows-gnu\release\FlistWalker.exe'
}
$exe = (Resolve-Path -LiteralPath $ExePath).Path
$fwExe = Join-Path (Split-Path -Parent $exe) 'fw.exe'
$alias = Join-Path (Split-Path -Parent $exe) 'flistwalker.exe'
if (-not (Test-Path -LiteralPath $alias -PathType Leaf)) {
    throw "Artifact alias is missing: $alias"
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $exe).Hash -ne
    (Get-FileHash -Algorithm SHA256 -LiteralPath $alias).Hash) {
    throw 'TC-147 executable aliases are not byte-identical.'
}
if (-not (Test-Path -LiteralPath $fwExe -PathType Leaf)) {
    throw "TC-193 CLI artifact is missing: $fwExe"
}

$mingwBin = Join-Path (Get-FlistWalkerMsys2Root) 'mingw64\bin'
$objdump = Resolve-FlistWalkerCommand `
    -Names @('objdump.exe', 'objdump') `
    -AbsoluteCandidates @((Join-Path $mingwBin 'objdump.exe'))
$strings = Resolve-FlistWalkerCommand `
    -Names @('strings.exe', 'strings') `
    -AbsoluteCandidates @((Join-Path $mingwBin 'strings.exe'))
if (-not $objdump -or -not $strings) {
    throw 'TC-147 requires MSYS2 objdump.exe and strings.exe.'
}

foreach ($artifact in @($exe, $fwExe)) {
    $sections = & $objdump -h $artifact
    if ($LASTEXITCODE -ne 0 -or ($sections -join "`n") -notmatch '\.rsrc') {
        throw "TC-147/TC-193 .rsrc section is missing: $artifact"
    }

    $pe = & $objdump -p $artifact
    $peText = $pe -join "`n"
    if ($LASTEXITCODE -ne 0 -or $peText -notmatch 'Subsystem\s+00000003\s+\(Windows CUI\)') {
        throw "TC-147/TC-193 Windows console subsystem is missing: $artifact"
    }
    $dlls = $pe | Select-String 'DLL Name:' | ForEach-Object { $_.Line.Trim() }
    if ($dlls -match 'msys-|mingw|libgcc|libstdc\+\+|libwinpthread') {
        throw "TC-147/TC-193 unexpected MSYS2 runtime DLL dependency: $($dlls -join ', ')"
    }
    # Shell32/User32 remain intentional for CLI open/reveal parity. Reject the
    # GUI framework, renderer, input-method, and window-compositor imports.
    if ($artifact -eq $fwExe -and $dlls -match 'GDI32|OPENGL32|imm32|psapi|dwmapi|uxtheme') {
        throw "TC-193 fw.exe unexpectedly imports GUI framework/rendering/window DLLs: $($dlls -join ', ')"
    }

    $text = & $strings -a $artifact
    if ($LASTEXITCODE -ne 0 -or ($text -join "`n") -notmatch 'requestedExecutionLevel level="asInvoker"') {
        throw "TC-147/TC-193 asInvoker manifest is missing: $artifact"
    }
}

Write-Host 'Windows build artifact validation passed.'
